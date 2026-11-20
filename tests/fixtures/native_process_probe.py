"""Owned Windows Job Object test fixture; never run against production data."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def main():
    if len(sys.argv) != 2:
        return 92
    config = Path(sys.argv[1]).resolve(strict=True)
    root = Path.cwd().resolve(strict=True)
    if config != root / "config.json":
        return 93
    with config.open(encoding="utf-8") as stream:
        data = json.load(stream)
    mode = data.get("mode")
    if mode not in {"clean", "delayed_exit", "failure", "cooperative", "stubborn"}:
        return 94
    evidence = Path(data.get("evidence", ""))
    if evidence.resolve() != root / "pids.json" or sys.stdin.buffer.readline().rstrip(b"\r\n") != b"GO":
        return 95
    seconds = 0.18 if mode in {"clean", "delayed_exit"} else 12
    child = subprocess.Popen(
        [sys.executable, "-I", "-S", "-B", "-c", "import sys,time;time.sleep(float(sys.argv[1]))", str(seconds)],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        cwd=root, close_fds=True,
    )
    pending = root / "pids.pending"
    with pending.open("x", encoding="utf-8") as stream:
        json.dump({"root_pid": os.getpid(), "child_pid": child.pid}, stream)
    os.replace(pending, evidence)
    if mode == "clean":
        return child.wait(timeout=2)
    if mode == "delayed_exit":
        return 0
    if mode == "failure":
        return 7
    if mode == "cooperative":
        if sys.stdin.buffer.readline().rstrip(b"\r\n") != b"STOP":
            return 96
        child.terminate()
        child.wait(timeout=2)
        return 0
    time.sleep(12)
    return child.wait(timeout=2)


if __name__ == "__main__":
    raise SystemExit(main())