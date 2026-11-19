"""Pure final-folder resolution policy; no execution, PATH lookup or fallback."""
import hashlib
from pathlib import Path

def pinned_final_file(root,configured,manifest):
    root=Path(root).resolve(strict=True);p=Path(configured)
    if not p.is_absolute():raise ValueError('explicit final path required')
    # Reject a path outside the final folder even if its contents match.
    if '..' in p.parts or not p.is_relative_to(root):raise ValueError('outside final folder')
    for part in (p,*p.parents):
        if part==root.parent:break
        m=part.lstat()
        if part.is_symlink() or getattr(m,'st_file_attributes',0)&0x400:raise ValueError('reparse final path')
    resolved=p.resolve(strict=True)
    if not resolved.is_relative_to(root) or not resolved.is_file():raise ValueError('invalid final file')
    pin=manifest.get(resolved.relative_to(root).as_posix())
    if not pin:raise ValueError('missing final inventory pin')
    with resolved.open('rb') as f:actual=hashlib.file_digest(f,'sha256').hexdigest()
    if actual!=pin:raise ValueError('final bytes mismatch')
    return resolved
