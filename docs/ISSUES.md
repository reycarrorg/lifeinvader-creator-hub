# Known issues and planned work

This file tracks durable context in Git. Add reproduction steps, evidence, status, and links to GitHub issues or pull requests when available. Do not add private credentials, cookies, account exports, or personal media.

## Open — Social platform access

Profile and source sync depend on external download tools and platform access. TikTok and Instagram can restrict public profile pages or require login; the application does not currently implement social-account sign-in. Isolated fake-downloader tests do not prove a real account can sync. Preserve logs locally and avoid publishing sensitive output when investigating.

## Planned — Reference existing files in the content chooser

Profile sync records post and asset identifiers and their local file locations, but the existing manual library import still copies selected files. Design a future chooser around stable platform/post/asset identities and existing files, handle missing or moved files safely, and distinguish reuploads with different platform IDs from true duplicates. Do not claim cross-account content deduplication is already implemented.

## Planned — Secure shared use

The app currently binds to localhost and has no authenticated shared-user access. A GitHub fork shares code only, not profiles, claims, source archives, or media. Design and test authentication, authorization, backups, and private networking before offering a shared hosted deployment.

## Open — Platform and environment validation

Windows builds and synthetic tests have been exercised. A macOS build and live social-platform sync have not been validated for this version. Verify each target environment and report actual results.

## Planned — Real account connections and analytics history

The dashboard supports manual snapshots, CSV, and opt-in Brave refresh of visible profile counts. The Brave extension must be installed and paired by the user; the real extension flow has not yet been verified end to end. It does not fetch private platform Insights, automatically fill profile photos, or publish posts. Official API access depends on each platform's permissions, account eligibility and app approval; see `ANALYTICS_RESEARCH.md`. Expand snapshot semantics and secure OAuth before advertising live analytics. Add corrections/deletion and platform-specific export mappings.

## Planned — Automated publishing and complete source picker

The source-video picker and local queue reference existing files; auto publishing, category-level posting schedules, AI caption drafting, photo/carousel queueing, copying video files to the clipboard, and verification against real social accounts remain unimplemented. The date filter only uses available original metadata, and old posts may lack it. YouTube links are accepted but live YouTube sync has not been tested. See `POSTING_RESEARCH.md`.

## Open — Instagram account download coverage

The unauthenticated Instagram profile extractor failed during a live test. An individual owner reel succeeded, and a bounded app test verified the fallback using a supplied captured link. The Brave link capture only covers visible posts and does not establish a complete account download. Photo/carousel coverage and the installed extension flow still need validation. See `BROWSER_ANALYTICS_AND_PUBLISHING.md`.

## Open — Older post dates and online availability

The date audit can use existing sidecars, verify missing dates online where supported, or derive a clearly labeled UTC estimate from an unambiguous TikTok post ID. It does not invent a date for unknown/ambiguous IDs. Platforms may block metadata-only lookups; source ID timestamps are estimates, not guaranteed publishing dates. Audit currently checks date-related metadata, not complete missing thumbnails, captions or all other post fields.
