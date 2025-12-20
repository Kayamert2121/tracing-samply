use tracing_subscriber::prelude::*;

#[test]
#[cfg_attr(windows, ignore = "todo")]
fn basic() {
    unsafe {
        std::env::set_var("RUST_LOG", "info");
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let dir = tmpdir.path();
    std::fs::remove_dir_all(dir).unwrap();
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_samply::SamplyLayer::builder().output_dir(dir).build().unwrap())
        .init();

    spanned(42);
    std::thread::spawn(|| {
        spanned(43);
    })
    .join()
    .unwrap();

    assert!(dir.exists());
    let pid = std::process::id().to_string();
    let process_dir = dir.join(&pid);
    assert!(process_dir.exists());
    let mut count = 0;
    for entry in process_dir.read_dir().unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let fname = path.file_name().unwrap().to_str().unwrap();
        assert!(fname.starts_with("marker-"), "{fname:?}");
        assert!(fname.contains(&pid), "{fname:?}");
        assert!(fname.ends_with(".txt"), "{fname:?}");
        count += 1;

        let contents = std::fs::read_to_string(path).unwrap();
        assert!(contents.contains("spanned\n"), "{contents:?}");
        assert!(contents.contains("info_span\n"), "{contents:?}");
        assert!(!contents.contains("debug_span\n"), "{contents:?}");
        assert!(contents.ends_with("\n"), "{contents:?}");
    }
    assert_eq!(count, 2);
}

#[tracing::instrument]
fn spanned(arg: usize) {
    tracing::info!("arg: {}", arg);
    tracing::info_span!("info_span").in_scope(|| {});
    tracing::debug_span!("debug_span").in_scope(|| {});
}
