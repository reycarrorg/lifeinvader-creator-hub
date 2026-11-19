"""Pure relocation planning; no copying, installs, configuration activation or execution.

Input hashes describe existing pinned bytes, not approval to execute relocated bytes.
The generated layout needs new absolute-path manifests and loader qualification.
"""
from pathlib import PureWindowsPath as W
import re

def plan(files, roots, excluded_roots):
    """roots: explicit source-root -> relative payload-root. Unknown files fail closed."""
    mapped, excluded, seen = [], [], set()
    for raw, digest in sorted(files.items()):
        source = W(raw)
        if not source.is_absolute() or '..' in source.parts or not re.fullmatch('[0-9a-f]{64}', digest):
            raise ValueError('invalid pinned input')
        matches = [(W(old), W(new)) for old, new in roots.items() if source.is_relative_to(W(old))]
        if len(matches) > 1:
            raise ValueError('ambiguous source mapping')
        if not matches:
            reasons = [label for old, label in excluded_roots.items() if source.is_relative_to(W(old))]
            if len(reasons) != 1:
                raise ValueError('unclassified input')
            excluded.append(dict(source=raw, sha256=digest, reason=reasons[0]))
            continue
        old, new = matches[0]
        relative = new / source.relative_to(old)
        if relative.is_absolute() or relative.drive or '..' in relative.parts:
            raise ValueError('unsafe payload destination')
        key = str(relative).casefold()
        if key in seen:
            raise ValueError('payload collision')
        seen.add(key)
        mapped.append(dict(source=raw, destination=relative.as_posix(), sha256=digest))
    return dict(schema='runtime-relocation-plan-v1', launch_enabled=False,
                qualified=False, license_review_complete=False,
                files=mapped, excluded=excluded)
