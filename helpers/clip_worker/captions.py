"""Pure caption/MP4 checks extracted from the reviewed standalone source."""
import math, re, struct
def ass_text(s):
    s=''.join(c if c >= ' ' else ' ' for c in s)
    return re.sub(r'\s+', ' ', s).strip().translate({ord('{'):'\uff5b',ord('}'):'\uff5d',ord('\\'):'\uff3c'})

def stamp(cs):
    h, rem = divmod(cs, 360000)
    m, rem = divmod(rem, 6000)
    s, c = divmod(rem, 100)
    return f'{h}:{m:02}:{s:02}.{c:02}'

def make_ass(words, offset=0.0, duration=26.0):
    if type(offset) not in (int,float) or not math.isfinite(offset) or offset < 0:
        raise ValueError('bad offset')
    if type(duration) not in (int,float) or not math.isfinite(duration) or duration <= 0:
        raise ValueError('bad duration')
    limit=math.floor(duration*100)
    cues=[]
    previous=0
    previous_word_start=-math.inf
    for start,end,text in words:
        if any(type(x) not in (int,float) or not math.isfinite(x) for x in (start,end)) or start<0 or start<previous_word_start or start>=end:
            raise ValueError(f'bad word timing: {start!r}, {end!r}, previous={previous_word_start!r}')
        if type(text) is not str or len(text)>150:
            raise ValueError('bad word text')
        previous_word_start=start
        a=max(previous, 0, math.floor((start-offset)*100))
        b=min(limit, math.ceil((end-offset)*100))
        if b <= a or not ass_text(text):
            continue
        cues.append((a,b,ass_text(text)))
        previous=b
    if not cues:
        raise ValueError('no grounded word captions')
    header="""[Script Info]
ScriptType: v4.00+
PlayResX: 1080
PlayResY: 1920
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Word,Arial,88,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,5,0,2,90,90,220,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
"""
    return header+''.join(f'Dialogue: 0,{stamp(a)},{stamp(b)},Word,,0,0,0,,{t}\n' for a,b,t in cues),cues

def faststart(path):
    length=path.stat().st_size
    offsets={}
    with path.open('rb') as f:
        while f.tell() < length:
            offset=f.tell()
            size,name=struct.unpack('>I4s',f.read(8))
            header=8
            if size==1:
                size=struct.unpack('>Q',f.read(8))[0]
                header=16
            elif size==0:
                size=length-offset
            if size<header or offset+size>length: raise ValueError('bad mp4 atom')
            offsets.setdefault(name,offset)
            f.seek(offset+size)
    return b'moov' in offsets and b'mdat' in offsets and offsets[b'moov']<offsets[b'mdat']
