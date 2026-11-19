"""Download TikTok slideshow images from publicly accessible post pages."""
import argparse
import json
import re
import sqlite3
import ssl
import subprocess
import sys
import time
from pathlib import Path
from urllib.parse import urlparse
from urllib.request import Request, urlopen

UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36'
POST = re.compile(r'https?://(?:www\.)?tiktok\.com/@([\w.-]+)/(?:photo|video)/(\d+)', re.I)
SCRIPT = re.compile(r'<script[^>]+id=["\']__UNIVERSAL_DATA_FOR_REHYDRATION__["\'][^>]*>(.*?)</script>', re.S)

def get_url(url, referer=None):
    context = ssl.create_default_context()
    if sys.platform == "darwin" and not context.get_ca_certs():
        context.load_verify_locations(cafile="/etc/ssl/cert.pem")
    headers = {'User-Agent': UA, 'Accept-Language': 'en-US,en;q=0.9'}
    if referer:
        headers['Referer'] = referer
    with urlopen(Request(url, headers=headers), timeout=30, context=context) as response:
        return response.read(), response.headers.get_content_type()

def get_item(username, post_id):
    url = f'https://www.tiktok.com/@{username}/video/{post_id}'
    content, _ = get_url(url)
    match = SCRIPT.search(content.decode('utf-8', errors='replace'))
    if not match:
        raise ValueError('TikTok did not return post data')
    data = json.loads(match.group(1))
    detail = data.get('__DEFAULT_SCOPE__', {}).get('webapp.video-detail', {})
    item = detail.get('itemInfo', {}).get('itemStruct')
    if not item or str(item.get('id')) != post_id:
        raise ValueError('Post unavailable or login required')
    return item, url

def save_photo_post(username, post_id, destination, cache):
    if cache.execute('SELECT 1 FROM videos WHERE id=?', (post_id,)).fetchone():
        return 'known video', 0
    item, referer = get_item(username, post_id)
    images = item.get('imagePost', {}).get('images', [])
    if not images:
        if item.get('video') and 'imagePost' not in item:
            cache.execute('INSERT OR IGNORE INTO videos (id) VALUES (?)', (post_id,))
            cache.commit()
            return 'video', 0
        raise ValueError('Post has no accessible images')
    owner = item.get('author', {}).get('uniqueId') or username
    owner = re.sub(r'[^\w.-]', '_', owner)[:80].strip('.') or 'unknown'
    folder = destination / owner
    previous_folders = [folder / post_id, destination / 'Photos' / owner / post_id]
    description = str(item.get('desc') or 'TikTok photo')
    title = re.sub(r'[<>:"/\\|?*\x00-\x1f]', '', description)
    title = re.sub(r'\s+', ' ', title).strip(' .')[:80] or 'TikTok photo'
    base = f'{title} [{post_id[-8:]}]'
    count = 0
    for index, image in enumerate(images, 1):
        urls = image.get('imageURL', {}).get('urlList', [])
        if not urls:
            raise ValueError(f'Image {index} has no download URL')
        url = urls[0]
        parsed = urlparse(url)
        host = (parsed.hostname or '').lower()
        if parsed.scheme != 'https' or not any(host == d or host.endswith('.' + d) for d in ('tiktokcdn.com', 'tiktokcdn-us.com', 'tiktokcdn-eu.com', 'ibytedtos.com', 'byteoversea.com')):
            raise ValueError(f'Unrecognized image host: {host}')
        image_suffixes = ('.jpg', '.jpeg', '.png', '.webp', '.avif')
        expected_stem = f'{base} - {index:02d}'
        existing = [p for p in folder.glob('*') if (p.stem == expected_stem or p.stem == f'{expected_stem} [{post_id}]') and p.suffix.lower() in image_suffixes and p.stat().st_size]
        if existing:
            continue
        previous = [p for old in previous_folders for p in old.glob('*') if (p.stem == f'{index:03d}' or p.stem == expected_stem) and p.suffix.lower() in image_suffixes and p.stat().st_size]
        if previous:
            print(f'Photo {post_id} slide {index} already exists in a previous photo folder.', flush=True)
            continue
        content, content_type = get_url(url, referer)
        extension = {'image/jpeg': '.jpg', 'image/png': '.png', 'image/webp': '.webp', 'image/avif': '.avif'}.get(content_type)
        if not extension or not content:
            raise ValueError(f'Image {index} returned {content_type}, not an image')
        folder.mkdir(parents=True, exist_ok=True)
        temp = folder / f'{post_id}-{index:03d}.part'
        temp.write_bytes(content)
        temp.replace(folder / f'{base} - {index:02d}{extension}')
        count += 1
    return 'photo', count

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('url')
    parser.add_argument('--destination', required=True)
    parser.add_argument('--yt', required=True)
    parser.add_argument('--browser', default='')
    parser.add_argument('--limit', type=int, default=0, help=argparse.SUPPRESS)
    args = parser.parse_args()
    destination = Path(args.destination)
    if (destination.name.casefold() == 'photos'
            and (destination / '.photo-scan-cache.sqlite3').is_file()
            and (destination.parent / 'download-archive.txt').is_file()):
        destination = destination.parent
        print('Using account folders instead of the previous Photos folder.', flush=True)
    destination.mkdir(parents=True, exist_ok=True)
    old_cache = destination / 'Photos' / '.photo-scan-cache.sqlite3'
    cache = sqlite3.connect(old_cache if old_cache.is_file() else destination / '.photo-scan-cache.sqlite3')
    cache.execute('CREATE TABLE IF NOT EXISTS videos (id TEXT PRIMARY KEY)')
    post = POST.match(args.url)
    proc = None
    if post:
        entries = [{'id': post.group(2), 'uploader': post.group(1)}]
    else:
        command = [args.yt, '--ignore-config', '--flat-playlist', '--no-warnings', '-j']
        if args.browser:
            command += ['--cookies-from-browser', args.browser]
            print('Browser login is used for the profile listing; photo pages are requested without login.', flush=True)
        if args.limit:
            command += ['--playlist-items', f'1:{args.limit}']
        command += ['--', args.url]
        proc = subprocess.Popen(command, stdout=subprocess.PIPE, text=True, encoding='utf-8', errors='replace')
        entries = (json.loads(line) for line in proc.stdout if line.strip())
    photos = images = failed = checked = 0
    try:
        for entry in entries:
            post_id = str(entry.get('id') or '')
            username = entry.get('uploader') or ''
            if not post_id.isdigit() or not re.fullmatch(r'[\w.-]+', username):
                print(f'Skipped a post with missing ID or username: {post_id}', flush=True)
                failed += 1
                continue
            checked += 1
            try:
                kind, count = save_photo_post(username, post_id, destination, cache)
                if kind == 'photo':
                    photos += 1
                    images += count
                    print(f'Photo post {post_id}: {count} new images', flush=True)
                elif kind != 'known video':
                    print(f'Checked {post_id}: video', flush=True)
                if kind != 'known video':
                    time.sleep(0.5)
            except Exception as exc:
                failed += 1
                print(f'Could not check {post_id}: {exc}', flush=True)
        if proc and proc.wait() != 0:
            failed += 1
            print('Profile listing failed or was incomplete.', flush=True)
    finally:
        cache.close()
    print(f'Photo scan: {checked} posts checked, {photos} photo posts, {images} new images, {failed} errors.', flush=True)
    return 1 if failed or not checked else 0

if __name__ == '__main__':
    sys.exit(main())
