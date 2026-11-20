VIDEO=['-vf','scale=1080:608,pad=1080:1920:0:656,setsar=1','-c:v','h264_nvenc','-pix_fmt','yuv420p']
EXPORT=[*VIDEO,'-c:a','aac','-ar','48000','-ac','2','-b:a','128k','-movflags','+faststart','-f','mp4']
RECIPE='selected-source-1080x1920-word-ass-v1-nvenc-aac48-128k-faststart'
def validate(p):
    videos=[x for x in p['streams'] if x.get('codec_type')=='video']
    audios=[x for x in p['streams'] if x.get('codec_type')=='audio']
    if len(videos)!=1 or len(audios)!=1: raise ValueError('expected one video and one audio')
    v,a=videos[0],audios[0]
    if (v.get('width'),v.get('height'),v.get('codec_name'),v.get('pix_fmt'))!=(1080,1920,'h264','yuv420p'): raise ValueError('video format')
    if (a.get('codec_name'),int(a.get('sample_rate',0)),a.get('channels'))!=('aac',48000,2): raise ValueError('audio format')
    if not 0<int(a.get('bit_rate',0))<=128000: raise ValueError('audio bitrate')
    if 'mp4' not in p['format'].get('format_name','').split(','): raise ValueError('container')
