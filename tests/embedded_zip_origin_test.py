"""Pure qualification of the actual bootstrap origin functions; no bootstrap entry."""
import ast,hashlib,os,pathlib,tempfile,zipfile,zipimport,importlib.machinery,types
repo=pathlib.Path(__file__).resolve().parents[1]
def load(name):
    tree=ast.parse((repo/'helpers/clip_worker'/name).read_text())
    node=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='pinned_module_origin')
    digest=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
    scope=dict(pathlib=pathlib,os=os,digest=digest,sha=digest)
    exec(compile(ast.Module(body=[node],type_ignores=[]),name,'exec'),scope)
    return scope['pinned_module_origin']
def reject(fn,spec,pins):
    try:fn(spec,pins)
    except ImportError:return
    raise AssertionError('unexpected admission')
with tempfile.TemporaryDirectory(prefix='zip-origin-') as td:
    root=pathlib.Path(td);z=root/'stdlib.zip'
    with zipfile.ZipFile(z,'w') as f:f.writestr('fixture_only.py','value=7\n')
    loader=zipimport.zipimporter(str(z));spec=loader.find_spec('fixture_only')
    key=os.path.normcase(str(z.resolve()));pin=hashlib.sha256(z.read_bytes()).hexdigest()
    fallback=root/'fallback.py';fallback.write_text('value=8\n')
    outside=importlib.machinery.ModuleSpec('fallback',importlib.machinery.SourceFileLoader('fallback',str(fallback)),origin=str(fallback))
    for name in ('native_bootstrap.py','asr_bootstrap.py'):
        fn=load(name);fn(spec,{key:pin})
        reject(fn,spec,{})
        reject(fn,spec,{key:'0'*64})
        reject(fn,outside,{key:pin})
        wrong=types.SimpleNamespace(loader=loader,name=spec.name,origin=str(z/'../outside.py'))
        reject(fn,wrong,{key:pin})
        fake=types.SimpleNamespace(loader=object(),name=spec.name,origin=spec.origin,cached=None)
        reject(fn,fake,{key:pin})
print('PASS: 12 actual bootstrap ZIP/file origin cases; no entry point or subprocess')
