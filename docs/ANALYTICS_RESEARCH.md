# Profile analytics: research and delivery plan

Updated 2026-11-18. This is a plan and an evidence log, not a claim that every feature is implemented.

## Goal and current delivery

Build a free, local-first creator dashboard for the user's own posting profiles. The first implementation records dated account and individual post snapshots, imports a documented CSV format atomically, displays the latest available values, changes between observations, a trend chart and the latest available post snapshots. Existing download archives and source folders are untouched. Account and post numbers are never combined. Unknown values are shown as unknown rather than zero.

The optional Brave extension can refresh visible TikTok and Instagram profile counts when paired and enabled; see `BROWSER_ANALYTICS_AND_PUBLISHING.md`. It does not connect to official private Insights, schedule or publish content, or offer all Metricool features. Downloading your posts is separate from obtaining analytics. Enter/import snapshots only: a daily activity count is not the same thing as a lifetime total, and different platforms may define metrics differently. Each metric's latest observation can be from a different date. A later edit to the same profile/date/post retains other existing nonblank numbers, so use the correct date and scope.

## Research: reusable projects

| Project | Purpose and decision | License / implications |
| --- | --- | --- |
| [Chart.js](https://github.com/chartjs/Chart.js) | Vendored 4.5.1 for local charts; no CDN or network needed at runtime. | MIT; copyright/license included at `web/CHARTJS-LICENSE.md`. |
| [Rust csv](https://github.com/BurntSushi/rust-csv) | CSV parsing, including quoted fields and validation, via Cargo. | MIT or Unlicense; Cargo pulls dependencies during build. |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | Already used as an external executable for accessible public post downloads and metadata; retain its archive and original IDs. Public metadata is not a substitute for private Insights. | Refer to project's license and bundled dependency notices; executable is not in this repo. |
| [gallery-dl](https://github.com/mikf/gallery-dl) | Already used optionally for accessible Instagram photos. | GPL-2.0; external tool installed separately. |
| [Postiz](https://github.com/gitroomhq/postiz-app) | Evaluate as a separate self-hosted scheduling product or integration reference if publishing becomes a priority. It is not embedded in Lifeinvader. It still depends on each provider's authorization and available APIs. | AGPL-3.0; its code has strong sharing obligations. Review compatibility before copying or combining it. It also brings a separate application stack. |

Open source can replace expensive software components, but it cannot make private platform data accessible without the platform's authorization or remove API limits.

## Platform feasibility

- [TikTok Display API](https://developers.tiktok.com/docs/en/display-api-get-started?enter_method=left_navigation) requires a developer account, approved Login Kit and API products, granted scopes and account authorization. [Scope documentation](https://developers.tiktok.com/docs/en/scopes-overview) distinguishes permissions. Availability of detailed metrics depends on approved scopes and products; do not assume all insights are exposed.
- [Meta Insights](https://developers.facebook.com/docs/insights/overview) and [Instagram Insights endpoints](https://developers.facebook.com/docs/insights/references) cover professional account and media metrics. Access levels, permissions, app review and potentially business verification apply; personal accounts and every metric cannot be assumed to qualify.
- [YouTube Analytics reports](https://developers.google.com/youtube/analytics/reference/reports/query) require user authorization and appropriate scopes; Google Cloud credentials are needed. Quotas and provider rules apply.
- Public pages and downloader metadata can be blocked, incomplete, or change without notice. Never ask users to paste session cookies into GitHub or promise that public metadata provides private analytics.

## Recommended implementation phases

1. **Account onboarding:** link-first platform/handle detection is implemented. Add optional best-effort public display-name/avatar resolution with strict host/redirect limits, timeouts, a manual fallback, and tests for unavailable/private accounts. Add explicit canonical account IDs and duplicate detection; a handle can change. Categories are personal choices and should not be guessed.
2. **Reliable history:** capture platform, immutable account/post ID, metric, numeric value, unit, scope (account/post), interval (lifetime/day), observation time, source, and retrieval status. Separate unknown/unavailable from zero; preserve raw provider metadata locally when appropriate. Add non-destructive correction/delete and export. Current CSV schema is an interim snapshot format, not a universal platform export parser.
3. **Account connections:** implement one provider at a time using its official desktop OAuth flow, scope-specific consent, secure OS credential storage, refresh and revocation. Do not store tokens in SQLite, source code, logs, Git or the browser. Verify account ownership and eligibility, app approval, rate limits, refresh failure and disconnect behavior before enabling automatic jobs.
4. **Analytics views:** date ranges, impressions/reach, engagement, follower history, top content and per-platform reporting only where meaning and permissions are known. Preserve metric definitions, distinguish daily increments from lifetime values and avoid aggregating incompatible counts. Show freshness, gaps, and data provenance. Add category summaries only when the underlying metrics are genuinely comparable.
5. **Content workflow:** link downloaded assets and existing library items through platform/post/asset identity, avoid copying videos, account for missing/moved files, and avoid claiming identical content detection from IDs alone. Do not modify source archives for analytics.
6. **Planning/publishing:** research provider-supported scheduling and upload APIs, authorization and platform policies before promising free publishing. Consider Postiz as a separately deployed integration, after license/operational review. A local planner can be built without pretending it publishes.
7. **Release discipline:** use synthetic accounts and fake APIs for automated tests, then opt-in tests with authorized accounts. Test Windows installation and backup/migration, OAuth revocation and provider outages. Never run real account sync during tests.

## CSV format

Use **Analytics → CSV template**. UTF-8 CSV columns are `date,post_id,followers,following,views,likes,comments,shares`; date is `YYYY-MM-DD`, counts are nonnegative integers, and blank means unknown. Leave `post_id` blank for account-level snapshots. For a post row, fill its stable ID and leave followers/following blank. At least one metric must be present. Same profile/date/post imports update that record without duplicating it; blank columns preserve prior values. Invalid files are rejected before any rows are committed. Limit: 2 MB and 10,000 rows per import. Generic platform exports may need their columns converted to this format first; verify whether their values are cumulative snapshots before entering them.
