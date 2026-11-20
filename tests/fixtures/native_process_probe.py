"""Qualification fixture only. No model, media, network, queue or producer."""
import sys
if sys.stdin.buffer.readline() != b"GO\n":
    raise SystemExit(20)
import json, os, pathlib, subprocess, time
config = json.loads(pathlib.Path(sys.argv[1]).read_text())
mode = config["mode"]
if mode not in ("clean", "delayed_exit", "failure", "cooperative", "stubborn"):
    raise SystemExit(21)
child = subprocess.Popen([sys.executable, "-I", "-S", "-B", "-c",
                          "import time; time.sleep(.15)" if mode == "delayed_exit" else "import time; time.sleep(60)"],
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, creationflags=0x08000000)
pathlib.Path(config["evidence"]).write_text(json.dumps({"parent": os.getpid(), "child": child.pid}))
if mode == "delayed_exit":
    raise SystemExit(0) # descendant exits naturally inside the existing 500ms bound
if mode == "clean":
    child.terminate(); child.wait(timeout=2); raise SystemExit(0)
if mode == "failure":
    raise SystemExit(7)  # Job must clean the deliberately retained descendant.
if mode == "cooperative":
    if sys.stdin.buffer.readline() == b"STOP\n":
        child.terminate(); child.wait(timeout=2); raise SystemExit(0)
time.sleep(60)  # Job termination must contain this owned tree on deadline.
