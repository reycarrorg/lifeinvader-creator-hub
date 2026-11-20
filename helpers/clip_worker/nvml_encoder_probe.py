"""One read-only NVML count/utilization snapshot; no process enumeration/setters."""
import ctypes,hashlib,json,os,sys,time
from pathlib import Path
DLL=Path(r'C:\Windows\System32\nvml.dll')
DLL_SHA256='0b8a01f48e22d0bafd285ef9191389491b9039db4b6163cf245cee82a1942b9c'

def collect(uuid):
    if os.name!='nt' or hashlib.sha256(DLL.read_bytes()).hexdigest()!=DLL_SHA256:raise ValueError('NVML identity')
    # Fixed absolute vendor DLL, dependencies searched only in DLL dir/System32.
    lib=ctypes.CDLL(str(DLL),winmode=0x100|0x800)
    U=ctypes.c_uint;P=ctypes.c_void_p;ref=ctypes.byref
    def bind(name,args):
        f=getattr(lib,name);f.argtypes=args;f.restype=ctypes.c_int;return f
    init=bind('nvmlInit_v2',[]);shutdown=bind('nvmlShutdown',[])
    handle=bind('nvmlDeviceGetHandleByUUID',[ctypes.c_char_p,ctypes.POINTER(P)])
    get_uuid=bind('nvmlDeviceGetUUID',[P,ctypes.c_char_p,U])
    stats=bind('nvmlDeviceGetEncoderStats',[P,ctypes.POINTER(U),ctypes.POINTER(U),ctypes.POINTER(U)])
    sessions=bind('nvmlDeviceGetEncoderSessions',[P,ctypes.POINTER(U),P])
    utilization=bind('nvmlDeviceGetEncoderUtilization',[P,ctypes.POINTER(U),ctypes.POINTER(U)])
    def ok(status):
        if status!=0:raise ValueError('NVML status '+str(status))
    ok(init())
    try:
        device=P();ok(handle(uuid.encode('ascii'),ref(device)))
        name=ctypes.create_string_buffer(96);ok(get_uuid(device,name,96))
        count=U();fps=U();latency=U();listed=U();percent=U();period=U()
        start=time.time_ns()//1000000
        ok(stats(device,ref(count),ref(fps),ref(latency)))
        # NVIDIA documents count=0 as a supported count query, no array needed.
        ok(sessions(device,ref(listed),None))
        ok(utilization(device,ref(percent),ref(period)))
        return dict(uuid=name.value.decode('ascii'),start_ms=start,end_ms=time.time_ns()//1000000,
                    session_count=count.value,enumerated_count=listed.value,
                    encoder_percent=percent.value,sampling_period_us=period.value)
    finally:ok(shutdown())

if __name__=='__main__':
    try:
        if len(sys.argv)!=2:raise ValueError('arguments')
        print(json.dumps(collect(sys.argv[1]),separators=(',',':')))
    except Exception:raise SystemExit(2) # no raw driver paths/diagnostics
