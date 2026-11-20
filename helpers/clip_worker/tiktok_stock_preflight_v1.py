"""APP preflight for the pinned stock TikTok extractor. No process or network authority."""
import ast
import hashlib
import re
import zipfile
from pathlib import Path

WHEEL_SHA256 = "a05c5b5d8562f7e8a62f5334348b406af4930d56d3a65f8021d14af0e22339ae"


def preflight(wheel: Path, url: str, mode: str) -> dict:
    """Return a job-safe HOLD before APP constructs or starts any child."""
    patterns = {
        "scan": r"https://www\.tiktok\.com/@[A-Za-z0-9_.]{1,24}",
        "download": r"https://www\.tiktok\.com/@[A-Za-z0-9_.]{1,24}/video/[0-9]{15,22}",
    }
    def hold(reason):
        return {"schema": "tiktok-stock-preflight-v1", "state": "HOLD",
                "reason": reason, "argv": None}

    if mode not in patterns or not re.fullmatch(patterns[mode], url):
        return hold("invalid_target")
    try:
        with wheel.open("rb") as file:
            if hashlib.file_digest(file, "sha256").hexdigest() != WHEEL_SHA256:
                return hold("wheel_pin_mismatch")
            file.seek(0)
            with zipfile.ZipFile(file) as archive:
                tree = ast.parse(archive.read("yt_dlp/extractor/tiktok.py"))
    except (OSError, KeyError, SyntaxError, zipfile.BadZipFile):
        return hold("wheel_unavailable")
    owner, method = (("TikTokUserIE", "_real_extract") if mode == "scan"
                     else ("TikTokBaseIE", "_extract_web_data_and_status"))
    for cls in tree.body:
        if isinstance(cls, ast.ClassDef) and cls.name == owner:
            for fn in cls.body:
                if isinstance(fn, ast.FunctionDef) and fn.name == method:
                    for call in ast.walk(fn):
                        if isinstance(call, ast.Call) and any(
                            kw.arg == "impersonate" and isinstance(kw.value, ast.Constant)
                            and kw.value.value is True for kw in call.keywords
                        ):
                            return hold("stock_extractor_requests_impersonation")
    return hold("extractor_contract_unqualified")
