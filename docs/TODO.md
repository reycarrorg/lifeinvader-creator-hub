# Shared to-do and project agenda

## Snapmatic public publishing

- Configure the hosted token exchange for creator installations, complete TikTok's Direct Post audit, and verify the first approved public post with its creator.
- Verify a public YouTube upload in the channel's Shorts tab and confirm that the Reel and X receipts link to the intended accounts.

Last reviewed: 2026-11-18. This is a working agenda for both collaborators and their AI assistants. It records the owner's current direction, not a requirement that every fork look or work the same way. Your own ideas, channels, platforms and design changes are welcome. Agree on shared behavior before merging changes that affect both installations.

Use the boxes for **future work**, not as a claim that a feature is finished. Priorities are a suggested order and can change. Check the code, `PROGRESS.md`, `ISSUES.md` and recent pull requests before starting: this list is a handoff, not a substitute for the current repository.

## Product direction

Build a free, local-first Windows creator hub that helps a person manage their own posting profiles and separate content sources: collect and organize media they have permission to use, understand original posts and account performance, choose content, prepare a queue, and eventually publish and verify posts across accounts. Keep the first view simple and support categories for different projects or channels. Reuse suitable open-source components where they help, checking licenses, maintenance, platform permissions and deployment costs first. A fork may pursue a different design or use case.

The full vision is **not finished**. In particular, a scheduled item does not post itself, private account Insights are not connected, and a paid service's entire feature set is not present. Do not imply otherwise in UI, release notes or AI handoffs.

## What exists today

| Area | Current implementation | What has not been established |
| --- | --- | --- |
| Accounts and categories | Separate profile and source views, categories, link-based name/platform detection, optional local profile pictures. | Reliable automatic profile-picture retrieval and actual account authentication. Platform access and link recognition can fail. |
| Downloading | Source and profile sync use external tools and download archives; TikTok profiles can retry with an owner-verified account ID from a saved post, and Instagram can retry previously captured links with partial coverage. A separate Solo Posts hub accepts individual links. New sources can preview posts and select a cutoff. | Complete live account sync remains unverified. Captured Instagram links can become stale, and previews can be incomplete. Selection is capped at 1,500 posts. Solo posts have a separate archive; photo sync is limited for selective sources. |
| Dates and metadata | Source listings read sidecars; date audit can check locally or request missing dates online without re-downloading media. Unambiguous TikTok IDs can yield labeled UTC estimates. | Full metadata recovery for every file; estimated dates are not confirmed publishing dates and can differ around day/timezone boundaries. |
| Selection and queue | Picker shuffles source photos/videos with source, category, type, date, text and available-duration controls. Optional per-profile rules queue random matching files on a timer while the Hub is open. The handoff provides revision-checked results and persistent claims. | A working scheduled assistant and actual posting are not yet validated. Automatic success verification, a full carousel as one selection, and duplicate detection across different post IDs remain unfinished. Manual library import still copies files. |
| Analytics | Manual snapshots, template CSV import, charts, and an optional paired Brave extension for visible profile counts. | Installed-extension end-to-end verification, private Insights, full account connection/OAuth, universal platform exports and automatic metrics for all posts. |
| Collaboration | Private GitHub upstream, branch/fork and PR workflow, issue/progress notes and research documents. | A GitHub clone does not include either person's local database, profiles, Drive folders, downloader executables or media. Access/invitation state can change and should be verified. |

Evidence so far includes isolated unit/smoke tests, a bounded live YouTube test-channel download, an Instagram reel and partial browser-link fallback test, and limited metadata-only date checks. These are not proof of complete channel downloads, successful publishing, private Insights, every platform/account, or every operating system. See `PROGRESS.md` and the research documents for details.

## Suggested next work

### 1. Make the existing experience dependable

- [x] Include Solo Posts in the source picker and library with original account/platform where available, and make quick-edit controls optional. Verify remaining missing metadata and carousel behavior separately.

- [x] Resolve the bundled or configured FFmpeg executable for yt-dlp calls, including Instagram individual posts and source sync; test discovery and arguments using disposable data. Live platform retry remains to be confirmed.

- [x] Allow category renaming while preserving membership and category appearance, improve the multi-category layout, show a one-hour new-media countdown, and open linked account folders.
- [x] Provide a clearer source picker with filtering, original details and playable previews before queueing; validate real-platform coverage separately.

- [x] Show source sync states and 24-hour tracked additions, add user-set dead status, restrict Sync all to auto-enabled active sources, and expose category icon/color/image settings. Validate behavior against disposable data.
- [ ] Validate the new quick editor with a wider range of formats, portrait/landscape videos and audio on collaborators’ machines; add interactive crop positioning, richer trimming and saved logo presets if needed. The current editor uses centered crop presets and an uploaded overlay.

- [x] Add profile/source and category context menus with reversible account removal, including preservation of local files, history and category membership. Automatic sync and Brave refresh stay off on restore.

- [x] Bring each role’s categories into its Profiles or Sources view, with a profile category dashboard and a clearer source cutoff selector. Validate using disposable data; real-platform feed order may differ.

- [ ] Verify the Brave extension end to end on an authorized test installation: installation, pairing, initial refresh, schedule, restart, disconnect/revoke, rounded counts, errors and after-post refresh. Keep cookies and platform credentials out of logs and Git; the current extension does not extract them.
- [ ] Validate solo-post links and source previews against explicitly authorized real accounts, including restricted posts, thumbnails, photos and source ordering; add safe pagination beyond 1,500 and cross-archive duplicate detection.
- [ ] Validate source and own-profile sync on explicitly authorized test accounts and representative media, including TikTok, Instagram videos/photos, YouTube and restricted/unavailable cases. Document what actually downloaded, failed or required access. The current Instagram visible-link fallback is partial.
- [ ] Investigate remaining missing thumbnails and profile pictures; use existing local files for previews and do not re-download archived videos just to create a thumbnail. Add a safe automatic avatar lookup only where it is reliable, with a manual fallback.
- [ ] Verify complete real-account TikTok and Instagram sync with the owner, including post/photo access and refreshed browser links; the metadata-only checks do not establish complete coverage.
- [ ] Verify upgrades against disposable copies of real-format databases and media layouts; preserve settings, archives and existing media. Test disconnected/moved folders, interrupted sync, and concurrent operations.
- [ ] Improve source date coverage and other missing metadata (titles, descriptions, links, duration) without overwriting originals. Preserve provenance and distinguish confirmed dates, ID estimates, file/download dates and unknowns. Keep online audits opt-in and cancellable.

### 2. Make the content picker and queue a complete workflow

- [x] Add opt-in per-profile auto queue rules with saved filters, interval and queue limit; show the countdown and supply per-profile assistant instructions. Automatic and manual length filtering use metadata or matching scanned durations. Validate live social posting separately.
- [x] Add a structured handoff result input and persistent per-post claim with review recovery after interruption. Validate the complete scheduler, browser session and real upload before relying on unattended operation.
- [ ] Connect source downloads, the manual library, and own posted content using stable platform/account/post/asset identities and **references to existing files** where possible. Do not silently duplicate video files or claim that different post IDs are the same video. Handle moved or unavailable files.
- [ ] Represent a multi-photo carousel as one post with its ordered assets; make photo and video selection, preview, duration and date filters work with incomplete metadata. Keep filtering and shuffle straightforward.
- [ ] Improve the queue manager: edit/remove/reorder or reschedule safely, distinguish draft/queued/posted/failed/needs review, show original title/description and source/post links, copy useful text, and preserve history. Items remain active until their status is recorded or checked off.
- [ ] Make the launch cutoff and date/timezone handling explicit and configurable. Do not assume the example cutoff is the actual GTA launch time, and do not treat date-only or estimated timestamps as exact.
- [ ] Build and validate an AI-assisted workflow that inspects media, drafts captions and operates an authorized posting flow. The local handoff and activity log do not themselves publish or verify anything. Persistent claims coordinate compliant operators; they do not prevent manual or external posting and require human recovery if interrupted.

### 3. Build real analytics and secure connections

- [ ] Choose the first provider and define the metrics it actually exposes, account eligibility, developer registration, scopes, authorization, refresh/revoke, limits and app-review requirements. Build and validate one connector end to end before generalizing.
- [ ] Use official OAuth where available and protect credentials with the operating system's credential store. The owner is open to signing in and saving credentials when needed, but this does not authorize putting secrets, browser-cookie exports or personal account data into Git, logs or shared issues. Support disconnect and revoke.
- [ ] Separate account vs post metrics, lifetime totals vs daily activity, actual vs rounded values, unavailable vs zero, and observation time vs post time. Show source and freshness; never synthesize missing data or combine incompatible platforms into a misleading total.
- [ ] Add real provider-backed profile and category dashboards, useful date ranges, post performance, follower trends and export/import mappings where authorized data supports them. Add corrections, deletion and backup/restore for analytics history.
- [ ] Continue evaluating open-source building blocks. `ANALYTICS_RESEARCH.md` and `BROWSER_ANALYTICS_AND_PUBLISHING.md` record candidates and tradeoffs; validate current project health, license obligations and service costs before adopting code.

### 4. Prepare reliable scheduled publishing

- [ ] Pick one provider and a concrete, authorized test account. Research its current posting API, supported media, scopes, account eligibility, privacy rules and app approval before implementing. Consider an approved browser-assisted workflow only if explicitly requested and appropriate for that platform.
- [ ] Implement durable per-target jobs with timezone-aware due times, opt-in profile/category settings, a clear preview/approval path, and separate preparing/uploading/published/failed/needs-review states. Decide whether jobs run only while the app is open or through an explicitly configured background service.
- [ ] Make retries and restarts safe: use leases/idempotency where supported, reconcile an ambiguous upload before retrying, and never mark a post successful merely because a button was clicked. Save the actual provider post ID/link and confirmed status independently for each destination.
- [ ] Add tests with fake providers and deliberate failures first. Perform a real upload only when the user has explicitly chosen the account, content and visibility; verify it on the platform and then record the result. Nothing in the existing queue authorizes an immediate post.

### 5. Make sharing and releases easier

- [ ] Validate a credential-safe Windows-to-Mac transfer and path remap on disposable fixtures, coordinate a single owner for queue/download writes, then separately verify a real handoff with the owner's data.
- [ ] Provide a repeatable Windows build/release and a friend-friendly first-run setup with no preloaded personal profiles or paths. Validate database migration, backup and recovery before replacing an installed executable. macOS support needs separate verification.
- [ ] Decide whether the collaborators need shared remote queues/data. The current server is localhost-only and unauthenticated; a GitHub fork shares code, not app data. Design authentication, authorization, backups and private networking before exposing any shared service.
- [ ] Improve accessibility, small-window layout and usability without forcing either collaborator's personal workflow on the other. Track different ideas in branches or issues and agree on what belongs upstream.

## How to use this list together

1. Before a task, read `AGENTS.md`, this file, `COLLABORATION.md`, `PROGRESS.md` and `ISSUES.md`; inspect current code and Git state. Ask the person you are helping for their own goal when it differs from this agenda.
2. Choose a small outcome and note dependencies. Create or reference an upstream GitHub issue when useful; do not invent issue numbers. Work in your fork or a focused branch and open a PR so the other person can review it.
3. When you finish or learn something, update this list, `PROGRESS.md` and relevant `ISSUES.md` entries in the same change. State what was implemented, what was actually tested and what remains uncertain; link real PRs/issues when known.
4. Do not check off an entire item just because a prototype or one account works. Split it into smaller items when needed. Keep personal branches and preferences distinct from agreed shared defaults.
5. Never commit local data, media, downloader binaries, account exports, credentials, cookies, logs or archives. Use synthetic fixtures for routine tests; keep live-account tests and publishing explicit and narrowly scoped.

Detailed references: [collaboration](COLLABORATION.md), [known issues](ISSUES.md), [progress](PROGRESS.md), [analytics research](ANALYTICS_RESEARCH.md), [posting research](POSTING_RESEARCH.md), and [browser analytics and publishing](BROWSER_ANALYTICS_AND_PUBLISHING.md).


- [ ] Independently review the installed macOS generated-preview cache integration, especially unknown third-party mirror/backup cases, cross-machine queue state, and its timed runtime behavior; validate further on disposable media before relying on or expanding its use. Do not claim Google Drive file eviction from thumbnail cleanup.
