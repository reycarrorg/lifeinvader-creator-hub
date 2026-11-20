"""Main-app dry-run adapter. Never starts Queue, ASR, FFmpeg or a producer.

Input is assembled by the local app, not accepted from an HTTP request verbatim.
Review pins are independent configuration. Passing this check is NOT render authority.
"""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
from cut_gate import parse
from intake_proof import dispatch
from rubric_gate import verify
from export_profile import RECIPE


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def prepare(c):
    expected = {'source', 'source_sha256', 'speech_id', 'caption_status', 'intake', 'intake_sha256',
                'review', 'review_sha256', 'proposal', 'start_ms', 'end_ms', 'cut_key',
                'ffmpeg', 'ffmpeg_sha256', 'ffprobe', 'ffprobe_sha256', 'model'}
    if type(c) is not dict or set(c) != expected:
        raise ValueError('configuration schema')
    if any(type(c[k]) is not int for k in ('start_ms', 'end_ms')):
        raise ValueError('cut types')
    proposal = parse(c['proposal'])
    status=c['caption_status']
    if status not in ('captioned','no_speech') or parse(Path(c['intake']).read_bytes().decode())['caption_status']!=status:
        raise ValueError('intake caption status')
    if status=='no_speech' and c['speech_id'] is not None:raise ValueError('unexpected speech identity')
    identity={'start_ms':c['start_ms'],'end_ms':c['end_ms']} if status=='no_speech' else {'speech_id':c['speech_id']}
    if proposal != [dict(identity,source_sha256=c['source_sha256'])]:
        raise ValueError('proposal identity')
    times = []
    decisions = []
    def reviewed(a, b):
        decisions.append(verify(c['review'], c['review_sha256'], c['intake_sha256'], c['proposal'], a, b))
        times.append((a, b))
    dispatch(sealed_record=c['intake'], expected_record_sha256=c['intake_sha256'],
             artifact=c['source'], proposal=c['proposal'], producer=reviewed)
    if times != [(c['start_ms'], c['end_ms'])]:
        raise ValueError('reviewed boundaries')
    full_recipe = f"{RECIPE}:cut={c['start_ms']}-{c['end_ms']}"
    key = hashlib.sha256((c['source_sha256'] + full_recipe).encode()).hexdigest()
    if key != c['cut_key']:
        raise ValueError('cut identity')
    for tool in ('ffmpeg', 'ffprobe'):
        p = Path(c[tool])
        if not p.is_absolute() or not p.is_file() or digest(p) != c[tool + '_sha256']:
            raise ValueError('tool identity')
    model = Path(c['model'])
    if not model.is_absolute() or not model.is_dir():
        raise ValueError('local model missing')
    return {'schema': 'controller-clip-preparation-v1', 'state': 'DRY_RUN_READY',
            'cut_key': key, 'start_ms': c['start_ms'], 'end_ms': c['end_ms'],
            **({'editorial_review':decisions[0]} if decisions[0] else {}), 'caption_status':status, 'recipe_base': RECIPE, 'recipe': full_recipe,
            'faster_whisper_available': importlib.util.find_spec('faster_whisper') is not None,
            'nvenc_verified': False, 'model_load_verified': False,
            'render_authorized': False, 'worker_started': False, 'publish': False}


def main():
    if sys.argv[1:] != ['--dry-run']:
        raise SystemExit('Only --dry-run is implemented; no render dispatch')
    try:
        raw = sys.stdin.buffer.read(16385)
        if not 0 < len(raw) <= 16384:
            raise ValueError('input size')
        result = prepare(parse(raw.decode('utf-8')))
    except Exception:
        result = {'schema': 'controller-clip-preparation-v1', 'state': 'HELD',
                  'reason': 'reviewed_input_or_dependency_mismatch', 'render_authorized': False,
                  'worker_started': False, 'publish': False}
    print(json.dumps(result, separators=(',', ':')))


if __name__ == '__main__':
    main()
