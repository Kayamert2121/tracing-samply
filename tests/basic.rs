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

        let contents = std::fs::read_to_string(&path).unwrap();
        eprintln!("--- {fname:?} ---\n{contents}");

        let mut lines = contents.lines();
        let mut next = || lines.next().unwrap();
        assert!(next().ends_with(" info_span"));
        assert!(next().ends_with(" other_info_span"));
        assert!(next().ends_with(" info_span"));
        assert!(next().ends_with(" info_span"));
        assert!(next().ends_with(" info_span"));
        assert!(next().ends_with(" info_span"));
        assert!(next().ends_with(" spanned"));
        assert_eq!(lines.next(), None);
        assert_eq!(contents.lines().count(), 7);

        assert!(contents.ends_with("\n"));
    }
    assert_eq!(count, 2);
}

#[tracing::instrument]
fn spanned(arg: usize) {
    tracing::info!("arg: {}", arg);
    let sp = tracing::info_span!("info_span");
    let other_sp = tracing::info_span!("other_info_span");
    let other_sp_guard = other_sp.enter();
    sp.in_scope(|| {
        sp.in_scope(|| {}); // 1
        let sp2 = sp.clone(); // Same ID
        sp2.in_scope(|| {
            drop(other_sp_guard); // 2; out of order exit
            sp2.in_scope(|| {}); // 3
        }); // 4
    }); // 5
    sp.in_scope(|| {}); // 6
    drop(sp);
    tracing::debug_span!("debug_span").in_scope(|| {});
} // 7
