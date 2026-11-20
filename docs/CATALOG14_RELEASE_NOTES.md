# Catalog14 adapted review delta

Base: `c33bcd31f4eb4f70cf47c009bd434df797502804` on
`andre/creator-hub-adapted`. This source-only delta adds independently pinned
offline authorized-source intake, native catalog selection, read-only durable
status/provider-health displays, and create-only per-attempt diagnostic evidence.
It does not grant rights, fetch sources, activate providers, change renderer or
GPU criteria, or enable publishing/deletion. Existing Make a clip, receipt import,
preview GET, heartbeat and scoped close behavior is preserved.

## Qualified local build and evidence

Retained folder on ANDRE5090: workspace-relative
`tools/catalog-review-folder-14` (outside the Git payload). Original
`tools/embedded-review-folder-01` and held jobs are unchanged.

- EXE SHA256: `fe9dd4e772cbd0609e055d787a5c08e1ebf056847e9ac307757cf1fba9b0c2b4`.
  The retained package EXE and dedicated Cargo target EXE match.
- Payload inventory SHA256: `1fcf8f46829f7b02706903ff2d41e8df40913a74c65572db79c083c93f8aa644`.
- Exact successful packet SHA256: `c95dd1fb45b021b72b231cf5d5be79da4c8bcc71432ad8502e34b25b65672c6c`.
  Its execution window is over; it must not be replayed.
- Source audit: `review/source-audit-postbuild.json`, SHA256
  `43df4cc91b7fb945682afd209d1c5ad7f00cab6812c1e58676f3421d7ddad1ff`.
  Its 63 sorted path/SHA rows cover tracked Rust/web/Cargo/build/worker inputs
  plus the new discovery module. Canonical compact sorted-key JSON rows digest:
  `edbf1dbfad9917a538d281dca15997a0e510e1dcf0a8c3a0fd432ccda5eea85b`.
  This is a post-build source snapshot, not a reproducible-build attestation.
  Runtime-source files were unchanged after compilation; subsequent changes
  concern the diagnostic driver/tests and documentation. No rebuild or render
  is needed solely to index these files.

On 2026-11-20 at 05:16:12-34 UTC (Nov19 23:16:12-34 Central), fresh exact-GPU
samples passed 0% encoder utilization and zero sessions. App PID35220 dispatched
one real worker PID18056. Controller reached READY_FOR_REVIEW with one queue
claim/completion. Restart PID25876 served identical preview bytes and receipt
without another claim. Both app exits were zero with zero owned descendants;
driver exit zero. Evidence is retained in `state/preflight/attempt-02` and
`state/worker`. Earlier rejected GPU evidence was neither moved nor overwritten.

- Media SHA256: `f46ffecfb6b3b7f11a061d500d1fc55a976e4ee58318c9469047f41539c5c345`.
- Receipt SHA256: `6267f5830a8209910013da9a6e6f230c2b5df0026986a318572fc34bd8ed65ff`.
- 1,266,616 bytes; 38.733333s; 1080x1920 H.264/NVENC; AAC stereo48kHz,
  128k target and 117,022bps measured average. No rerender to change the average.
- Offline faster-whisper CPU requested int8, effective int8_float32.

Offline evidence includes four discovery tests, two Rust catalog tests,
read-only status/auth and preview-route tests, targeted full prelaunch checks,
final-folder runtime/CPU initialization, and focused evidence path rejection.
No hardware tests were repeated during commit preparation.

## Still missing or deliberately unavailable

This was headless byte/receipt reuse, not observed visual playback, caption
quality, audio intelligibility, automatic source selection, or in-flight worker
recovery. Full native Connect/enrollment and live provider health are not wired:
the GET display truthfully remains unconfigured, with Connect disabled. Native
publishing and Drive backup are not integrated or qualified; no OAuth dependency
is required for rendering. Deletion stays off. No installer or public binary
redistribution is claimed; local FFmpeg corresponding-source/license closure
remains incomplete. No runtime, model, media, receipt, credential, enrollment
configuration or generated package belongs in this source-only PR delta.
