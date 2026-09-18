use airwav_core::{Config, IqBlock, Metrics, Snapshot};
use airwav_dsp::SpectrumEngine;
use airwav_record::{Source, Writer};
use std::{
    path::Path,
    process::{Command, Output},
    sync::Arc,
};
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_airwav"))
        .args(args)
        .env("AIRWAV_CONFIG", root.join("config.toml"))
        .env("AIRWAV_DATA_DIR", root.join("data"))
        .output()
        .unwrap()
}
fn fixture(path: &Path) {
    let config = Config {
        post_trigger_seconds: 0,
        minimum_free_bytes: 0,
        ..Config::default()
    };
    let mut writer = Writer::create(
        path,
        &config,
        Source::DemoFixture {
            description: "CLI integration test".into(),
        },
    )
    .unwrap();
    let block = Arc::new(IqBlock {
        first_sample: 0,
        received_ns: 10,
        bytes: vec![128; 65536],
    });
    let spectrum = SpectrumEngine::new(2048)
        .unwrap()
        .push(&block, &config.receiver)
        .unwrap()
        .unwrap();
    let snapshot = Snapshot {
        timestamp_ns: 10,
        receiver: config.receiver,
        spectrum,
        islands: vec![],
        metrics: Metrics::default(),
    };
    writer.append_snapshot(&snapshot).unwrap();
    writer.begin_event(snapshot, &[block], 32768).unwrap();
    writer.finish().unwrap();
}
#[test]
fn offline_inspect_replay_and_export_need_no_driver() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fixture.awr");
    fixture(&path);
    for command in ["inspect", "replay"] {
        let mut args = vec![
            "--library",
            "/does/not/exist.so",
            command,
            path.to_str().unwrap(),
        ];
        if command == "replay" {
            args.push("--headless");
        }
        let out = run(root.path(), &args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["events"], 1);
        assert_eq!(json["manifest"]["source"]["kind"], "DemoFixture");
    }
    let svg = root.path().join("screen.svg");
    let args = [
        "export",
        path.to_str().unwrap(),
        "--output",
        svg.to_str().unwrap(),
    ];
    assert!(run(root.path(), &args).status.success());
    assert!(std::fs::read_to_string(&svg).unwrap().contains("DEMO"));
    assert!(svg.with_extension("json").is_file());
    assert!(!run(root.path(), &args).status.success());
}
#[test]
fn doctor_missing_driver_is_structured_and_nonzero() {
    let root = tempfile::tempdir().unwrap();
    let out = run(
        root.path(),
        &[
            "--library",
            "/does/not/exist.so",
            "doctor",
            "--json",
            "--stream-seconds",
            "0",
        ],
    );
    assert!(!out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert!(
        json["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["check"] == "hardware" && c["status"] == "FAIL")
    );
}
#[test]
fn config_init_does_not_overwrite_and_invalid_values_fail() {
    let root = tempfile::tempdir().unwrap();
    assert!(run(root.path(), &["config", "--init"]).status.success());
    assert!(!run(root.path(), &["config", "--init"]).status.success());
    std::fs::write(root.path().join("config.toml"), "fft_size=13").unwrap();
    assert!(!run(root.path(), &["config"]).status.success());
}
#[test]
fn tiny_export_dimensions_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fixture.awr");
    fixture(&path);
    let svg = root.path().join("tiny.svg");
    assert!(
        !run(
            root.path(),
            &[
                "export",
                path.to_str().unwrap(),
                "--output",
                svg.to_str().unwrap(),
                "--width",
                "0"
            ]
        )
        .status
        .success()
    );
    assert!(!svg.exists());
}
