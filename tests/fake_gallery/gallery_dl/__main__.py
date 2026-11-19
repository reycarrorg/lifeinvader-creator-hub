"""Disposable stand-in for gallery-dl used by the profile-sync test."""
import json, os, pathlib, sqlite3, sys
args=sys.argv[1:]
if os.environ.get('LV_GALLERY_PROFILE_FAIL') and args[-1].rstrip('/').endswith('/demo-account'):
    print('synthetic profile photo scan failure',file=sys.stderr);sys.exit(1)
assert '--config-ignore' in args and '--write-metadata' in args and '--filter' in args
assert 'jpg' in args[args.index('--filter')+1]
target=pathlib.Path(args[args.index('--directory')+1])
archive=pathlib.Path(args[args.index('--download-archive')+1])
assert archive.parent==target
db=sqlite3.connect(archive);db.execute('CREATE TABLE IF NOT EXISTS archive (id TEXT PRIMARY KEY)')
if not db.execute('SELECT 1 FROM archive WHERE id=?',('image-1',)).fetchone():
    image=target/'image-1.jpg';image.write_bytes(b'fake-image')
    (target/'image-1.jpg.json').write_text(json.dumps({'post_shortcode':'post-code','media_id':'image-1','date':'2024-05-06T12:00:00'}),encoding='utf8')
    db.execute('INSERT INTO archive VALUES (?)',('image-1',));db.commit()
db.close()
