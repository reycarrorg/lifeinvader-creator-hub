# Project progress

## 2026-11-18 — Category management and source picker preview

Review: https://github.com/reycarrorg/lifeinvader-creator-hub/pull/12 (stacked on the source dashboard and editor branch).

- Category Manage now updates the name and membership together, preserves appearance and archived members, and rejects duplicate names. Category cards wrap into readable rows.
- New-media highlights expire one hour after the Hub recorded the download, unless marked seen earlier; the tile border and label show the remaining time. The separate 24-hour activity count is unchanged. Account views show and can open their existing linked folders.
- Reworked the post picker with concise primary and optional filters, source-aware choices, original details, and video/photo preview before queueing. Already queued or recorded items are excluded for the current posting profile.
- Checked renaming, hidden membership and appearance, expiration and folder path discovery with disposable data. Browser checks covered multi-category layout, renaming, highlight countdown, media preview and queueing. No live sync or publication was performed.

## 2026-11-18 — Source status, categories and quick edits

Review: https://github.com/reycarrorg/lifeinvader-creator-hub/pull/11 (stacked on the account context-menu branch).

The owner requested private visibility. The shared repository was made private again and antisolvent retained write access, verified through GitHub.

- Added source 24-hour tracked download counts, explicit dead/active state, status labels and colors, and server-enforced Sync all eligibility. Existing auto-sync timers are preserved; marking dead pauses automatic work and cancels pending jobs.
- Added category icon/color selection and local PNG/JPEG uploads, improved settings and sync log following, collapsible media/dashboard sections, and original-date-first media order.
- Added FFmpeg quick edits with centered crop, trim, mute and optional 50% logo overlay. Edits are separate files and queue entries retain original post identity while their handoff points to the edited copy. Own-profile media supports editing and download; source media can also be queued for review.
- Validated source eligibility and category images with disposable data; generated real synthetic video/audio and image edits and verified crop, trim, muted/preserved audio, output, queue and handoff. Browser checks covered the category selector and render-to-queue flow. No live sync or publishing was performed.

## 2026-11-18 — Account and category context menus

Review: https://github.com/reycarrorg/lifeinvader-creator-hub/pull/10 (stacked on the category-navigation branch).

- Profile and source tiles offer right-click and keyboard menus; category tiles offer open and member management. Account pages also have an options button.
- Removing an account archives it from the workspace without deleting files, archives, categories, queues or history. Active syncs prevent removal; pending sync jobs are cancelled. Scheduled sync and Brave refresh are disabled and remain off after restoration. Removed accounts can be restored in their own section.
- Tested with disposable data in the browser and a dedicated archive smoke test, including queue references, category membership and in-progress sync guards. No real account or media was removed.

## 2026-11-18 — Combined categories and source cutoff clarity

Review: https://github.com/reycarrorg/lifeinvader-creator-hub/pull/9 (stacked on the preceding profile sync and library branch).

- Profiles and Sources now show their own categories and account tiles together. Category dashboards retain member management; profile categories show profile tiles and sync counts, and source categories keep their batch tools.
- The first-download picker labels the newer and older sides, highlights the cutoff and marks included and skipped previews. Selection still follows platform order, which may not be chronological.
- Validated category navigation and both selection modes in an isolated local browser with a fake downloader; Rust unit tests passed. No live sync or provider interaction was started. The shared GitHub repository was changed to public at the owner’s request; new work remains on branches.

## 2026-11-18 — Profile sync recovery and source library

Shared review: [PR #8](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/8), stacked on PR #7.

- Added owner-verified TikTok profile recovery from a saved post URL. On a failed profile listing, the app verifies the uploader and retries using the secondary account ID while retaining the existing archive; the photo helper receives the recovered account URL. A bounded real metadata-only check resolved a TikTok account and listed two posts; full live sync remains unverified.
- Instagram fallback now labels captured-link sync as partial and can retry photo posts individually when the profile photo listing fails. A bounded metadata-only check retrieved one Instagram reel and one photo post; real full media sync remains unverified. Captured links cover only visible posts and can become stale.
- Profile and category dashboards now browse source media directly, with source/category/type/search/length filters and queue actions that reference existing files. A user-triggered local length scan populates a browser-local cache shared with the random picker; unknown lengths remain excluded from length filters.
- Validated with fake-downloader recovery, account mismatch and archive reuse, partial Instagram video/photo fallback, existing profile/source/queue tests, and a synthetic browser walkthrough of library filters and queueing. No live full sync was started during the tests.

## 2026-11-18 — Computer-use posting handoff

Shared review: [PR #7](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/7), stacked on PR #6.

- Added a local all-profile JSON handoff with target, source details, validated local file path when available, planned time, due classification, state, revision, record template and activity history. A profile can copy or download it. Queue edits can record notes, needs-review items and failed attempts without clearing the queue.
- Added persistent event snapshots and optional revision checks for updates; the UI and handoff use these checks. Old entries have no fabricated prior history. Activity records and exports remain local unless shared by the user.
- Validated with synthetic multi-profile queue data, missing files, due/future/unscheduled times, stale-update rejection and restart persistence; no social-media post was made. The handoff is not an autonomous publisher, provider verification, a scheduler or a lease against concurrent uploads. See `COMPUTER_USE_HANDOFF.md`.

Keep entries concise and dated. Record actual validation and remaining limitations, and link relevant issues or pull requests when available. The current code and repository history are authoritative if an entry becomes stale.

## 2026-11-18 — Initial shared project

- Created the initial Rust/Axum and SQLite application with separate posting profiles, source hubs, categories, a manual posting library, local thumbnails, source scheduling, and profile download views.
- Added per-profile download archives and metadata records as a foundation for a later file-referencing chooser. The existing manual library import still copies selected files.
- Prepared a clean fresh-install flow: no personal profiles or source folder are seeded in a new database. Local data and downloader binaries are excluded from Git.
- Verified on Windows with unit tests and isolated smoke tests for fresh start, migration, categories, source and profile sync using fake downloaders, scheduling, and local thumbnails. These tests do not establish that live TikTok or Instagram downloads work for any particular account.
- Added collaboration and AI handoff guidance. See `ISSUES.md` for outstanding work.

## 2026-11-18 — Link-first onboarding and analytics foundation

- Added account-link recognition, a compact add dialog, local profile and post analytics snapshots, CSV import, and offline Chart.js trends. CSV import is an explicit template format, not a universal platform export importer.
- Platform API access, reliable avatar lookup, publishing, automatic insights and deeper reporting remain future work. See `ANALYTICS_RESEARCH.md` for sources, license notes and the phased plan.

## 2026-11-18 — Source picker and local posting queue

- Added original sidecar metadata fields, YouTube channel-link validation, a source video picker, and a persistent queue referencing existing source files. Queue status and planned time are manual; no auto publishing was added.
- Added optional profile sync schedules and tile countdowns alongside existing source schedules. See `POSTING_RESEARCH.md` for platform/authentication constraints and remaining work.

Validation for the analytics and queue work: Rust unit tests; isolated analytics, queue, fresh-install, category, source sync, profile sync and scheduler smoke tests with synthetic data and fake downloaders. The new link dialog, manual analytics entry and source picker/queue were also exercised in a local test browser. No live account sync, upload or social-platform authorization was tested.

## 2026-11-18 — Brave analytics refresh and broader source picker

- Added an opt-in local Brave extension and paired bridge for scheduled visible TikTok/Instagram profile counts, with provenance, approximate-value labels, error reporting and optional refresh after a recorded post. It does not extract cookies or provide private Insights. The user must install and pair the extension; an installed-extension end-to-end test is still pending.
- The source picker now shuffles photos and videos and supports media type, source/category, saved duration, date and text filtering with optional ordering. Length filtering excludes files without duration metadata; selecting one carousel asset does not queue the whole carousel.
- Live bounded tests: the app downloaded from a YouTube test channel in a disposable folder. Instagram profile extraction failed; an individual owner reel succeeded and the app's browser-link fallback downloaded a partial reel in a separate disposable test. No real account was modified or post published.
- See `BROWSER_ANALYTICS_AND_PUBLISHING.md` for setup, credential handling and researched publishing components.

## 2026-11-18 — Source date audit

- Added automatic local date resolution for source listings and a separate per-source, category or all-source audit. The audit can optionally request platform metadata without downloading media; it tracks progress, cancellation, missing dates and failed checks. The app preserves archive/media files and labels TikTok ID-derived UTC dates as estimates. Queued items carry that provenance.
- Verified synthetic local and online audit, ambiguous archive suffix handling, queue updates, cancellation and unchanged media/archive contents. Limited metadata-only live checks were performed without downloading media; platform access may fail. Source ID estimates can differ from published dates, especially around day/timezone boundaries.

## 2026-11-18 — Shared agenda and AI handoff

- Added `TODO.md` with the product direction, current implementation and validation boundaries, prioritized follow-up work, and a workflow for both collaborators and their AI assistants. Collaborators can adapt their own forks while proposing shared changes through review.
- Corrected stale notes about the bounded YouTube test, date audit, photo selection and Brave refresh. Documentation-only change; verified references and consistency against the current repository and prior recorded tests.

## 2026-11-18 — Solo links and initial source selection

Shared review: [PR #6](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/6).

- Added a batch link box for individual TikTok, Instagram and YouTube posts, a non-syncing Solo Posts collection, a separate archive, progress, and original-account metadata where returned. The flow adapts the existing interactive script's downloader approach rather than launching its prompts.
- Added a metadata-only source preview and a centered cutoff strip with all/before/after choices. Excluded IDs remain separate from the real download archive and are filtered from subsequent syncs; a before selection remains limited to selected IDs. Post order is platform-supplied and the center post is included.
- Validated in an isolated database with a fake downloader and the test browser; no new real social content was downloaded. Restrictions: at most 1,500 preview posts, missing/unreliable ordering or thumbnails, no account login in the solo flow, separate solo archive, and selective photo sync blocked where it could bypass exclusions.
