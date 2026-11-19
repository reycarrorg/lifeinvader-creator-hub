# Lifeinvader Creator Hub

A local desktop-style content hub built with Rust, SQLite, and a browser interface. Organize posting profiles and content sources into categories, browse locally downloaded media, and track a manual posting workflow.

## What works

- Separate profile and source views, categories, and category dashboards. Paste a supported account link to fill its name and platform; photos and categories can be set under More options.
- Profile analytics for manual snapshots, template CSV imports and optional Brave-based refresh of visible TikTok/Instagram profile counts, with offline charts. Private platform Insights are not connected. See [analytics research and roadmap](docs/ANALYTICS_RESEARCH.md).
- Source sync with a shared download archive, per-source Sync all exclusions, optional scheduled sync while the app is open, progress logs, local video previews, and a separate date audit.
- Paste individual TikTok, Instagram, or YouTube post links under **Sources → Post links** to download into a separate, non-syncing Solo Posts collection. It uses the source folder and a separate archive like the existing solo-post script; original account details are shown when metadata supplies them.
- When adding a source, preview up to 1,500 posts and choose all, or one side of a selected post. The center post is included. Skipped post IDs are recorded separately from the real archive and excluded from later video syncs; the earlier-side selection remains limited to the selected IDs. **Choose posts** on the source lets you revisit the selection.
- Profile sync for accessible TikTok, Instagram and YouTube videos and, when the optional photo tools are installed, photo posts. Profile downloads have their own folders and archives.
- A manual content library with per-profile claim and posting status.
- A local all-profile AI handoff and activity history for the posting queue, with target account, source context, planned time, file references and manually recorded outcomes. Copy or download it from a profile's queue. It does not publish posts; see [computer-use handoff](docs/COMPUTER_USE_HANDOFF.md).
- Shuffle downloaded source photos and videos using source/category, media type, available duration, original date and text filters; queue original file references for a profile and manually record status. Planned times do not automatically post. See [posting research and roadmap](docs/POSTING_RESEARCH.md). A reported post is a manual record; the app does not publish posts.

A fresh database starts without accounts or source folders. Add your own accounts and choose your own library folder in the app. Profiles and downloaded files are local to each installation; a GitHub fork does not share the app's database or media.

## Collaborate with a friend or AI

This is the shared upstream repository. The owner can invite a GitHub user as a collaborator. After the invitation is accepted, work in a personal fork if GitHub permits it, or in a separate branch in the shared repository. Submit a pull request into upstream `main` so the other person can review the change before it is merged.

Start with the [shared to-do and project agenda](docs/TODO.md) and [collaboration guide](docs/COLLABORATION.md). AI assistants should also read [AGENTS.md](AGENTS.md), [project progress](docs/PROGRESS.md), and [known issues](docs/ISSUES.md) before making changes. Update those records with meaningful results and link related GitHub issues or pull requests when available.

Your fork or clone contains the code, not another person's local accounts, database, or media. Do not add those files or credentials to the repository.

## Windows setup

Install Git, [Rust](https://www.rust-lang.org/tools/install), the Visual Studio C++ build tools, and Microsoft Edge. Python 3 is required for the optional photo tools and smoke tests. In PowerShell, clone your fork or the shared repository and build:

```powershell
git clone https://github.com/YOUR-USERNAME/lifeinvader-creator-hub.git
cd lifeinvader-creator-hub
cargo build --release
Copy-Item target\release\lifeinvader_hub.exe .\Lifeinvader.exe
.\Lifeinvader.exe
```

Replace `YOUR-USERNAME` with the owner of your fork, or with `reycarrorg` to clone the shared repository directly. GitHub authentication is required for a private repository.

The app opens its own Edge window at `127.0.0.1:3847`. Closing that window ends the local service after any running sync finishes. You can also start a development server with `cargo run -- --server-only` and open the local address in a browser.

## Download tools

Downloaded utilities and media are intentionally excluded from the repository. To enable sync and thumbnails:

1. Download the Windows executable for [yt-dlp](https://github.com/yt-dlp/yt-dlp/releases) from its official releases and save it as `tools/yt-dlp.exe`, or select its full path under **Sources → Source folders**.
2. Download an FFmpeg build from a source listed on the [FFmpeg download page](https://ffmpeg.org/download.html) and save its executable as `tools/ffmpeg.exe`. It creates thumbnail images from local videos. The browser may provide a fallback when FFmpeg is unavailable.
3. For TikTok photo posts, install Python 3. The included `helpers/tiktok_photos.py` is detected automatically when started from the project folder; its path can also be selected in **Source folders**.
4. For Instagram photo posts, install the helper into the project folder:

   ```powershell
   python -m pip install --target tools/gallery-dl-python gallery-dl==1.32.20
   ```

Choose an existing library folder under **Sources → Source folders** before syncing. Source videos use a shared `download-archive.txt` in that folder; profile downloads use `_My Profiles/<platform>/profile-<id>/` and their own archives. The tools skip entries already recorded in their archives. They do not guarantee that every post can be accessed: TikTok and Instagram may require authentication or restrict automated access. This app does not currently sign in to those services.

The post selector uses the order returned by the platform, which may not reflect exact posting times; dates and thumbnails can be missing. Flat previews can fail for restricted accounts, and the app offers an explicit download-all fallback for a new source. Photo sync is disabled for selective sources when it could bypass the exclusion policy. Solo-post downloads do not currently use a signed-in browser, and their separate archive does not deduplicate against other source archives.

Use the app only for content you have the right to access and use. Sync is manual unless you enable an individual source's timer. Use only one downloader at a time; separate programs are not coordinated.

## Source date audit

Source pages automatically use any saved original date. For older TikTok files with a full post ID in the filename, or a unique eight-digit suffix in the shared archive, the app can also show an **estimated post date (UTC)**. This is inferred from the ID creation time and can differ from the published date. Ambiguous suffixes and unrecognized IDs remain unknown; file/download dates are not passed off as original dates. The picker labels estimates and lets you exclude them.

Choose **Sources → Audit dates** for all sources, or **Audit dates** inside a source or source category. The default local audit checks saved metadata, filenames and the existing archive, with progress and cancellation. Optionally check **Also check the platform for verified dates** to request missing dates with yt-dlp in metadata-only mode. This can take a long time and access may be blocked. The audit never downloads media, changes the download archive, or replaces a source sidecar. It stores results in the local database and keeps partial results when stopped. Online checks and normal sync run one at a time.

The [TikTok ID timestamp research](https://hindsig.ht/blog/tinkering-with-tiktok-timestamps/) and [Bellingcat's timestamp tool](https://github.com/bellingcat/tiktok-timestamp) describe the ID-derived estimate. Platform-provided dates are preferred whenever available.

## Content and duplicates

The profile download view saves post IDs and file locations for a future file-referencing chooser. It is not yet connected to the manual content library: **Add content** currently copies selected files into that library. The app rejects duplicate imported platform/post ID pairs, but it does not detect identical videos uploaded under different post IDs.

## Data and security

The SQLite database is created at `data/lifeinvader.sqlite3`; imported media, profile photos, and preview cache are stored in `media/`. Both folders are excluded from Git. Optional environment variables `LIFEINVADER_DB` and `LIFEINVADER_MEDIA` choose alternate locations; an explicitly selected media folder must already exist.

The server listens on loopback only. It has no user authentication or shared-user security and is not ready to expose to the internet or a local network. Forking the code gives another person their own copy, not remote access to your running app. Back up the database and media separately; cloud sync is not an independent backup.

## Tests

After `cargo build`, run:

```powershell
cargo test
python tests/fresh_start_smoke.py
python tests/analytics_smoke.py
python tests/bridge_smoke.py
python tests/bridge_sync_smoke.py
node tests/bridge_collector_test.js
python tests/queue_smoke.py
python tests/handoff_smoke.py
python tests/date_audit_smoke.py
python tests/smoke.py
python tests/categories_smoke.py
python tests/source_smoke.py
python tests/profile_sync_smoke.py
python tests/scheduler_smoke.py
```

The smoke tests use isolated temporary databases and fake downloaders on local port 3859. `tests/thumbnail_smoke.py` additionally needs `tools/ffmpeg.exe`. A bounded live YouTube test-channel download and Instagram reel/fallback test succeeded, but complete channel coverage and macOS builds remain unverified.

## Optional Brave profile refresh

See [browser analytics setup and publishing research](docs/BROWSER_ANALYTICS_AND_PUBLISHING.md). Install the local `browser-extension` folder in Brave, pair it with the code shown under a profile's **Analytics → Brave refresh**, and enable the schedule. Both Brave and the hub must be open. The extension reads displayed profile counts, not private Insights or cookies. For Instagram sources, **Brave links** can capture visible post links to use as a partial fallback when profile extraction is blocked. This is not a full archive.
