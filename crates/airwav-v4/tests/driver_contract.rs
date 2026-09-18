//! A separate test-only C ABI fixture exercises the real Rust FFI and worker.
use airwav_core::{BLOCK_BYTES, ReceiverConfig};
use airwav_v4::V4Driver;
use std::{process::Command, sync::atomic::Ordering, time::Duration};
#[test]
fn driver_contract_and_stream_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test-only-librtlsdr.so");
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test_driver.c");
    assert!(
        Command::new("cc")
            .args(["-shared", "-fPIC", "-std=c11", "-D_POSIX_C_SOURCE=200809L"])
            .arg(source)
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    // SAFETY: this is the test library compiled above, with an exact C signature.
    let control = unsafe { libloading::Library::new(&path).unwrap() };
    // SAFETY: exported by the test fixture; retained Library outlives this symbol.
    let mode = unsafe {
        control
            .get::<unsafe extern "C" fn(i32)>(b"airwav_test_mode\0")
            .unwrap()
    };
    let driver = V4Driver::load(Some(&path)).unwrap();
    let config = ReceiverConfig::default();
    for invalid in [1, 2, 3, 7, 8] {
        // SAFETY: no receiver worker exists while setting fixture state.
        unsafe {
            mode(invalid);
        }
        assert!(
            driver.open(&config).is_err(),
            "mode {invalid} must fail closed"
        );
    }
    // SAFETY: no live worker.
    unsafe {
        mode(0);
    }
    let bad_gain = ReceiverConfig {
        gain_tenth_db: Some(123),
        ..config.clone()
    };
    assert!(driver.open(&bad_gain).is_err());
    let mut stream = driver.open(&config).unwrap().start(16, false).unwrap();
    let mut next = 0;
    while let Ok(block) = stream.receiver.recv_timeout(Duration::from_secs(2)) {
        assert_eq!(block.first_sample, next);
        assert_eq!(block.bytes.len(), BLOCK_BYTES);
        next += block.samples();
    }
    stream.stop().unwrap();
    assert_eq!(next, 60 * (BLOCK_BYTES / 2) as u64);
    assert_eq!(stream.stats.snapshot().queue_dropped_samples, 0);
    drop(stream);
    let mut stream = driver.open(&config).unwrap().start(2, false).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    stream.stop().unwrap();
    let stats = stream.stats.snapshot();
    assert_eq!(stats.received_samples, 60 * (BLOCK_BYTES / 2) as u64);
    assert_eq!(stats.queue_dropped_samples, 58 * (BLOCK_BYTES / 2) as u64);
    assert_eq!(stream.receiver.len(), 2);
    assert_eq!(stats.hardware_lost_samples, None);
    drop(stream);
    // SAFETY: previous worker is joined before fixture mode changes.
    unsafe {
        mode(6);
    }
    let mut stream = driver.open(&config).unwrap().start(2, true).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    stream.stop().unwrap();
    assert_eq!(
        stream.stats.counter_discontinuities.load(Ordering::Relaxed),
        1
    );
    assert_eq!(
        stream
            .stats
            .counter_missing_bytes_lower_bound
            .load(Ordering::Relaxed),
        7
    );
    drop(stream);
    // SAFETY: previous worker is joined.
    unsafe {
        mode(5);
    }
    let mut stream = driver.open(&config).unwrap().start(16, false).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    assert!(stream.stop().unwrap_err().to_string().contains("code -4"));
    drop(stream);
    // SAFETY: previous worker is joined.
    unsafe {
        mode(4);
    }
    for _ in 0..30 {
        // Exercise cancel-before-read startup race and reopen.
        let mut stream = driver.open(&config).unwrap().start(2, false).unwrap();
        stream.stop().unwrap();
    }
}
