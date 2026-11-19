# Project progress

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

- Added a batch link box for individual TikTok, Instagram and YouTube posts, a non-syncing Solo Posts collection, a separate archive, progress, and original-account metadata where returned. The flow adapts the existing interactive script's downloader approach rather than launching its prompts.
- Added a metadata-only source preview and a centered cutoff strip with all/before/after choices. Excluded IDs remain separate from the real download archive and are filtered from subsequent syncs; a before selection remains limited to selected IDs. Post order is platform-supplied and the center post is included.
- Validated in an isolated database with a fake downloader and the test browser; no new real social content was downloaded. Restrictions: at most 1,500 preview posts, missing/unreliable ordering or thumbnails, no account login in the solo flow, separate solo archive, and selective photo sync blocked where it could bypass exclusions.
