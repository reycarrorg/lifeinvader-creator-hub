"""Exact reviewed Windows image admission; directory location alone grants nothing."""
import hashlib,os
from pathlib import Path

def exact_system_image(path,windows_root,reviewed):
    p=Path(path).resolve(strict=True);root=Path(windows_root).resolve(strict=True)
    if not p.is_relative_to(root) or not p.is_file():raise ValueError('not reviewed Windows image')
    expected=reviewed.get(os.path.normcase(str(p)))
    if not expected:raise ValueError('unlisted Windows image')
    with p.open('rb') as f:actual=hashlib.file_digest(f,'sha256').hexdigest()
    if actual!=expected:raise ValueError('changed Windows image')
    return actual
