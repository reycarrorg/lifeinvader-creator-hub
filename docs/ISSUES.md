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
