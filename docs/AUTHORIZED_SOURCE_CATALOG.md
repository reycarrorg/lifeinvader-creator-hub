# Authorized source catalog integration

Windows startup optionally accepts `--authorized-source-catalog <absolute-path>`
and `--authorized-source-catalog-sha256 <independently-reviewed-digest>` together.
The pin must not be generated automatically from a user-selected file. The app
leases the catalog during its bounded read and retains the parsed snapshot for
the session. Invalid supplied arguments stop startup before database migration.
No catalog means no catalog discovery capability; existing manually reviewed
source/worker admission is unchanged, not automatically enabled.

The schema is shared with `helpers/clip_worker/discover_gta6.py`:
`authorized-source-catalog-v1`, with `sources` entries containing `source_id`,
`revision`, `topic`, `origin`, `source_sha256`, `rights_basis`, and
`authorization_ref`. For the native Controller, source_id is the decimal existing
source profile ID, and revision is the exact `controller_binding` digest for that
profile/root. Topic must be `gta6` to be selected. Origins are `local-owned` or
`authorized-export`; rights bases are `owned`, `licensed`, or `permission`.
These labels are reviewed assertions, not evidence created by discovery.

GET `/api/controller/authorized-sources` returns only current matching catalog
entries. The UI filters its source picker; the source resolver independently
checks the profile binding and actual selected file SHA-256 before enqueue and
preparation. This does not replace immutable worker leases, verified intake,
sentence/cut guards, independent rubric approval, or explicit render admission.
There is no remote search/downloader, source-profile creation, or catalog-write
HTTP route. Existing source revision changes make entries unavailable.

GET `/api/controller/jobs/{id}/status-details` reports durable state separately
from worker evidence. Recorded preview hashes are not fresh verification;
preview GET still performs the existing integrity check. GET
`/api/controller/provider-auth` currently reports all providers unconfigured,
Connect unavailable and publishing off. It does not inspect another helper's
credential store, call OPS, or contact providers. No Connect POST is installed.

Qualification: two catalog Rust tests and one read-only status/auth route test
passed offline; JavaScript syntax checked. Catalog14 final-folder runtime and
one headless hardware render/read-only restart passed. See
CATALOG14_RELEASE_NOTES.md for exact evidence and remaining limitations.
The original review folder and prior held jobs remain unchanged.
