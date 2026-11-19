# Private Stage 2 controller slice

Base: d3f8f43fca07f70d4bf2d20393facdd72198ef9e plus frozen overlay SHA-256 faad1348fc430754d6a20809874f3e2e1570cb5459d40874e0a005812e0350d9. This is a separate source-only scratch copy. No Stage 1 runtime/owner-pipe package, remote branch or active fixture is changed.

## Implemented

- `/controller` screen, source-card entry, explicit local enqueue, Pause/Resume, review approve/reject, safe retry control, review-required Autopilot setting default OFF, visible failure/hold reasons and separate quota views.
- Atomic version-1 controller-only SQLite schema initialization; unknown/partial controller schemas hold. No OPS schema/reservations are created or changed.
- QUEUED -> PROCESSING -> READY_FOR_REVIEW -> POSTED -> BACKED_UP. Manual approval stays READY_FOR_REVIEW with review=APPROVED. Rejection becomes HELD/review_rejected. Worker failure becomes FAILED. Safe manual retry returns to QUEUED and clears current preview/approval while retaining historical snapshots/events.
- Revision checks plus SQLite immediate transactions prevent stale actions and competing claims. Only trusted future adapters can report verified preview/post/restore facts; there are no HTTP endpoints for these claims.
- At startup: PROCESSING -> HELD/interrupted_processing, no automatic retry, controller paused and Autopilot OFF. Interrupted work remains held until the worker lane supplies reconciliation support; ordinary Resume cannot override uncertainty.
- `GET /api/controller/jobs/{id}/preview` reopens an already-stored <=16 MiB byte snapshot, verifies SHA-256 and current source-profile/root identity, and returns media or a closed hold. It does not call the Stage 1 RPC, submit, retry or write timestamps/events. Preview snapshots are written only by the future verified worker adapter. No playable-media claim is inferred from synthetic tests.
- Intake uses the existing original-source resolver. Archived, wrong-account, edited and unverified Solo inputs remain rejected. Preview/review also reject an archived or changed source identity.
- All control mutations require exact same-origin loopback headers and strict JSON. User-facing mutation endpoints cannot forge POSTED/BACKED_UP or worker completion.

## Routes

GET `/api/controller`: settings, up to 500 latest jobs, total count, quota views and disconnected-adapter flags.
POST `/api/controller/settings`: `{revision,action}` with pause/resume/autopilot_on/autopilot_off.
POST `/api/controller/jobs`: `{source_id,path,platform}` for an original source video.
GET `/api/controller/jobs/{id}`: current durable state/revision.
POST `/api/controller/jobs/{id}/review`: `{revision,action}` with approve/reject/resume.
GET `/api/controller/jobs/{id}/preview`: read-only snapshot, never submit.
GET `/api/controller/quota`: read-only local-receipt and OPS projections.

## OPS boundary (wave 4)

Read `offline-ops-durable-wave4/README.md` and matched its exact successful status fields. `controller_ops::StatusReader` / `JsonStatusReader` is the injection boundary for a trusted bridge that calls ONLY OPS `status(path, now=<aware datetime>)`, which owns mode=ro/query_only handling. The Rust caller runs this bounded synchronous adapter off the async request thread and waits at most one second. A timeout does not cancel an already-running synchronous read; the bridge must honor OPS's bounded DB behavior.

No bridge executable, path discovery, external DB opening or OPS mutations are installed in this slice. Default is unavailable/held, with NO fabricated counters. Failed or malformed status and any action-enabled response also become unknown/held. Local Resume never clears OPS pause. The existing status may include pending/reserved/unresolved capacity and provider allowances; those values are displayed separately.

Local receipts use the half-open rolling window `(now-86400, now]`, per-platform ceiling 15 and floor-zero arithmetic headroom. They are explicitly incomplete for global admission and never grant a posting reservation. OPS remains sole reservation authority. POSTED records represent verified facts already observed, so they must still count if Pause or quota changed during an in-flight publication; this function is not permission to publish.

## Integration assumptions / limits

Worker/publisher/backup adapters are absent. Autopilot preference is durable and always review-gated, but this slice starts no autonomous work. Before using adapter-only claim/ready/failed/posted/backed_up functions, integration must authenticate evidence, revalidate original-source provenance and actual content binding, enforce worker idempotency/recovery, obtain OPS reservation authority and verify exact receipt/restore bytes. In particular, ready() verifies byte/hash agreement, not independent provenance, media decoding or rights. Approval alone never publishes.

There is no destination-account chooser or account-level quota authorization in this slice: platform is the planning scope, and the publication adapter must resolve/verify the destination. Interrupted processing intentionally cannot be resumed without future reconciliation. No process killing, publishing, backup I/O, deletion, fixture arming, embedded Windows host or shutdown endpoint is introduced.

## Validation performed

15 new Rust tests passed using `cargo test --locked --offline --manifest-path .../app-stage2-controller/Cargo.toml controller --bin lifeinvader_hub` (22 existing tests filtered out). Tests cover all state outcomes, approval binding/stale actions, safe rejection/retry, SQLite reopen recovery, simultaneous claims on separate DB connections, quota edges, unknown schema, original-source restrictions and OPS unknown/paused status.

Read-only regression: repeated in-process HTTP GET previews and quota requests succeed with `PRAGMA query_only=ON`; SQLite `total_changes()` remains unchanged and the job count remains one. POST on the preview route is 405. No HTTP server process or Windows executable was launched.

`node --check web/controller.js` passed. UI is source/syntax checked, not visually rendered or native-tested. Test artifacts are synthetic and retained under controller-test-data. No old suite or Windows fixture tests were rerun.
