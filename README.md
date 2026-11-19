# Lifeinvader Creator Hub

A local desktop-style content hub built with Rust, SQLite, and a browser interface. Organize posting profiles and content sources into categories, browse locally downloaded media, and track a manual posting workflow.

## What works

- Profile and source views each include their own categories and category dashboards. Use Manage to rename a category and choose its members. Right-click a tile for account or category options; account removal is reversible from Removed accounts and keeps files and history. Paste a supported account link to fill its name and platform; photos and categories can be set under More options.
- Profile analytics for manual snapshots, template CSV imports and optional Brave-based refresh of visible TikTok/Instagram profile counts, with offline charts. Private platform Insights are not connected. See [analytics research and roadmap](docs/ANALYTICS_RESEARCH.md).
- New-download highlights expire one hour after the Hub records a download, with a visible countdown; marking media seen clears them earlier. Source and profile views offer **Open folder** as well as folder settings.
- Source sync with a shared download archive, optional scheduled sync while the app is open, progress logs, local previews, and a separate date audit. Sync all includes only sources with auto sync enabled that have not been marked dead. Right-click a source to mark it dead or active. Tile counts show tracked downloads from the last 24 hours, even after marking them seen.
- Paste individual TikTok, Instagram, or YouTube post links under **Sources → Post links** to download into a separate, non-syncing Solo Posts collection. It uses the source folder and a separate archive like the existing solo-post script; original account details are shown when metadata supplies them.
- Choose **Solo Posts · individual links** in the source or category filter of the source picker or content library. Matching video thumbnails are omitted from those choices, while standalone photos remain available. Original account, platform and post details depend on saved metadata; queue entries refer to the local files.
- When adding a source, preview up to 1,500 posts and choose all, or one side of a selected post. The center post is included. Skipped post IDs are recorded separately from the real archive and excluded from later video syncs; the earlier-side selection remains limited to the selected IDs. **Choose posts** on the source lets you revisit the selection.
- Profile sync for accessible TikTok, Instagram and YouTube videos and, when the optional photo tools are installed, photo posts. Profile downloads have their own folders and archives.
- Category dashboards let you choose an icon and color or upload a PNG/JPEG for the category. The picture stays in local media storage.
- Edit source media or your own downloaded media with centered crop presets, start/end trims, optional mute, and an optional uploaded PNG/JPEG logo at 50% opacity. FFmpeg creates a separate file under the source folder. Source edits can be queued for review; the queue keeps the original post identity and description and references the edited copy. Nothing is published automatically. The editor needs FFmpeg (`tools/ffmpeg.exe` on Windows, `LIFEINVADER_FFMPEG`, or a system PATH installation).
- Quick editing is optional. In **Settings**, turn off **Show quick editor** to hide media edit buttons; original media can still be previewed and queued. The preference is stored in the app browser and can be changed again at any time.
- Profile dashboards browse downloaded media across source accounts, with category, source, type, search and length filters and direct queueing. Older local videos can be scanned for length from the library; the scan reads the existing files and does not download them again.
- A manual content library with per-profile claim and posting status.
- A local all-profile AI handoff and activity history for the posting queue, with target account, source context, planned time, file references and manually recorded outcomes. Copy or download it from a profile's queue. It does not publish posts; see [computer-use handoff](docs/COMPUTER_USE_HANDOFF.md).
- Pick downloaded source photos and videos using source/category, media type, available duration, original date, text and ordering controls; preview the result, see its original details, and queue file references for a profile to review and manually record status. Planned times do not automatically post. See [posting research and roadmap](docs/POSTING_RESEARCH.md). A reported post is a manual record; the app does not publish posts.
- For a profile, choose **Pick / auto queue**, set filters and expand **Auto queue using these filters**. Enable it, choose an interval and a maximum number of queued posts, and save. While the Hub is open it adds one random matching local post per interval until the limit is reached; missed intervals do not create a backlog. A blue inner tile border shows the countdown. Length filters use saved metadata or scanned durations; videos with unknown lengths are skipped. The rule is off until enabled and does not publish.
- Add profile-specific instructions for the posting assistant in the same panel. **Copy task prompt** prepares the instructions for a scheduler. The local handoff supplies a per-item claim and result contract to prevent a second compliant worker claiming an in-progress item. Interrupted claims remain held for human review. The Hub does not run the assistant or verify provider-side publication. See [computer-use handoff](docs/COMPUTER_USE_HANDOFF.md) and the [scheduled prompt](docs/SCHEDULED_POSTING_PROMPT.md).

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

Replace `YOUR-USERNAME` with the owner of your fork, or with `reycarrorg` to clone the shared repository directly. The shared repository is private; GitHub authentication and collaborator access are required.

The app opens its own Edge window at `127.0.0.1:3847`. Closing that window ends the local service after any running sync finishes. You can also start a development server with `cargo run -- --server-only` and open the local address in a browser.

## Download tools

Downloaded utilities and media are intentionally excluded from the repository. To enable sync and thumbnails:

1. Download the Windows executable for [yt-dlp](https://github.com/yt-dlp/yt-dlp/releases) from its official releases and save it as `tools/yt-dlp.exe`, or select its full path under **Sources → Source folders**.
2. Download an FFmpeg build from a source listed on the [FFmpeg download page](https://ffmpeg.org/download.html) and save both `ffmpeg.exe` and `ffprobe.exe` in `tools/`. Lifeinvader passes the discovered FFmpeg path to yt-dlp for source and individual-post downloads; it also uses FFmpeg for local thumbnails and edits. `LIFEINVADER_FFMPEG` can point to a different executable or containing folder.
3. For TikTok photo posts, install Python 3. The included `helpers/tiktok_photos.py` is detected automatically when started from the project folder; its path can also be selected in **Source folders**.
4. For Instagram photo posts, install the helper into the project folder:

   ```powershell
   python -m pip install --target tools/gallery-dl-python gallery-dl==1.32.20
   ```

Choose an existing library folder under **Sources → Source folders** before syncing. Source videos use a shared `download-archive.txt` in that folder; profile downloads use `_My Profiles/<platform>/profile-<id>/` and their own archives. The tools skip entries already recorded in their archives. They do not guarantee that every post can be accessed: TikTok and Instagram may require authentication or restrict automated access. This app does not currently sign in to those services.

The post selector uses the order returned by the platform, which may not reflect exact posting times; dates and thumbnails can be missing. Flat previews can fail for restricted accounts, and the app offers an explicit download-all fallback for a new source. Photo sync is disabled for selective sources when it could bypass the exclusion policy. Solo-post downloads do not currently use a signed-in browser, and their separate archive does not deduplicate against other source archives.

For a TikTok profile that fails to resolve, **Fix TikTok sync** lets the user save a link to one of that account's own posts. After the normal listing fails, the app checks the post's owner, obtains its account ID and retries with the existing profile archive. Instagram can retry previously captured Brave profile links when its profile extractor fails; this is explicitly reported as partial coverage and photos can also use captured post links. These fallbacks depend on platform access, and captured links do not automatically include later posts unless refreshed.

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
python tests/auto_queue_handoff_smoke.py
python tests/solo_picker_smoke.py
python tests/handoff_smoke.py
python tests/date_audit_smoke.py
python tests/smoke.py
python tests/categories_smoke.py
python tests/source_smoke.py
python tests/profile_sync_smoke.py
python tests/recovery_smoke.py
python tests/scheduler_smoke.py
```

The smoke tests use isolated temporary databases and fake downloaders on local port 3859. `tests/thumbnail_smoke.py` additionally needs `tools/ffmpeg.exe`. A bounded live YouTube test-channel download and Instagram reel/fallback test succeeded, but complete channel coverage and macOS builds remain unverified.

## Optional Brave profile refresh

See [browser analytics setup and publishing research](docs/BROWSER_ANALYTICS_AND_PUBLISHING.md). Install the local `browser-extension` folder in Brave, pair it with the code shown under a profile's **Analytics → Brave refresh**, and enable the schedule. Both Brave and the hub must be open. The extension reads displayed profile counts, not private Insights or cookies. For Instagram sources, **Brave links** can capture visible post links to use as a partial fallback when profile extraction is blocked. This is not a full archive.
