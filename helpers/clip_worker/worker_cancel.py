"""One process-local cancellation state; never a retry or ownership reset."""
import os,threading
if os.name=='nt':
    import ctypes,msvcrt
    from ctypes import wintypes
    _kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    _peek=_kernel.PeekNamedPipe
    _peek.argtypes=[wintypes.HANDLE,ctypes.c_void_p,wintypes.DWORD,ctypes.c_void_p,ctypes.POINTER(wintypes.DWORD),ctypes.c_void_p]
    _peek.restype=wintypes.BOOL
_cancelled=threading.Event()
def request():_cancelled.set()
def checkpoint():
    if _cancelled.is_set():raise RuntimeError('worker cancelled; HOLD; no retry')
def listen(stream):
    # After GO this channel admits no further work: any byte, EOF, or error
    # cancels. A sole-reader Peek avoids a CRT fd lock held by blocking read,
    # which stalls NumPy native initialization on the qualified Windows host.
    # No other read may run concurrently on this handle. Outer Job deadline
    # remains mandatory; synchronous Windows I/O is not universally wait-free.
    try:
        if os.name=='nt':
            handle=msvcrt.get_osfhandle(stream.fileno())
            while not _cancelled.is_set():
                available=wintypes.DWORD()
                if not _peek(handle,None,0,None,ctypes.byref(available),None) or available.value:
                    request();return
                _cancelled.wait(.02)
        else:
            stream.readline();request()
    except Exception:
        request() # never silently lose cancellation supervision
