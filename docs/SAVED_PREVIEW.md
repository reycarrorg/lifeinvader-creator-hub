# Saved-video viewing in the main app

`lifeinvader_hub.exe --saved-preview` is an opt-in read-only viewer in the normal
application binary. Set process-local `LIFEINVADER_PREVIEW_FILE`,
`LIFEINVADER_PREVIEW_SHA256` (64 lowercase hex), and `LIFEINVADER_PREVIEW_BYTES`
(positive, at most 64 MiB) from independently recorded expectations.

It opens the selected video read-only, verifies the full size and hash, and serves
only those verified bytes from memory with byte-range playback support. On
Windows the read handle excludes write/delete sharing and rejects a leaf reparse
point. It does not open the worker SQLite database, receipt, source, profile, spool
or registry. This is file-integrity verification, not worker-completion validation
or permission to publish; the page says so. Existing Controller authority gates
remain unchanged.

The mode branches before normal database initialization, scheduler or writable
API routes. Only GET/HEAD for the page and video exist. It binds an ephemeral
loopback port and uses the normal main-app Edge window mechanism with a unique
separate browser profile. Closing that window drains/exits the server. Reopening
the main app with the same exact expectations rechecks and plays the existing
bytes; no renderer is reachable. Browser profile files are retained, not cleaned.

Qualification still needed: native build, synthetic regressions, real advancing
video/caption observation, ordinary window close and same-file reopen. No launch
or playback success is implied by this source change.

Recipe boundary: `SelectedCut::from_authenticated_profile` accepts a canonical
base or an exact matching `:cut=start-end` suffix on already authenticated input.
Construct the selection before forming/reviewing a registry record. Registry and
receipt readers reject noncanonical bases; they never normalize signed/reviewed
bytes. This branch has no production authenticated selection adapter to wire;
the constructor is exercised with the existing synthetic evidence bridge.
