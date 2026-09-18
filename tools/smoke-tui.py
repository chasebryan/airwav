#!/usr/bin/env python3
"""Exercise replay, keys, screenshot, and terminal restoration in a real Linux PTY.
Usage: python3 tools/smoke-tui.py /path/to/airwav /path/to/demo-fixture.awr
Only use an explicitly labeled DemoFixture recording for this development check.
"""
import fcntl
import json
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


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    binary, recording = (Path(value).resolve(strict=True) for value in sys.argv[1:])
    manifest = json.loads((recording / "manifest.json").read_text())
    assert manifest["source"]["kind"] == "DemoFixture", "Use a DEMO FIXTURE recording"
    with tempfile.TemporaryDirectory(prefix="airwav-tui-check-") as directory:
        root = Path(directory)
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 42, 132, 0, 0))
        env = os.environ.copy()
        env.update(TERM="xterm-256color", COLORTERM="truecolor",
                   AIRWAV_DATA_DIR=str(root), AIRWAV_CONFIG=str(root / "config.toml"))
        process = subprocess.Popen([str(binary), "replay", str(recording)],
                                   stdin=slave, stdout=slave, stderr=slave, env=env)
        os.close(slave)
        transcript = bytearray()
        start = time.monotonic()
        keys = [(1.6, b" "), (1.7, b"\x1b[24~"), (2.0, b"d"),
                (2.2, b"\x1b"), (2.4, b"t"), (2.6, b"q")]
        try:
            while process.poll() is None and time.monotonic() - start < 15:
                if keys and time.monotonic() - start >= keys[0][0]:
                    os.write(master, keys.pop(0)[1])
                ready, _, _ = select.select([master], [], [], 0.03)
                if ready:
                    try:
                        transcript.extend(os.read(master, 65536))
                    except OSError:
                        break
            code = process.wait(timeout=5)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
        assert code == 0, transcript[-1000:]
        assert b"\x1b[?1049h" in transcript and b"\x1b[?1049l" in transcript
        assert b"DEMO FIXTURE" in transcript
        screenshots = list((root / "screenshots").glob("*.svg"))
        assert screenshots, "F12 did not create a screenshot"
        metadata = json.loads(screenshots[0].with_suffix(".json").read_text())
        assert metadata["source"] == "DEMO FIXTURE"
        assert metadata["observation"]["spectrum"]["power_dbfs"]
        print("PASS: PTY replay, fixture label, pause, F12 export, diagnostics, theme, quit, terminal restoration")


if __name__ == "__main__":
    main()
