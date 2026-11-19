"""Read-only inventory of this process's actually loaded Windows images.
Not a loader security boundary: system trust is qualified separately.
"""
import ctypes,hashlib,os,pathlib
from ctypes import wintypes as w

def snapshot(pinned):
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    k.GetCurrentProcess.restype=w.HANDLE
    k.K32EnumProcessModules.argtypes=[w.HANDLE,ctypes.POINTER(w.HMODULE),w.DWORD,ctypes.POINTER(w.DWORD)]
    k.K32EnumProcessModules.restype=w.BOOL
    k.K32GetModuleFileNameExW.argtypes=[w.HANDLE,w.HMODULE,w.LPWSTR,w.DWORD]
    k.K32GetModuleFileNameExW.restype=w.DWORD
    k.GetSystemDirectoryW.argtypes=[w.LPWSTR,w.UINT];k.GetSystemDirectoryW.restype=w.UINT
    system=ctypes.create_unicode_buffer(32768)
    if not 0<k.GetSystemDirectoryW(system,len(system))<len(system):raise ValueError('system directory')
    system=os.path.normcase(str(pathlib.Path(system.value).resolve()))
    modules=(w.HMODULE*2048)();needed=w.DWORD();process=k.GetCurrentProcess()
    if not k.K32EnumProcessModules(process,modules,ctypes.sizeof(modules),ctypes.byref(needed)) or needed.value>ctypes.sizeof(modules):raise ValueError('module enumeration')
    records=[];unlisted=[]
    for handle in modules[:needed.value//ctypes.sizeof(w.HMODULE)]:
        buf=ctypes.create_unicode_buffer(32768)
        if not 0<k.K32GetModuleFileNameExW(process,handle,buf,len(buf))<len(buf):raise ValueError('module path')
        path=pathlib.Path(buf.value);key=os.path.normcase(str(path.resolve()))
        with path.open('rb') as f:pin=hashlib.file_digest(f,'sha256').hexdigest();stat=os.fstat(f.fileno())
        if key in pinned:
            if pin!=pinned[key]:raise ValueError('loaded module pin mismatch')
            kind='manifest-leased'
        elif os.path.dirname(key)==system:kind='windows-system-pending-signature'
        else:kind='UNLISTED';unlisted.append(str(path))
        records.append(dict(path=str(path),sha256=pin,bytes=stat.st_size,file_index=stat.st_ino,kind=kind))
    return dict(schema='loaded-native-images-v1',pid=os.getpid(),modules=sorted(records,key=lambda r:r['path'].lower()),unlisted=unlisted)

# Process-local search hardening; never changes Windows/global environment policy.
_cookies=[]

def search_plan(pinned):
    system=os.path.normcase(os.path.abspath(os.environ['SystemRoot']))
    directories=set();names={}
    for key,pin in pinned.items():
        p=pathlib.Path(key)
        # Python .pyd imports use guarded exact paths; never add their names
        # to global DLL search (PyAV legitimately repeats names per package).
        if p.suffix.lower()!='.dll':continue
        if key.startswith(system+os.sep):continue
        directories.add(str(p.parent))
        name=p.name.lower()
        if name in names and names[name]!=pin:raise ValueError('ambiguous native basename')
        names[name]=pin
    for directory in directories:
        p=pathlib.Path(directory)
        if any(x.is_symlink() or (x.stat().st_file_attributes & 0x400) for x in (p,*p.parents)):
            raise ValueError('native directory reparse')
        for entry in p.iterdir():
            if entry.suffix.lower() not in ('.dll','.pyd'):continue
            key=os.path.normcase(str(entry.resolve()))
            if entry.is_symlink() or key not in pinned:raise ValueError('unlisted native sibling')
    return sorted(directories)

def enforce_snapshot(pinned):
    evidence=snapshot(pinned)
    if evidence['unlisted'] or any(m['kind']!='manifest-leased' for m in evidence['modules']):
        raise ValueError('native runtime allowlist HOLD')
    return evidence

def install(pinned):
    import sys
    directories=search_plan(pinned)
    allowed=set(directories)
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    k.SetDefaultDllDirectories.argtypes=[w.DWORD];k.SetDefaultDllDirectories.restype=w.BOOL
    if not k.SetDefaultDllDirectories(0x800|0x400):raise ValueError('safe DLL search unavailable')
    # System32 + exact inventoried native directories; no CWD/PATH/application fallback.
    def audit(event,args):
        if event=='os.add_dll_directory':
            if os.path.normcase(str(pathlib.Path(args[0]).resolve())) not in allowed:raise ValueError('DLL directory unapproved')
        elif event=='ctypes.dlopen':
            name=args[0]
            if not isinstance(name,str):raise ValueError('DLL target unapproved')
            p=pathlib.Path(name)
            if p.is_absolute():
                if os.path.normcase(str(p.resolve())) not in pinned:raise ValueError('DLL path unapproved')
            else:
                # Only the explicit Windows API library used by this reviewed code.
                if name.lower() not in ('kernel32','kernel32.dll'):raise ValueError('bare DLL name unapproved')
    sys.addaudithook(audit)
    for directory in directories:_cookies.append(os.add_dll_directory(directory))
    enforce_snapshot(pinned)
