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
        frames: vec![],
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

fn audio_fixture(path: &Path, mode: &str) {
    let config = Config {
        post_trigger_seconds: 0,
        minimum_free_bytes: 0,
        ..Config::default()
    };
    let mut writer = Writer::create(
        path,
        &config,
        Source::DemoFixture {
            description: "DEMO FIXTURE: generated 1 kHz audio".into(),
        },
    )
    .unwrap();
    let rate = config.receiver.sample_rate;
    let offset = 160_000.;
    let mut bytes = Vec::new();
    for i in 0..rate / 10 {
        let time = i as f64 / rate as f64;
        let phase = std::f64::consts::TAU * 1000. * time;
        let carrier = std::f64::consts::TAU * offset * time;
        let (amplitude, angle) = match mode {
            "am" => (0.4 * (1. + 0.5 * phase.cos()), carrier),
            "fm" => (0.6, carrier + 40. * phase.sin()),
            _ => (0.6, carrier + 1.5 * phase.sin()),
        };
        bytes.extend([
            (127.5 + 128. * amplitude * angle.cos()).round() as u8,
            (127.5 + 128. * amplitude * angle.sin()).round() as u8,
        ]);
    }
    let block = Arc::new(IqBlock {
        first_sample: 0,
        received_ns: 10,
        bytes,
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
        frames: vec![],
    };
    writer.append_snapshot(&snapshot).unwrap();
    let samples = block.samples();
    writer.begin_event(snapshot, &[block], samples).unwrap();
    writer.finish().unwrap();
}

#[test]
fn am_fm_and_nfm_export_playable_wav_with_embedded_evidence() {
    for mode in ["am", "fm", "nfm"] {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("audio.awr");
        let output = root.path().join("audio.wav");
        audio_fixture(&input, mode);
        let args = [
            "--library",
            "/does/not/exist.so",
            "audio",
            input.to_str().unwrap(),
            "--mode",
            mode,
            "--frequency-hz",
            "136160000",
            "--output",
            output.to_str().unwrap(),
        ];
        let result = run(root.path(), &args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let metadata: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(metadata["mode"], mode);
        assert_eq!(metadata["event_complete"], true);
        assert_eq!(metadata["source_label"], "DEMO FIXTURE");
        assert_eq!(metadata["clipped_samples"], 0);
        let wav = std::fs::read(&output).unwrap();
        let u32_at = |offset| u32::from_le_bytes(wav[offset..offset + 4].try_into().unwrap());
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32_at(4) as usize + 8, wav.len());
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(&wav[20..24], &[1, 0, 1, 0]); // mono PCM
        assert_eq!(u32_at(24), 48_000);
        assert_eq!(&wav[34..36], &[16, 0]);
        assert_eq!(&wav[36..40], b"data");
        let samples = u32_at(40) as usize / 2;
        assert!((samples as i32 - 4800).abs() <= 1);
        let pcm: Vec<_> = wav[44..44 + samples * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f64 / 32768.)
            .collect();
        let pcm = &pcm[960..];
        let (re, im) = pcm.iter().enumerate().fold((0., 0.), |(re, im), (i, s)| {
            let phase = std::f64::consts::TAU * 1000. * i as f64 / 48_000.;
            (re + s * phase.cos(), im + s * phase.sin())
        });
        assert!(
            2. * re.hypot(im) / pcm.len() as f64 > 0.25,
            "{mode}: missing recovered tone"
        );
        let info = 44 + samples * 2;
        assert_eq!(&wav[info..info + 4], b"LIST");
        assert_eq!(&wav[info + 8..info + 16], b"INFOICMT");
        let len = u32_at(info + 16) as usize;
        let embedded: serde_json::Value =
            serde_json::from_slice(&wav[info + 20..info + 20 + len - 1]).unwrap();
        assert_eq!(metadata, embedded);
        // Re-running never destroys a user's completed file.
        assert!(!run(root.path(), &args).status.success());
        assert_eq!(std::fs::read(&output).unwrap(), wav);
    }
}

#[test]
fn audio_rejects_corrupt_iq_invalid_channels_and_recording_mutation() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("audio.awr");
    fixture(&input);
    let output = root.path().join("audio.wav");
    let base = [
        "audio",
        input.to_str().unwrap(),
        "--mode",
        "am",
        "--output",
        output.to_str().unwrap(),
    ];
    for extra in [
        ["--frequency-hz", "1"],
        ["--gain", "NaN"],
        ["--event", "missing"],
        ["--deemphasis-us", "75"],
    ] {
        let mut args = base.to_vec();
        args.extend(extra);
        assert!(!run(root.path(), &args).status.success());
        assert!(!output.exists());
    }
    let inside = input.join("derived.wav");
    assert!(
        !run(
            root.path(),
            &[
                "audio",
                input.to_str().unwrap(),
                "--mode",
                "am",
                "--output",
                inside.to_str().unwrap()
            ]
        )
        .status
        .success()
    );
    assert!(!inside.exists());
    std::fs::write(input.join("iq/AW-EVENT-000001.cu8"), vec![129; 65536]).unwrap();
    let result = run(root.path(), &base);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("BLAKE3"));
    assert!(!output.exists());
}

#[test]
fn audio_requires_explicit_selection_for_multiple_events_and_can_export_partial_iq() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("events.awr");
    fixture(&path);
    let event_path = path.join("events.jsonl");
    let row = std::fs::read_to_string(&event_path).unwrap();
    let mut second: serde_json::Value = serde_json::from_str(row.trim()).unwrap();
    second["id"] = "AW-EVENT-000002".into();
    second["complete"] = false.into();
    std::fs::write(
        &event_path,
        format!("{row}{}\n", serde_json::to_string(&second).unwrap()),
    )
    .unwrap();
    let inspect = run(root.path(), &["inspect", path.to_str().unwrap()]);
    assert!(inspect.status.success());
    let index: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(index["event_index"][1]["id"], "AW-EVENT-000002");
    let output = root.path().join("selected.wav");
    let mut args = vec![
        "audio",
        path.to_str().unwrap(),
        "--mode",
        "am",
        "--output",
        output.to_str().unwrap(),
    ];
    let ambiguous = run(root.path(), &args);
    assert!(!ambiguous.status.success());
    assert!(!output.exists());
    args.extend(["--event", "AW-EVENT-000002"]);
    let result = run(root.path(), &args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["event_complete"], false);
    assert_eq!(json["event_id"], "AW-EVENT-000002");
}

#[cfg(unix)]
#[test]
fn optional_audio_playback_passes_exact_path_and_preserves_wav_on_player_failure() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let recording = root.path().join("source.awr");
    fixture(&recording);
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let player = bin.join("pw-play");
    // A test-only player confirms argument handling without opening an audio device.
    std::fs::write(
        &player,
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$PLAYER_ARGUMENT\"\nexit \"$PLAYER_EXIT\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&player, std::fs::Permissions::from_mode(0o755)).unwrap();
    for exit in ["0", "7"] {
        let output = root.path().join(format!("audio with spaces {exit}.wav"));
        let argument = root.path().join(format!("argument-{exit}"));
        let result = Command::new(env!("CARGO_BIN_EXE_airwav"))
            .args([
                "audio",
                recording.to_str().unwrap(),
                "--mode",
                "am",
                "--output",
                output.to_str().unwrap(),
                "--play",
            ])
            .env("AIRWAV_CONFIG", root.path().join("config.toml"))
            .env("AIRWAV_DATA_DIR", root.path().join("data"))
            .env("PATH", &bin)
            .env("PLAYER_ARGUMENT", &argument)
            .env("PLAYER_EXIT", exit)
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            exit == "0",
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(output.is_file());
        assert_eq!(
            std::fs::read_to_string(&argument).unwrap(),
            output.to_str().unwrap()
        );
    }
}
