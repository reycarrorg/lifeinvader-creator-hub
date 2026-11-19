# Fresh13 headless native render result

Completed 2026-11-19 23:12:22-23:12:43 UTC (17:12:22-17:12:43 Central). One production Controller admission, real standalone queue/CPU faster-whisper/NVENC, verified receipt, graceful close and one read-only restart/reopen passed. This does not establish visible playback, audible speech or visual caption quality.

## Exact identity and evidence

- App SHA256: `49cd51396239f728e171e57ddff856b0414a8b781198a89f2b4de27f3dcdc8ea` (unchanged approved build).
- Packet SHA256: `d19d5da84cde0463a30bc929619aeb4c15f6daef594da79cfed13d81f491e634`.
- Plan SHA256: `809bea9b780ff6d6f07aad31ec8b823427051f42da897a8414d4e9e954bfed24`.
- Runtime manifest SHA256: `4913385b500cfef22f1255dd673e85f3caa74c7bb264dabb8596af344d21dc3d`.
- Source SHA256: `b86b69e755cce1a6806a15aa3792aeb5bdca46d11f8b94ed6908662b657d6593`; independently reviewed synthetic cut 1650-36650 ms, score 5/7 and mandatory synthetic-scope flags.
- Job: `isolated-5328db2e59d44e5ead906486e1a39d75`; admission `7775c70e-dbe3-49f8-9516-6f97136b384c`.
- Media SHA256: `3289528227d8c76507f6b9084712e55a1fdd4720f87825d0f7a6d4268560ff7d`, 1,113,128 bytes.
- Receipt SHA256: `572987ee41102777d562fa92da09c54cff978b42a61e5a7c49aeb93ce0ab5aa1`.
- Evidence stays ignored/local in `tools/native-fresh-hardware-13`; primary log `preflight/headless-run.jsonl`, retained probe `worker/producer-evidence.json`, ASR `worker/asr-evidence.json`. No media/DB/runtime payload belongs in Git.

## Observed acceptance

App PID28964 started23:12:22.679886Z; one POST23:12:22.912463Z. Verified preview GET23:12:43.212384Z, claim count1, Controller READY_FOR_REVIEW revision2, durable launch count1. App exited0 at23:12:43.436577Z with zero owned descendants. Same-build restart PID17544 reopened identical receipt/media at23:12:43.685920Z and exited0 at23:12:43.726951Z, zero owned descendants. Native phase log records child PID19580; no claim of separately captured ASR/encoder PIDs.

Actual output: 35.000s, 1080x1920, H.264 High/yuv420p, 15fps/525frames; encoder tag h264_nvenc. AAC-LC stereo48kHz, measured111995bps (below128kbps). Actual CPU ASR uses faster-whisper1.2.1/CTranslate2 4.9.0/NumPy2.5.5, requested int8/effective int8_float32, local tiny.en. Word timings/ASS generated; recognition has imperfections and was not listened to. No browser launched and no visual-caption/playback claim.

Pinned GPU UUID, fresh agreeing <=10% encoder utilization and zero active sessions passed at required stages; recorded preclaim/preencode values0%/0sessions. WDDM resident rows remained informational. No guard relaxation or full environment inheritance. Previous held workspaces were preserved.

Before launch the exact production no-dispatch proof passed with8137 retained leases, actual startup initialization and stale-Fresh09-path negative. Runtime diff lists six job-local path rebindings, preparation-root bytes and reviewed_job validation extraction only; unexpected entries0. ProgramFiles-only environment fix was already approved. No retry or second render.

## Package / source-only PR gates

Official embedded Python3.12.10 relative _pth plus24 pinned offline wheels is staged, not a qualified portable app. Import-only image capture identified `C:\Windows\System32\uxtheme.dll`, SHA256 `e2e972e21a16c62e7c678334dc50f33c2f327c1b6854a2fb096ca244ae650f27`, valid Microsoft Windows signature. It remains NOT admitted. Exact-pin review and completion of native inventory are required; never trust a directory wholesale.

Also pending: production bootstrap ZIP-stdlib lease/origin policy, final-folder worker/model/tool proof with original-path fallback excluded, final app payload, GPL corresponding-source/notices closure. FFmpeg reports GPLv3-or-later/static/no nonfree; this alone is not distribution clearance. OAuth/publishing remains unconfigured; no social workflow needed. Commit/push/PR waits for required Phase1 package qualification; source-only diff must exclude tools, models, media, DBs and credentials. No main push/merge.
