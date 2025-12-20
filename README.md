# tracing-perf-markers

A [`tracing`] subscriber layer that records span timings and writes them to a file for profiler integration.

## Overview

This crate provides `PerfMarkersLayer`, which:

1. Records the start and end timestamps (in nanoseconds) of each tracing span
2. Writes them to a file in the format `<start_ns> <end_ns> <name>`
3. Memory-maps the file on drop so profilers like [`samply`] can pick it up

## Usage

```rust
use tracing_perf_markers::PerfMarkersLayer;
use tracing_subscriber::prelude::*;

fn main() -> std::io::Result<()> {
    let (layer, _guard) = PerfMarkersLayer::new("spans.txt")?;
    tracing_subscriber::registry().with(layer).init();

    // Your application code with tracing spans
    do_work();

    Ok(())
}

#[tracing::instrument]
fn do_work() {
    // ...
}
```

## Output Format

Each line in the output file contains:

```
<start_ns> <end_ns> <span_name>
```

Where timestamps are nanoseconds elapsed since the layer was created.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

[`tracing`]: https://docs.rs/tracing
[`samply`]: https://github.com/mstange/samply
