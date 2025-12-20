#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
};
use tracing_core::{Subscriber, span};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

/// A tracing layer that records span timings to a file.
///
/// See the [crate docs](self) for more information.
pub struct PerfMarkersLayer {
    file: File,
}

struct SpanData {
    start_ns: u64,
}

impl PerfMarkersLayer {
    /// Creates a new layer and guard, writing to the given file path.
    ///
    /// `dir` will contain all the marker files.
    /// If `None`, a temporary directory will be created.
    pub fn new(dir: Option<&Path>) -> io::Result<Self> {
        let path = match dir {
            Some(path) => path,
            None => {
                let dir = std::env::temp_dir().join("tracing-perf-markers");
                std::fs::create_dir_all(&dir)
                    .map_err(map_io_err("could not create perf markers tmp dir", &dir))?;
                let pid = std::process::id();
                &dir.join(format!("marker-{pid}.txt"))
            }
        };
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(map_io_err("could not create perf markers file", &path))?;
        // mmap the file to notify samply/perf.
        #[cfg(unix)]
        let _ = unsafe {
            memmap2::MmapOptions::new()
                .map(&file)
                .map_err(map_io_err("could not mmap perf markers file", &path))?
        };
        Ok(Self { file })
    }
}

impl<S> Layer<S> for PerfMarkersLayer
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
        let _ = (&self.file).write_all(line.as_bytes());
    }
}

fn map_io_err(s: &str, p: &Path) -> impl FnOnce(io::Error) -> io::Error {
    move |e| io::Error::new(e.kind(), format!("{s} {p:?}: {e}"))
}

fn now_timestamp() -> u64 {
    #[cfg(unix)]
    {
        let mut ts = unsafe { std::mem::zeroed() };
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } != 0 {
            return u64::MAX;
        }
        std::time::Duration::new(ts.tv_sec as _, ts.tv_nsec as _)
            .as_nanos()
            .try_into()
            .unwrap_or(u64::MAX)
    }
    #[cfg(not(unix))]
    {
        0
    }
}
