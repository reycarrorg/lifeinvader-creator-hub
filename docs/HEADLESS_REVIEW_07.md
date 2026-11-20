# Exact headless review 07 — awaiting new executable approval

No app, probe, model or encoder launched in this preparation. Fresh isolated state is `tools/native-fresh-hardware-07`: one QUEUED revision-0 job, paused Controller, zero admissions, empty worker directory. Earlier workspaces and previews untouched. An unpublished first assembly is preserved in preflight/assembly-v1; final assembly adds strict utilization agreement.

## Candidate and scope

`tools/headless-review-build-target/debug/lifeinvader_hub.exe`, 11849728 bytes, SHA256 `955b9a01f9279f3770ce740c1909e8d13734b14b77246760c0aa182c62487387`. Newly compiled unsigned known project build; previous cd7b0b approval does not cover this different executable automatically.

Normal CLI: `--server-only --headless-review --isolated-review-plan <exact plan> --isolated-review-sha256 <external pin>`. Headless mode requires both server-only and provisioned pinned plan. It launches no browser and no source scheduler; router is restricted to existing Controller endpoints. Default startup remains unconfigured. API readiness/heartbeat/close use existing same-origin local routes. These are local origin checks, not remote-user authentication; acceptance driver also checks the listener is 127.0.0.1 owned by the exact child PID before every request. No new bearer token or alternate transport.

## Final fingerprints

- Packet: `dbb3ab7ec99faa4f1f86cc5861451b3dd5ebd50f7e207a438be307df1f3e536a`
- Plan: `9f7bbf2b64332e52261866c003ae91e0b3259f2c1186d66b973dc3f54a17a319`
- Runtime: `05ff835690da4e6a09a0c21cedf52015d091956e6c69afe1566e03738e25322a`
- Driver source diagnostics/headless_review_run.py: `b8c15aacea1cb512b036a4f27fc325d9856af8eb503a113c6ea9cf1f23602307`
- Expiry: 2026-11-19 22:15:45 UTC / 16:15:45 Central. No silent renewal.
- Local port: 43863; no external host or browser.

Source remains independently reviewed SHA b86b69e755cce1a6806a15aa3792aeb5bdca46d11f8b94ed6908662b657d6593, cut 1650–36650 ms, key eecdfb4976ae653f2433463c26b9f50ffb11bda77a0d9af6cae66d0baf9af3f6.

## GPU policy and qualification boundary

Explicit operator change Sentinel_a95650a2c5208191a3f28d27c29e291f: pinned GPU UUID, fresh encoder utilization <=10%, zero encoder sessions. Desktop/WDDM residency is informational, no PID whitelist. The guard requires matching nvidia-smi and NVML utilization, matching NVML stats/session count, <=5 second observation span/freshness and supported sample interval. Unknown, unsupported, malformed, stale, conflicting evidence fails closed. Preflight/preclaim/postclaim/preencode guards remain; no own-session exception is required before encoding and none is implemented.

Official read-only NVML calls: GetHandleByUUID, GetUUID, GetEncoderStats, GetEncoderSessions(count=0 documented count query), GetEncoderUtilization. System32 nvml.dll SHA `0b8a01f48e22d0bafd285ef9191389491b9039db4b6163cf245cee82a1942b9c`, Valid Microsoft Windows Hardware Compatibility Publisher signature inspected read-only. Runtime manifest pins DLL/helper; producer retains DLL read lease. Child is bounded, isolated Python; no setters. Actual API availability on this GPU is still untested and is a first preflight HOLD condition, not assumed successful.

## One combined approved command

From repository root, after exact executable approval and only before expiry, request the official per-command execution exception (no broad prefix):

`tools/speech-runtime/Scripts/python.exe -I -S -B diagnostics/headless_review_run.py tools/native-fresh-hardware-07/preflight/combined-launch-packet.json dbb3ab7ec99faa4f1f86cc5861451b3dd5ebd50f7e207a438be307df1f3e536a`

Driver verifies pins/fresh state/listener scope, collects one bounded preflight, starts exact app, GETs Controller status, sends same-origin ready/resume and exactly one render-reviewed POST. Actual app admission/supervisor/bootstrap/queue/CPU offline ASR/NVENC/receipt verifier must reach READY_FOR_REVIEW. No fake producer in this execution. It verifies preview GET bytes against the receipt artifact, one claim and proof; closes via normal route and verifies app exit/no descendants. Only then restarts same build once with identical state, GETs same job/preview and unchanged receipt/media/claim count, then closes. No second render POST. Headless GET does not prove visible playback, listening or caption appearance; report these separately.

Any unexpected result preserves evidence and stops, with ordinary close attempted only on exact owned listener. No force or blind retry. Tool/Windows security prompts stop for the required approval; no workaround. OAuth/publishing are absent/default unavailable. No commits/PR until the real verification succeeds.

## Tests actually run

Nine prior offline diagnostic acceptance cases PASS. Five new encoder policy tests PASS; four real disposable queue/fake-query lifecycle tests PASS (preclaim refusal creates no queue; postclaim/preencode race retains one claim/no encoder; no automatic retry). One new Rust headless configuration/scheduler suppression test PASS; dedicated offline Cargo build PASS with existing dead-code warnings. Changed Python sources compiled without execution. Relocated Python stdlib-only isolated startup PASS; third-party imports/model/FFmpeg/driver execution and full relocated runtime remain unqualified.
