"""Embedded final-folder import and loaded-image proof; no model/process/network."""
import sys,os,json,hashlib,ctypes,importlib,importlib.machinery,contextlib
import pathlib
from pathlib import Path
from ctypes import wintypes as W
root=Path(sys.argv[1]).resolve();out=Path(sys.argv[2]).resolve();host_manifest=Path(sys.argv[3]).resolve()
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
trace=(out/'phases.txt').open('x',encoding='ascii')
def mark(s):trace.write(s+'\n');trace.flush()
def pinned_module_origin(spec, files):
    """Check a file or an exact retained ZIP container, never a path prefix alone."""
    import zipimport
    if isinstance(spec.loader, zipimport.zipimporter):
        archive = pathlib.Path(spec.loader.archive).resolve(strict=True)
        key = os.path.normcase(str(archive))
        if key not in files or digest(archive) != files[key]:
            raise ImportError('unlisted or changed module archive')
        origin = pathlib.Path(spec.origin)
        try: member = origin.relative_to(archive)
        except ValueError: raise ImportError('archive origin mismatch') from None
        if not member.parts or any(p in ('.', '..') for p in member.parts):
            raise ImportError('archive member escape')
        if spec.loader.get_filename(spec.name) != spec.origin:
            raise ImportError('archive loader mismatch')
        return
    origin = os.path.normcase(str(pathlib.Path(spec.origin).resolve()))
    if origin not in files or digest(origin) != files[origin]:
        raise ImportError('unlisted or changed module')
    if spec.cached and pathlib.Path(spec.cached).exists():
        cached = os.path.normcase(str(pathlib.Path(spec.cached).resolve()))
        if cached not in files or digest(cached) != files[cached]:
            raise ImportError('unlisted or changed bytecode')

digest=lambda p:sha(Path(p))

def run():
 assert sha(root/'manifest.package-check.sha256')=='92a66a722357e123df2edcac1b7eca5da68b84079a9fa72c269d81bc3cc1cc65'
 assert sha(host_manifest)=='35186fd88f09e292c2530273bee468f7c0be8bfc828eafa95b742e090127b711'
 pins={line.split('  ',1)[1]:line.split('  ',1)[0] for line in (root/'manifest.package-check.sha256').read_text().splitlines()}
 host={os.path.normcase(str(Path(k).resolve())):v for k,v in json.loads(host_manifest.read_text())['files'].items() if Path(k).is_relative_to(Path(os.environ['SystemRoot']))}
 requirement=json.loads((root/'host-requirements.json').read_text(encoding='utf-8-sig'))
 assert requirement['path']==r'C:\Windows\System32\uxtheme.dll' and requirement['sha256']=='e2e972e21a16c62e7c678334dc50f33c2f327c1b6854a2fb096ca244ae650f27' and requirement['signature_status']=='Valid' and requirement['publisher']=='Microsoft Windows' and requirement['copy_allowed'] is False
 host[os.path.normcase(requirement['path'])]=requirement['sha256']
 python=root/'runtime/python';stdlib=python/'python312.zip';site=root/'runtime/site-packages'
 assert Path(sys.executable).resolve()==python/'python.exe' and Path(sys.prefix).resolve()==python
 assert sys.flags.isolated==1 and sys.flags.no_site==1
 assert all(Path(p).resolve().is_relative_to(root) for p in sys.path)
 assert set(map(lambda p:str(Path(p).resolve()),sys.path))=={str(stdlib),str(python),str(site)}
 k=ctypes.WinDLL('kernel32',use_last_error=True);k.CreateFileW.argtypes=[W.LPCWSTR,W.DWORD,W.DWORD,ctypes.c_void_p,W.DWORD,W.DWORD,W.HANDLE];k.CreateFileW.restype=W.HANDLE;k.CloseHandle.argtypes=[W.HANDLE]
 handles=[]
 try:
  for relative,pin in pins.items():
   p=root/relative;assert p.resolve().is_relative_to(root)
   handle=k.CreateFileW(str(p),0x80000000,1,None,3,0x00200000,None)
   if handle==ctypes.c_void_p(-1).value:raise OSError('lease failed')
   handles.append(handle);assert sha(p)==pin
  def module_pin(path):
   p=Path(path)
   if str(p).startswith(str(stdlib)+os.sep):return # entire exact zip is retained/hashed above
   p=p.resolve();assert p.is_relative_to(root)
   assert pins.get(p.relative_to(root).as_posix())==sha(p)
  absolute_pins={os.path.normcase(str((root/n).resolve())):v for n,v in pins.items()}
  negative_count=0
  for name in ('email','numpy'):
   spec=importlib.machinery.PathFinder.find_spec(name)
   assert spec is not None
   try:pinned_module_origin(spec,{})
   except ImportError:negative_count+=1
   else:raise AssertionError('missing pin admitted')
  assert negative_count==2
  class Guard:
   def find_spec(self,fullname,path=None,target=None):
    if any(x in ('oauth','_oauth','_oauth_device') for x in fullname.split('.')):raise ImportError('OAuth excluded')
    spec=importlib.machinery.PathFinder.find_spec(fullname,path,target)
    if spec:
     if spec.origin:pinned_module_origin(spec,absolute_pins)
     elif any(not Path(d).resolve().is_relative_to(root) for d in spec.submodule_search_locations or []):raise ImportError('namespace fallback')
    return spec
  sys.meta_path=[Guard() if x is importlib.machinery.PathFinder else x for x in sys.meta_path]
  k.SetDefaultDllDirectories.argtypes=[W.DWORD];assert k.SetDefaultDllDirectories(0x800|0x400)
  cookies=[os.add_dll_directory(d) for d in sorted({str((root/n).parent) for n in pins if n.lower().endswith('.dll')})]
  def audit(event,args):
   if event in ('subprocess.Popen','os.system','socket.__new__','socket.connect','socket.bind'):raise RuntimeError('external effect denied')
  sys.addaudithook(audit)
  for name in ('numpy','ctranslate2','av','onnxruntime','tokenizers','faster_whisper'):
   mark('IMPORT_'+name);importlib.import_module(name);mark('IMPORTED_'+name)
  mark('PYTHON_ORIGINS');count=0
  frozen=0
  for module in tuple(sys.modules.values()):
   if getattr(getattr(module,'__spec__',None),'origin',None) in ('built-in','frozen'):
    frozen+=1;continue # supplied by exact pinned interpreter, __file__ may be informational
   path=getattr(module,'__file__',None)
   if path and Path(path).resolve()!=Path(__file__).resolve():module_pin(path);count+=1
  assert not any(any(p in ('oauth','_oauth','_oauth_device') for p in n.split('.')) for n in sys.modules)
  mark('NATIVE_ORIGINS');k.GetCurrentProcess.restype=W.HANDLE;k.K32EnumProcessModules.argtypes=[W.HANDLE,ctypes.POINTER(W.HMODULE),W.DWORD,ctypes.POINTER(W.DWORD)];k.K32GetModuleFileNameExW.argtypes=[W.HANDLE,W.HMODULE,W.LPWSTR,W.DWORD]
  process=k.GetCurrentProcess();modules=(W.HMODULE*2048)();needed=W.DWORD();assert k.K32EnumProcessModules(process,modules,ctypes.sizeof(modules),ctypes.byref(needed)) and needed.value<=ctypes.sizeof(modules)
  native=0;system=0;unlisted=[]
  for handle in modules[:needed.value//ctypes.sizeof(W.HMODULE)]:
   buf=ctypes.create_unicode_buffer(32768);assert k.K32GetModuleFileNameExW(process,handle,buf,len(buf));p=Path(buf.value).resolve()
   if p.is_relative_to(root):module_pin(p);native+=1
   elif host.get(os.path.normcase(str(p)))==sha(p):system+=1
   else:
    unlisted.append({'category':'WINDOWS' if p.is_relative_to(Path(os.environ['SystemRoot'])) else 'OUTSIDE_WINDOWS_AND_PAYLOAD','sha256':sha(p),'name':p.name,'windows_path':str(p) if p.is_relative_to(Path(os.environ['SystemRoot'])) else None})
  if unlisted:
   with (out/'native-hold-all.json').open('x') as f:json.dump(unlisted,f,indent=2)
   raise ValueError('unlisted native image')
  result=dict(embedded_imports='PASS',relative_pth='PASS',original_path_fallback=False,oauth_imported=False,python_modules=count,builtin_frozen_modules=frozen,payload_native_modules=native,exact_host_native_modules=system,retained_payload_leases=len(handles),model_loaded=False,gpu_queried=False,subprocess_started=False,production_bootstrap_zip_origin_qualified=True,negative_no_fallback_cases=negative_count)
  with (out/'result.json').open('x') as f:json.dump(result,f,indent=2)
  print(json.dumps(result));mark('COMPLETE')
 finally:
  for handle in handles:k.CloseHandle(handle)
try:run()
except Exception as e:
 print(json.dumps({'result':'HOLD','error_class':type(e).__name__,'code':str(e) if str(e) in ('OAuth excluded','namespace fallback','external effect denied','unlisted native image','lease failed') else 'FIXED_CHECK_FAILED'}));raise SystemExit(2)
