"""Generate the local Windows shortcut icon using only the Python standard library."""
import struct
from pathlib import Path

size = 64
pixels = bytearray()
for y in reversed(range(size)):
    for x in range(size):
        # Transparent corners give the pink tile a rounded shape.
        dx = max(10-x, 0, x-53)
        dy = max(10-y, 0, y-53)
        alpha = 0 if dx*dx + dy*dy > 100 else 255
        is_l = (19 <= x <= 27 and 14 <= y <= 49) or (19 <= x <= 45 and 41 <= y <= 49)
        r,g,b = (255,255,255) if is_l else (246,55,112)
        pixels.extend((b,g,r,alpha))
mask = bytes(((size + 31)//32)*4*size)
bitmap = struct.pack('<IiiHHIIiiII',40,size,size*2,1,32,0,len(pixels),0,0,0,0)+pixels+mask
header = struct.pack('<HHH',0,1,1)
entry = struct.pack('<BBBBHHII',size,size,0,0,1,32,len(bitmap),22)
Path(__file__).with_name('lifeinvader.ico').write_bytes(header+entry+bitmap)
