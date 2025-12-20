#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

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
    output_dir: Option<Box<Path>>,
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
        self.output_dir = Some(dir.into().into());
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
        std::fs::create_dir_all(&dir)
            .map_err(map_io_err("could not create perf markers dir", &dir))?;
        Ok(SamplyLayer { dir: dir.into_boxed_path() })
    }
}

/// A tracing layer that bridges `tracing` events and spans with `samply`.
///
/// See the [crate docs](crate) for more information.
pub struct SamplyLayer {
    dir: Box<Path>,
}

struct SpanData {
    start_ns: u64,
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
        #[cfg(unix)]
        let _ = unsafe {
            memmap2::MmapOptions::new()
                .map(&file)
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
        let Some(span) = ctx.span(id) else { return };
        let start_ns = now_timestamp();
        span.extensions_mut().insert(SpanData { start_ns });
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let extensions = span.extensions();
        let Some(data) = extensions.get::<SpanData>() else { return };
        let end_ns = now_timestamp();
        let line = format!("{} {} {}\n", data.start_ns, end_ns, span.name());
        let _ = MARKER_FILE.with_borrow_mut(|file| {
            file.get_or_insert_with(|| self.create_marker_file()).write_all(line.as_bytes())
        });
    }
}

fn now_timestamp() -> u64 {
    cfg_if::cfg_if! {
        if #[cfg(unix)] {
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

fn gettid() -> Option<i32> {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "linux")] {
            Some(unsafe { libc::gettid() })
        } else if #[cfg(target_vendor = "apple")] {
            // TODO
            None
        } else {
            None
        }
    }
}

fn map_io_err(s: &str, p: &Path) -> impl FnOnce(io::Error) -> io::Error {
    move |e| io::Error::new(e.kind(), format!("{s} {p:?}: {e}"))
}
