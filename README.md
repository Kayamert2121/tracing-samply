# tracing-samply

A [`tracing-subscriber`] layer that bridges [`tracing`] with [`samply`].

Currently, this only records spans as markers using `samply`'s ad-hoc marker file format,
which are then detected at runtime by `samply` and written to the profile.

Markers for a thread are only detected if `samply` is attached when the thread is created.

This crate will adapt to any changes upstream.
The creator has expressed that the marker file is a temporary measure and that it will be replaced with a more robust solution in the future: <https://github.com/mstange/samply/pull/143#issuecomment-2067747892>

See also: [mstange/samply#349](https://github.com/mstange/samply/issues/349)

## Usage

```rust
use tracing_subscriber::prelude::*;

fn main() {
    tracing_subscriber::registry()
        // ... other layers
        .with(tracing_samply::SamplyLayer::new().unwrap())
        .init();
    
    // Your application code that uses `tracing`
}
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

[`tracing`]: https://docs.rs/tracing
[`tracing-subscriber`]: https://docs.rs/tracing-subscriber
[`samply`]: https://github.com/mstange/samply
