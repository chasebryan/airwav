//! Doctor storage messaging integration check.
use std::{
    path::Path,
    process::{Command, Output},
};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_airwav"))
        .args(args)
        .env("AIRWAV_CONFIG", root.join("config.toml"))
        .env("AIRWAV_DATA_DIR", root.join("data"))
        .output()
        .unwrap()
}

#[test]
fn doctor_storage_check_reports_configured_minimum() {
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
    let storage = json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["check"] == "storage")
        .expect("storage check");
    assert_eq!(storage["status"], "PASS");
    assert!(
        storage["detail"]
            .as_str()
            .unwrap()
            .contains("configured minimum"),
        "{}",
        storage["detail"]
    );
}
