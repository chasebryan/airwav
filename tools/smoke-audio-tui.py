#!/usr/bin/env python3
"""Test terminal audio with synthetic IQ and a test-only PCM sink.
Usage: python3 tools/smoke-audio-tui.py /path/to/airwav /path/to/demo.awr
No receiver or sound device is opened. The separate C ABI fixture is temporary;
it is never installed or linked into production.
"""
import fcntl
import json
import math
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time


def exercise(binary, args, root, fail=False, stall=False, rapid=False):
    root.mkdir()
    bins = root / "bin"
    bins.mkdir()
    script = (
        "import sys\nsys.stderr.write('test-player-device-unavailable\\n')\nsys.exit(3)\n" if fail else
        "import fcntl,time\ntry: fcntl.fcntl(0,fcntl.F_SETPIPE_SZ,4096)\nexcept OSError: pass\ntime.sleep(60)\n" if stall else
        "import os,time\nwith open(os.environ['AIRWAV_TEST_PCM'], 'ab', buffering=0) as f:\n while True:\n  b=os.read(0,4096)\n  if not b: break\n  f.write(b)\n  time.sleep(len(b)/96000)\n"
    )
    for name in ("pw-cat", "paplay", "aplay", "ffplay"):
        player = bins / name
        player.write_text(f"#!{sys.executable}\n" + script)
        player.chmod(0o755)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 42, 132, 0, 0))
    env = os.environ.copy()
    env.update(TERM="xterm-256color", COLORTERM="truecolor", PATH=str(bins)+os.pathsep+env.get("PATH", ""),
               AIRWAV_CONFIG=str(root / "config.toml"), AIRWAV_DATA_DIR=str(root), AIRWAV_TEST_PCM=str(root / "pcm.raw"))
    process = subprocess.Popen([str(binary), *args], stdin=slave, stdout=slave, stderr=slave,
                               env=env, start_new_session=True)
    os.close(slave)
    transcript = bytearray()

    def pump():
        ready, _, _ = select.select([master], [], [], 0.02)
        if ready:
            try:
                transcript.extend(os.read(master, 65536))
                # Bound retained output while keeping enough terminal state for failures.
                if len(transcript) > 2_000_000:
                    del transcript[1000:-1_000_000]
            except OSError:
                pass

    def observe(predicate, description, timeout=15):
        deadline = time.monotonic() + timeout
        next_capture = 0
        capture_pending = False
        last = {}
        while time.monotonic() < deadline:
            assert process.poll() is None, (description, transcript[-1500:])
            if not capture_pending and time.monotonic() >= next_capture:
                os.write(master, b"\x1b[24~")
                capture_pending = True
            pump()
            for path in sorted((root / "screenshots").glob("*.json")):
                try:
                    saved = json.loads(path.read_text())
                except (OSError, json.JSONDecodeError):
                    continue
                path.unlink()
                path.with_suffix(".svg").unlink(missing_ok=True)
                capture_pending = False
                next_capture = time.monotonic() + 0.2
                last = saved
                if predicate(saved):
                    return saved
        raise AssertionError((description, last.get("audio"), transcript[-1500:]))

    def playing(saved, mode=None, volume=None):
        state = saved["audio"]
        return (state["active"] and state["pcm_samples"] > 512
                and (mode is None or state["status"].startswith(mode + " "))
                and (volume is None or state["volume"] == volume))

    try:
        baseline = observe(lambda s: s["observation"] is not None, "first measured frame")
        if rapid:
            os.write(master, b"aa")
            observe(lambda s: not s["audio"]["active"] and s["audio"]["status"] == "Audio off • A Listen",
                    "two rapid Listen presses must return to off")
            # Keep all commands in one write, without waiting for the UI to refresh.
            os.write(master, b"a0m")
            saved = observe(lambda s: playing(s, "FM", 60), "rapid Listen/volume/mode must reach FM")
        elif stall:
            os.write(master, b"a")
            saved = observe(lambda s: s["audio"]["active"] and s["audio"]["flow"].startswith("Output stalled"),
                            "stalled output must be reported")
            assert saved["audio"]["dropped_samples"] > 0, saved["audio"]
            assert saved["observation"]["metrics"]["processed_samples"] > baseline["observation"]["metrics"]["processed_samples"]
        elif fail:
            os.write(master, b"a")
            observe(lambda s: not s["audio"]["active"] and "test-player-device-unavailable" in s["audio"]["status"],
                    "player failure must be visible")
            os.write(master, b"m0")
            saved = observe(lambda s: s["audio"]["mode"] == "FM" and s["audio"]["volume"] == 60,
                            "mode and volume remain usable after player failure")
        else:
            os.write(master, b"a")
            observe(lambda s: playing(s, "AM"), "AM PCM delivery")
            os.write(master, b"m0")
            observe(lambda s: playing(s, "FM", 60), "FM PCM delivery")
            os.write(master, b"a")
            observe(lambda s: not s["audio"]["active"] and s["audio"]["status"] == "Audio off • A Listen",
                    "mute must stop the player")
            os.write(master, b"a")
            saved = observe(lambda s: playing(s, "FM", 60), "Listen must restart PCM output")

        state = saved["audio"]
        if not fail and not stall:
            assert state["mode"] == "FM" and state["volume"] == 60, state
            pcm = root / "pcm.raw"
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline and (not pcm.exists() or pcm.stat().st_size <= 1024):
                assert process.poll() is None, (state, transcript[-1500:])
                pump()
            assert pcm.exists() and pcm.stat().st_size > 1024, state
            assert math.isfinite(state["rms_dbfs"]) and -120 <= state["rms_dbfs"] <= state["peak_dbfs"] <= 0, state
        os.write(master, b"q")
        deadline = time.monotonic() + 5
        while process.poll() is None and time.monotonic() < deadline:
            pump()
        pump()
        assert process.poll() == 0, ("quit/cleanup timed out", transcript[-1500:])
        assert b"\x1b[?1049h" in transcript and b"\x1b[?1049l" in transcript
        return state
    finally:
        # Keep a failed test from leaving its own stalled player behind.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        if process.poll() is None:
            process.wait()
        os.close(master)


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    binary, recording = (Path(v).resolve(strict=True) for v in sys.argv[1:])
    assert json.loads((recording / "manifest.json").read_text())["source"]["kind"] == "DemoFixture"
    with tempfile.TemporaryDirectory(prefix="airwav-audio-tui-") as directory:
        root = Path(directory)
        source = Path(__file__).resolve().parents[1] / "crates/airwav-v4/tests/fixtures/test_driver.c"
        text = source.read_text().replace("static int mode;", "static int mode = 4;")
        text = text.replace("struct timespec pause={0,1000000};", "struct timespec pause={0,13000000};")
        # One known mid-band AM carrier avoids selecting counter-pattern harmonics
        # at the IQ edge, where a broadcast FM filter correctly refuses to run.
        text = text.replace("#include <time.h>", "#include <time.h>\n#include <math.h>")
        payload = "for(uint32_t i=0;i<len;i++) b[i]=v++;"
        assert text.count(payload) == 1, "C ABI fixture payload changed"
        text = text.replace(payload, """
        for(uint32_t i=0;i<len;i+=2) {
            double t=((uint64_t)block*(len/2)+i/2)/(double)d->rate;
            double amplitude=0.4*(1.0+0.3*cos(6.283185307179586*1000.0*t));
            double phase=6.283185307179586*200000.0*t;
            b[i]=(uint8_t)(127.5+128.0*amplitude*cos(phase));
            b[i+1]=(uint8_t)(127.5+128.0*amplitude*sin(phase));
        }
        """)
        stub = root / "test-only-driver.c"
        stub.write_text(text)
        library = root / "test-only-driver.so"
        subprocess.run(["cc", "-O2", "-shared", "-fPIC", "-std=c11", "-D_POSIX_C_SOURCE=200809L", str(stub), "-lm", "-o", str(library)], check=True)
        exercise(binary, ["--library", str(library)], root / "rapid-controls", rapid=True)
        exercise(binary, ["replay", str(recording)], root / "replay")
        exercise(binary, ["replay", str(recording)], root / "failure", fail=True)
        exercise(binary, ["--library", str(library)], root / "stream")
        exercise(binary, ["--library", str(library)], root / "stalled-stream", stall=True)
    print("PASS: terminal ordered controls, AM/FM, volume, PCM levels, player errors/stalls, IQ progress, quit/restoration")


if __name__ == "__main__":
    main()
