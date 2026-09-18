#!/usr/bin/env python3
"""Test terminal audio end-to-end using only synthetic IQ and a test-only PCM sink.
Usage: python3 tools/smoke-audio-tui.py /path/to/airwav /path/to/demo.awr
No receiver or sound device is opened. The separate C ABI fixture is compiled
into a temporary directory; it is never installed or linked into production.
"""
import fcntl
import json
import math
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time


def exercise(binary, args, root, fail=False, stall=False):
    root.mkdir()
    bins = root / "bin"
    bins.mkdir()
    player = bins / "pw-cat"
    player.write_text(f"#!{sys.executable}\n" + (
        "import sys\nsys.stderr.write('test-player-device-unavailable\\n')\nsys.exit(3)\n" if fail else
        "import fcntl,time\nfcntl.fcntl(0,fcntl.F_SETPIPE_SZ,4096)\ntime.sleep(60)\n" if stall else
        "import os\nwith open(os.environ['AIRWAV_TEST_PCM'], 'ab', buffering=0) as f:\n while True:\n  b=os.read(0,4096)\n  if not b: break\n  f.write(b)\n"
    ))
    player.chmod(0o755)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 42, 132, 0, 0))
    env = os.environ.copy()
    env.update(TERM="xterm-256color", COLORTERM="truecolor", PATH=str(bins)+os.pathsep+env.get("PATH", ""),
               AIRWAV_CONFIG=str(root / "config.toml"), AIRWAV_DATA_DIR=str(root), AIRWAV_TEST_PCM=str(root / "pcm.raw"))
    process = subprocess.Popen([str(binary), *args], stdin=slave, stdout=slave, stderr=slave, env=env)
    os.close(slave)
    keys = [(0.5, b"a"), (0.95, b"m"), (1.15, b"0"), (1.35, b"\x1b[24~"), (1.6, b"a"), (1.8, b"a"), (2.1, b"q")]
    if stall:
        keys = [(0.5, b"a"), (0.8, b"\x1b[24~"), (2.5, b"\x1b[24~"), (2.8, b"q")]
    transcript = bytearray()
    start = time.monotonic()
    try:
        while process.poll() is None and time.monotonic()-start < 15:
            if keys and time.monotonic()-start >= keys[0][0]:
                os.write(master, keys.pop(0)[1])
            ready, _, _ = select.select([master], [], [], 0.02)
            if ready:
                try:
                    transcript.extend(os.read(master, 65536))
                except OSError:
                    break
        code = process.wait(timeout=3)
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
    assert code == 0, transcript[-1500:]
    assert b"\x1b[?1049h" in transcript and b"\x1b[?1049l" in transcript
    metadata = sorted((root / "screenshots").glob("*.json"))
    assert metadata, "Audio controls prevented screenshot/terminal input"
    saved = json.loads(metadata[-1].read_text())
    audio = saved["audio"]
    assert audio["mode"] == ("AM" if stall else "FM") and audio["volume"] == (50 if stall else 60), audio
    if fail:
        assert not audio["active"] and "test-player-device-unavailable" in audio["status"], audio
    elif stall:
        assert audio["active"] and audio["flow"].startswith("Output stalled"), audio
        assert audio["dropped_samples"] > 0, audio
        assert len(metadata) == 2, "Stalled output blocked screenshot input"
        earlier = json.loads(metadata[0].read_text())["observation"]["metrics"]["processed_samples"]
        assert saved["observation"]["metrics"]["processed_samples"] > earlier, "Stalled output blocked IQ processing"
    else:
        assert (root / "pcm.raw").stat().st_size > 1024, audio
        assert audio["pcm_samples"] > 512, audio
        assert math.isfinite(audio["rms_dbfs"]) and -120 <= audio["rms_dbfs"] <= audio["peak_dbfs"] <= 0, audio
    return audio


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    binary, recording = (Path(v).resolve(strict=True) for v in sys.argv[1:])
    assert json.loads((recording / "manifest.json").read_text())["source"]["kind"] == "DemoFixture"
    with tempfile.TemporaryDirectory(prefix="airwav-audio-tui-") as directory:
        root = Path(directory)
        exercise(binary, ["replay", str(recording)], root / "replay")
        exercise(binary, ["replay", str(recording)], root / "failure", fail=True)
        source = Path(__file__).resolve().parents[1] / "crates/airwav-v4/tests/fixtures/test_driver.c"
        text = source.read_text().replace("static int mode;", "static int mode = 4;")
        text = text.replace("struct timespec pause={0,1000000};", "struct timespec pause={0,13000000};")
        stub = root / "test-only-driver.c"
        stub.write_text(text)
        library = root / "test-only-driver.so"
        subprocess.run(["cc", "-shared", "-fPIC", "-std=c11", "-D_POSIX_C_SOURCE=200809L", str(stub), "-o", str(library)], check=True)
        exercise(binary, ["--library", str(library)], root / "stream")
        exercise(binary, ["--library", str(library)], root / "stalled-stream", stall=True)
    print("PASS: terminal Listen/Mute, AM/FM mode, volume, PCM delivery/levels, player errors/stalls, ABI streaming, quit/restoration")


if __name__ == "__main__":
    main()
