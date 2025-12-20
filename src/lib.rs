#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use smallvec::{SmallVec, smallvec};
use std::{
    cell::RefCell,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use tracing_core::{Subscriber, span};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

thread_local! {
    static MARKER_FILE: RefCell<Option<File>> = const { RefCell::new(None) };
}

/// [`SamplyLayer`] builder.
///
/// See the [crate docs](crate) for more information.
pub struct SamplyLayerBuilder {
    output_dir: Option<PathBuf>,
}

impl Default for SamplyLayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplyLayerBuilder {
    /// Creates a new [`SamplyLayerBuilder`].
    pub fn new() -> Self {
        Self { output_dir: None }
    }

    /// Sets the output directory for intermediate files.
    ///
    /// If unset, a temporary directory will be created and used.
    pub fn output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }

    /// Builds a new [`SamplyLayer`].
    pub fn build(self) -> io::Result<SamplyLayer> {
        let Self { output_dir } = self;
        let dir = match &output_dir {
            Some(dir) => dir,
            None => &*std::env::temp_dir().join("tracing-samply"),
        };
        let dir = dir.join(std::process::id().to_string());
        if cfg!(unix) {
            std::fs::create_dir_all(&dir)
                .map_err(map_io_err("could not create perf markers dir", &dir))?;
        }
        Ok(SamplyLayer { dir: dir.into_boxed_path() })
    }
}

/// A tracing layer that bridges `tracing` events and spans with `samply`.
///
/// See the [crate docs](crate) for more information.
pub struct SamplyLayer {
    dir: Box<Path>,
}

struct SpanDataStack {
    stack: SmallVec<[SpanData; 1]>,
}
struct SpanData {
    start_ts: u64,
}

impl SamplyLayer {
    /// Creates a new [`SamplyLayer`].
    ///
    /// This is the same as `SamplyLayer::builder().build()`.
    pub fn new() -> io::Result<Self> {
        Self::builder().build()
    }

    /// Creates a new [`SamplyLayer`] builder.
    pub fn builder() -> SamplyLayerBuilder {
        SamplyLayerBuilder::new()
    }

    fn create_marker_file(&self) -> File {
        match self.try_create_marker_file() {
            Ok(file) => file,
            Err(err) => panic!("{err}"),
        }
    }

    fn try_create_marker_file(&self) -> io::Result<File> {
        let pid = std::process::id();
        let fname = match gettid() {
            Some(tid) => format!("marker-{pid}-{tid}.txt"),
            None => format!("marker-{pid}.txt"),
        };
        let path = &*self.dir.join(fname);
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)
            .map_err(map_io_err("could not create perf markers file", path))?;
        // mmap the file to notify samply.
        // Linux perf needs `exec` permission to record it in perf.data.
        // On macOS, samply only needs the file to be opened, not mmap'ed.
        #[cfg(all(unix, not(target_vendor = "apple")))]
        let _ = unsafe {
            memmap2::MmapOptions::new()
                .map_exec(&file)
                .map_err(map_io_err("could not mmap perf markers file", path))?
        };
        Ok(file)
    }
}

impl<S> Layer<S> for SamplyLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        if !cfg!(unix) {
            return;
        }
        let Some(span) = ctx.span(id) else { return };
        let data = SpanData { start_ts: now_timestamp() };
        let mut extensions = span.extensions_mut();
        if let Some(stack) = extensions.get_mut::<SpanDataStack>() {
            stack.stack.push(data);
        } else {
            extensions.insert(SpanDataStack { stack: smallvec![data] });
        }
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        if !cfg!(unix) {
            return;
        }
        let Some(span) = ctx.span(id) else { return };
        let mut extensions = span.extensions_mut();
        let Some(data) = extensions.get_mut::<SpanDataStack>() else { return };
        let Some(SpanData { start_ts }) = data.stack.pop() else { return };
        let end_ts = now_timestamp();
        MARKER_FILE.with_borrow_mut(|file| {
            let file = file.get_or_insert_with(|| self.create_marker_file());
            let line = format!("{start_ts} {end_ts} {}\n", span.name());
            let _ = file.write_all(line.as_bytes());
        });
    }
}

fn now_timestamp() -> u64 {
    cfg_if::cfg_if! {
        if #[cfg(target_vendor = "apple")] {
            // https://github.com/mstange/samply/blob/2041b956f650bb92d912990052967d03fef66b75/samply/src/mac/time.rs#L7
            use std::sync::OnceLock;
            use mach2::mach_time;

            static NANOS_PER_TICK: OnceLock<mach_time::mach_timebase_info> = OnceLock::new();

            let nanos_per_tick = NANOS_PER_TICK.get_or_init(|| unsafe {
                let mut info = mach_time::mach_timebase_info::default();
                let errno = mach_time::mach_timebase_info(&mut info as *mut _);
                if errno != 0 || info.denom == 0 {
                    info.numer = 1;
                    info.denom = 1;
                };
                info
            });

            let time = unsafe { mach_time::mach_absolute_time() };

            time * nanos_per_tick.numer as u64 / nanos_per_tick.denom as u64
        } else if #[cfg(unix)] {
            let mut ts = unsafe { std::mem::zeroed() };
            if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } != 0 {
                return u64::MAX;
            }
            std::time::Duration::new(ts.tv_sec as _, ts.tv_nsec as _)
                .as_nanos()
                .try_into()
                .unwrap_or(u64::MAX)
        } else {
            0
        }
    }
}

fn gettid() -> Option<u64> {
    // https://github.com/rust-lang/rust/blob/9044e98b66d074e7f88b1d4cea58bb0538f2eda6/library/std/src/sys/thread/unix.rs#L325
    cfg_if::cfg_if! {
        if #[cfg(target_vendor = "apple")] {
            let mut tid = 0u64;
            let status = unsafe { libc::pthread_threadid_np(0, &mut tid) };
            (status == 0).then_some(tid)
        } else if #[cfg(unix)] {
            Some(unsafe { libc::gettid() } as u64)
        // } else if #[cfg(windows)] {
        //     let tid = unsafe { c::GetCurrentThreadId() } as u64;
        //     if tid == 0 { None } else { Some(tid as _) }
        } else {
            None
        }
    }
}

fn map_io_err(s: &str, p: &Path) -> impl FnOnce(io::Error) -> io::Error {
    move |e| io::Error::new(e.kind(), format!("{s} {p:?}: {e}"))
}
