# Browser analytics and publishing research

Updated 2026-11-18.

## What is implemented

The optional `browser-extension` is an unpacked Manifest V3 extension for Brave. It uses the user's existing signed-in browser session to visit only enabled TikTok or Instagram profile URLs in short-lived background tabs. It reads visible followers/following and, when shown, TikTok likes. Abbreviated values such as 44.6K are marked approximate. It sends numbers, approximation flags and up to 50 visible Instagram post links to the local app over loopback. It does not read or export cookies, passwords, messages or page HTML. It cannot read private Insights that do not appear on the profile page.

Connect from **Analytics → Brave refresh** for a posting profile or **Brave links** for an Instagram source. The pairing code authorizes this extension to submit these limited observations; it is stored locally in the app database and Brave extension storage. Do not share it. Use **Revoke all pairing codes** in the hub to invalidate existing extension connections, then pair again with the new code. The browser and hub must be running. Each account is opt-in, minimum interval one hour. An optional refresh after a manually recorded post is supported. A fresh observation is saved separately with time and source; missing fields remain unknown.

Browser pages change, and signed-in access can expire, show a challenge, block the extension or show a different account. The extension reports failures rather than fabricating values. Counts are profile-level public display values, not a replacement for official private Insights, reach, impressions or demographics. The extension opens a temporary background tab for each due account; it closes only its own tab. Instagram visible links are a partial page capture. If normal yt-dlp profile extraction fails, an Instagram sync can try those links using the existing download archive. This fallback may miss older posts, images and posts not present in the captured page; it must not be described as a complete profile backup. No cookies are passed to yt-dlp.

Brave setup: open `brave://extensions`, enable Developer mode, choose **Load unpacked**, and select this repository's `browser-extension` folder. Open the extension, paste the code displayed by the app, and enable refresh on each desired account. Only install a local extension after inspecting/trusting its code and requested site permissions. Disconnect in the extension popup; rotating the pairing code in the hub revokes all paired browser copies.

## Download validation

- The installed yt-dlp version on the project owner's machine was tested against a YouTube test channel using the app, a one-item bounded partial-download wrapper, and a disposable database/folder. It produced a file. This is not proof of a full channel archive or future availability.
- The owner's Instagram profile endpoint failed in yt-dlp, while a known reel URL downloaded in an isolated direct test. A separate app integration test supplied the observed reel URL as a synthetic bridge result and verified fallback sync produced a partial test file. This validates the app's fallback with one reel, not that the browser extension or a full channel download has been installed and verified.
- Both accounts' profile pages displayed counters in an existing signed-in Brave window, but it was signed in as other viewer accounts; this is a read of visible profile data, not proof that owner-only Insights or publishing access exists.

## Open-source publishing options

| Candidate | Value | Limits / decision |
| --- | --- | --- |
| [Postline](https://github.com/adeel1608/social-media-scheduler) | MIT-licensed official API adapters and a lease/idempotency queue covering Instagram, TikTok and YouTube. | Its own documentation says adapters are mock-tested and no real platform publication has been verified. It requires a separate cloud stack and owner credentials. Inspect exact source, notices and deployment assumptions before adapting pieces. Do not copy unreviewed code or expose media accidentally. |
| [Postiz](https://github.com/gitroomhq/postiz-app) | Large self-hosted scheduler with integrations, public API, analytics and calendar. | AGPL-3.0; running and integrating it introduces a separate backend/database/worker stack, and platform approvals remain necessary. Docker was not found on this machine during this investigation. |
| [OpenPlanner](https://github.com/capo-dev/openplanner) | A local Windows-oriented planner worth evaluating for UX and its connector architecture. | Verify exact license, maintenance, secure credential handling and real publishing behavior before using code or installing it. |

Official APIs still require account consent, product approvals and platform-specific media rules: [TikTok direct post](https://developers.tiktok.com/docs/en/content-posting-api-get-started), [Meta Instagram publishing reference](https://developers.facebook.com/docs/instagram-platform/instagram-graph-api/reference/ig-user/v21.0/), and [YouTube video upload](https://developers.google.com/youtube/v3/docs/videos/insert). User willingness to store credentials does not create app credentials or bypass provider review. Future credentials should use the Windows Credential Manager / OS keyring, with refresh, revoke and disconnect; never commit them or save raw cookie exports in repository files.

## Next publishing implementation

1. Choose one official provider and register an app with the owner's account. Confirm account eligibility, consent and required app review.
2. Add OS-protected credential storage, OAuth state/PKCE, token refresh and revoke; never store secrets in logs, the browser bridge, or Git.
3. Model each target independently, with due time and timezone, immutable asset reference, caption, privacy settings, upload state, retry policy and a provider post ID. Include explicit `needs_review` when an upload might have succeeded but the response was lost; do not automatically repost ambiguous results.
4. Add opt-in per-profile/category scheduling, source-rights checks and a clear review screen. Use a worker with durable leases; restart safely and avoid concurrent duplicate uploads. Respect each provider's explicit upload consent and privacy rules.
5. Use fake provider tests first, then owner-authorized real tests with an explicitly chosen target and visibility. Verify the published link and only then record success. Do not imply current queue planned times publish automatically.
