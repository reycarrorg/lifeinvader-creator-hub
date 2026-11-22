# Known issues and planned work

## Changed — Snapmatic local-file imports

Related work: [PR #17](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/17).

New imports previously recorded only the filename even though the file was written in the local drops folder, and the synthetic source could be rejected by source resolution. Repeated names in the same second could also overwrite a file. The Windows setup branch fixes new imports and checks them with disposable videos. Previously imported records are not migrated; investigate and preserve their files before repairing those records. Full editor captioning and social publishing are not established by this test.

## Open — Solo Post metadata and picker coverage

The picker and library hide an image only when its filename stem and post ID match a Solo video; other image files remain selectable, including possible unmatched thumbnails. Original account, platform and dates depend on the saved metadata; missing fields can remain unknown. Solo downloads use a separate archive and can duplicate downloads from other source archives. Multi-asset carousels are not yet represented as one post. The quick-editor visibility setting is stored in browser-local storage, so it may differ between browsers.

## Open — Instagram download tool validation

The app now passes its discovered FFmpeg location to yt-dlp. Offline detection and synthetic processing worked with FFmpeg and FFprobe 7.1; the owner’s next real Instagram run has not yet been verified. Archives still skip already-recorded videos, so restarting a batch does not force a higher-quality re-download. Other platform restrictions and photo support remain separate issues.

## Open — Quick editor coverage

The first editor supports centered aspect presets, one contiguous trim, mute and one uploaded PNG/JPEG overlay at 50% opacity. It does not provide arbitrary crop positioning, multi-cut timelines, built-in branding, overlay placement, cancellation or edit-project history. Encoding depends on FFmpeg and codec support; large inputs can take time and use storage. An edited copy is separate from the original, and publishing is not automated. Validate additional formats and long or rotated media before treating this as a full editor.

24-hour source counts and one-hour new-media highlights include only files tracked by Hub sync after tracking began; they are not a complete historical import count. The one-hour period begins at recorded download time, even if the user opens the source later. Original post dates may be unknown or estimated.

## Open — Profile recovery and content-library coverage

Related work: [PR #8](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/8).

TikTok account-ID recovery requires a valid post belonging to the profile and can still fail when platform access is restricted. Instagram captured-link fallback only covers posts previously visible in Brave and must be refreshed to include new posts; successful fallback is labeled partial. Photo and full account coverage have not been verified in a real sync. Library length scans read local media metadata and store lengths in the app browser's local storage; unsupported or unavailable files can remain unknown.

## Open — Computer-use posting and concurrency

Related work: [PR #7](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/7).

The queue exposes a local JSON handoff, durable activity notes and persistent per-item claims. An interrupted claim does not expire; a person must verify the platform and release it for review. Claims coordinate clients that use the contract but cannot prevent manual or external uploads. Scheduled times do not start publication, and `posted` remains an operator report rather than independent platform verification. The scheduled assistant, browser access, provider reconciliation, confirmation flow and real uploads have not been tested end to end. Older items may lack activity history from before logging was added.

Automatic selection runs only while the Hub is open. Scanned durations are matched to the source root, path, file size and modified time; moved or changed files need to be scanned again. Videos without a known duration are excluded when a length filter is active. Unknown original dates and missing metadata still limit selection.

This file tracks durable context in Git. Add reproduction steps, evidence, status, and links to GitHub issues or pull requests when available. Do not add private credentials, cookies, account exports, or personal media.

## Open — Social platform access

Profile and source sync depend on external download tools and platform access. TikTok and Instagram can restrict public profile pages or require login; the application does not currently implement social-account sign-in. Isolated fake-downloader tests do not prove a real account can sync. Preserve logs locally and avoid publishing sensitive output when investigating.

## Planned — Reference existing files in the content chooser

Profile sync records post and asset identifiers and their local file locations, but the existing manual library import still copies selected files. Design a future chooser around stable platform/post/asset identities and existing files, handle missing or moved files safely, and distinguish reuploads with different platform IDs from true duplicates. Do not claim cross-account content deduplication is already implemented.

## Planned — Secure shared use

The app currently binds to localhost and has no authenticated shared-user access. A GitHub fork shares code only, not profiles, claims, source archives, or media. Design and test authentication, authorization, backups, and private networking before offering a shared hosted deployment.

## Open — Platform and environment validation

Windows builds and synthetic tests have been exercised. A bounded live YouTube test-channel download and an Instagram reel/fallback test succeeded in disposable folders; these did not validate complete channel downloads or every account. The Apple Silicon macOS build, isolated tests and launcher startup have been verified locally. Windows data transfer and source-path migration, full GUI use, real social downloads and publication on macOS remain unverified.

## Open — Stalled cloud folder enumeration

On macOS a source-summary scan was observed blocked in a directory open while other local endpoints timed out. A candidate isolates background and interactive scans into separate bounded slots; it has been tested with simulated stalls only and needs verification after a controlled restart. A stalled interactive scan still prevents opening another source until that filesystem call finishes or the app is restarted. Other filesystem operations are not covered by this bound. The Terminal-launched app and a read-only Terminal probe also stalled opening the Solo archive, while an independent Codex-shell check succeeded; the underlying launch-context/File Provider cause is not established.

## Planned — Real account connections and analytics history

The dashboard supports manual snapshots, CSV, and opt-in Brave refresh of visible profile counts. The Brave extension must be installed and paired by the user; the real extension flow has not yet been verified end to end. It does not fetch private platform Insights, automatically fill profile photos, or publish posts. Official API access depends on each platform's permissions, account eligibility and app approval; see `ANALYTICS_RESEARCH.md`. Expand snapshot semantics and secure OAuth before advertising live analytics. Add corrections/deletion and platform-specific export mappings.

## Planned — Automated publishing and complete source picker

The source picker and local queue reference existing photo or video files; auto publishing, category-level posting schedules, AI caption drafting, whole-carousel queueing, copying video files to the clipboard, and verification of published posts against real social accounts remain unimplemented. Date filters use available original metadata or clearly labeled TikTok ID estimates; old posts may still lack dates. A bounded live YouTube test-channel download succeeded, but full-channel and account coverage remain unverified. See `POSTING_RESEARCH.md`.

## Open — Instagram account download coverage

The unauthenticated Instagram profile extractor failed during a live test. An individual owner reel succeeded, and a bounded app test verified the fallback using a supplied captured link. The Brave link capture only covers visible posts and does not establish a complete account download. Photo/carousel coverage and the installed extension flow still need validation. See `BROWSER_ANALYTICS_AND_PUBLISHING.md`.

## Open — Older post dates and online availability

The date audit can use existing sidecars, verify missing dates online where supported, or derive a clearly labeled UTC estimate from an unambiguous TikTok post ID. It does not invent a date for unknown/ambiguous IDs. Platforms may block metadata-only lookups; source ID timestamps are estimates, not guaranteed publishing dates. Audit currently checks date-related metadata, not complete missing thumbnails, captions or all other post fields.

## Open — Initial source selection and solo-link coverage

Related work: [PR #6](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/6).

Metadata-only previews depend on the provider and can fail, omit thumbnails or dates, or return a nonchronological/incomplete feed. Selection currently stops at 1,500 posts and includes the center post. Filtering preserves skipped IDs outside the downloaded archive; later unseen posts can be included for an after selection. A before selection stays limited to selected IDs. Photo sync is blocked where it could bypass the selection. Solo links use the original script's separate folder/archive, so duplicates across other source archives are not detected. Real-platform end-to-end validation, login support, retry/cancel controls, and large-account pagination remain open.


## Mac preview cache safety and external sync

The installed Mac worker manages only newly generated, registered previews in the macOS per-user cache. It preserves legacy/unregistered previews and does not evict Google Drive media. Independently review root-locality checks, including non-File-Provider third-party mirror/backup configurations and the non-atomic check/unlink race, and test real-world timer behavior with disposable media before relying on or expanding its use. Source identity uses file metadata, not a content hash; a replacement preserving those values or a stale path-only posting report may remain ambiguous. An unresolved pending path, over 500 outstanding paths, a database snapshot over 64 MiB, a database change, or inspection taking over three seconds prevents a sweep. Queue state is local and does not prove that another machine has no pending work. Browser IndexedDB previews are separate.

One streamed preview was observed without a cache registration; the initial registration failure was not logged, so its exact cause is unknown. The isolated fix retries changed source metadata, avoids adopting or overwriting old unregistered previews, and reports a generic reason. Further real File Provider validation remains necessary.

## Open — Parallel macOS test interference

On 2026-11-19, the full Rust test suite failed seven preview-cache tests under default parallel execution; all 40 passed with `cargo test -- --test-threads=1`. Investigate shared test state before relying on parallel runs. This does not establish Windows behavior.
