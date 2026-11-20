"""Local pair staging. Called only inside the APP's pinned, enrolled worker."""
import ctypes
import hashlib
import json
import os
import re
import subprocess
import uuid
from contextlib import ExitStack
from pathlib import Path

from local_producer import bounded, read_lease
from selected_staging_v3 import safe, exclusive_json, LIMIT

ROOT = Path(__file__).resolve().parents[3] / "snapmatic-local-pairs-v1"
TEXT = ("title", "description", "creator", "uploader", "id", "upload_date", "language")
COUNTS = ("view_count", "like_count", "comment_count", "repost_count", "duration")

def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def clean(value, limit):
    if not isinstance(value, str) or len(value) > limit:
        return None
    if re.search(r"(?i)(?:https?://|www\.|bearer\s|cookie\s*[:=]|(?:token|signature|password|secret|api[_-]?key)\s*[:=])", value):
        return None
    return "".join(c for c in value if c.isprintable() or c == "\n").strip()

def metadata(raw):
    if type(raw) is not dict:
        raise ValueError("metadata_shape")
    result = {}
    for key in TEXT:
        value = clean(raw.get(key), 4096 if key == "description" else 256)
        if value:
            result[key] = value
    for key in COUNTS:
        value = raw.get(key)
        if type(value) in (int, float) and 0 <= value <= 2**53:
            result[key] = value
    tags = raw.get("tags")
    if isinstance(tags, list):
        result["tags"] = [text for tag in tags[:64] if (text := clean(tag, 80))]
    for key in ("subtitles", "automatic_captions"):
        if isinstance(raw.get(key), dict):
            result[key + "_languages"] = sorted(k for k in raw[key] if re.fullmatch(r"[a-zA-Z]{2,3}(?:-[a-zA-Z0-9]{2,8}){0,2}", k))[:64]
    return result

def stage(job_id, pins, tools):
    member = ctypes.c_int()
    if not ctypes.windll.kernel32.IsProcessInJob(ctypes.c_void_p(-1), None, ctypes.byref(member)) or not member.value:
        raise ValueError("job_required")
    if str(uuid.UUID(job_id)) != job_id or set(pins) != {"accepted.mp4", "accepted.info.json", "ffmpeg", "ffprobe"} or set(tools) != {"ffmpeg", "ffprobe"}:
        raise ValueError("job_binding")
    root = safe(ROOT)
    source = safe(root / "inbox" / job_id)
    pending = safe(root / "tmp" / job_id)
    ready = safe(root / "ready" / job_id)
    if ready.exists():
        raise ValueError("already_ready")
    paths = {name: safe(source / name) for name in ("accepted.mp4", "accepted.info.json")}
    paths.update({k: safe(Path(v)) for k, v in tools.items()})
    with ExitStack() as leases:
        for key, path in paths.items():
            leases.enter_context(read_lease(path))
            size = path.stat().st_size
            if key.startswith("accepted.") and (path.stat().st_nlink != 1 or not 0 < size <= (LIMIT if key.endswith(".mp4") else 1048576)):
                raise ValueError("input_size_or_link")
            if digest(path) != pins[key]:
                raise ValueError("pin_mismatch")
        info = metadata(json.loads(paths["accepted.info.json"].read_bytes()))
        pending.mkdir()
        media = pending / "media.mp4"
        ff = str(paths["ffmpeg"])
        def probe(path):
            data = bounded([str(paths["ffprobe"]), "-v","error","-protocol_whitelist","file,pipe","-f","mov","-show_streams","-show_format","-of","json",str(path)], pending, 15)
            return json.loads(data)
        def streams(data):
            video = [s for s in data["streams"] if s["codec_type"] == "video" and not s.get("disposition", {}).get("attached_pic")]
            audio = [s for s in data["streams"] if s["codec_type"] == "audio"]
            if len(video) != 1 or len(audio) != 1 or not 0 < float(data["format"]["duration"]) <= 900 or not 0 < video[0]["width"] * video[0]["height"] <= 16777216:
                raise ValueError("media_layout")
            return video[0], audio[0]
        src = paths["accepted.mp4"]
        v, a = streams(probe(src))
        args = [ff,"-nostdin","-v","error","-xerror","-protocol_whitelist","file,pipe","-enable_drefs","0","-f","mov","-i",str(src),"-map",f"0:{v['index']}","-map",f"0:{a['index']}","-map_metadata","-1","-map_chapters","-1","-c:v","copy" if v["codec_name"] == "h264" and v["pix_fmt"] == "yuv420p" else "libx264","-c:a","copy" if a["codec_name"] == "aac" else "aac"]
        if args[args.index("-c:v")+1] != "copy":
            args.extend(["-pix_fmt","yuv420p"])
        for key in ("title","description"):
            if key in info:
                args.extend(["-metadata",key+"="+info[key]])
        bounded(args+["-movflags","+faststart","-fs",str(LIMIT),"-n",str(media)], pending, 180, stdout=subprocess.DEVNULL)
        data = probe(media)
        ov, oa = streams(data)
        if (ov["codec_name"],ov["pix_fmt"],oa["codec_name"]) != ("h264","yuv420p","aac") or media.stat().st_size >= LIMIT or abs(float(data["format"]["duration"])-float(probe(src)["format"]["duration"])) > 0.1:
            raise ValueError("output_validation")
        if any(data["format"].get("tags",{}).get(k) != info.get(k) for k in ("title","description")):
            raise ValueError("embed_mismatch")
        bounded([ff,"-nostdin","-v","error","-xerror","-protocol_whitelist","file,pipe","-i",str(media),"-map","0:v:0","-map","0:a:0","-f","null","-"],pending,120,stdout=subprocess.DEVNULL)
        with media.open("r+b") as out:
            os.fsync(out.fileno())
        exclusive_json(pending/"media.info.json", info)
        receipt = dict(schema="local-pair-stage-v1",job_id=job_id,input_sha256=pins["accepted.mp4"],input_info_sha256=pins["accepted.info.json"],media_sha256=digest(media),media_bytes=media.stat().st_size,info_sha256=digest(pending/"media.info.json"),publication_enabled=False)
        exclusive_json(pending/"receipt.json",receipt)
        os.rename(pending,ready)
        return receipt
