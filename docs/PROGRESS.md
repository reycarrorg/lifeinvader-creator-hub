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
