# Package09: qualification and pending exact app launch

Local candidate: tools/embedded-review-folder-01. No redistributable upload or binary PR.
Final inventory manifest.final-review-09.sha256 SHA256: `90dbfde1df0c4c84fd929854ae2444c297d2b7a337c8b756f8f6a1fbbda1fad9`.

## Actual evidence

- Complete import inventory06 captured all14 previously unlisted host images in one pass. Exact System32 paths, observed/current hashes and valid Microsoft Authenticode checked in batch. Host requirements only, no Windows/driver copies, no directory trust. One compatibility-runtime image is signed Microsoft Windows Software Compatibility Publisher; other13 Microsoft Windows. Exact pins below.
- Production Rust gated_manifest -> package-local native_bootstrap.py -> package-local ASR child passed in tools/embedded-bootstrap-07. Retained2465file bindings, manifest acf06b93f91f4b6356bbed60aa3620e8561054836c3fac929d5a5c4502bf1b29. WorkerPID26352, ASRchild6544; CPU tiny.en initialized at final model path, requestedint8/effectiveint8_float32.142loaded native images allpinned; system_trust_pending=false. No transcription, media, GPU query or queue. Parent completed normally at23:24:58.015397Z/17:24:58CT, exit0, forcedfalse, active_after0.
- Final-path check08 passed9actual roles and4negative cases: original venv, absent pin, missing model and relative tool all refused. Final ffprobe -version PID28260 exit0, emptyPATH; FFmpeg metadata reused existing exact final-folder evidence. No render.
- Future producer/export recipe now128k (recipe suffix aac48-128k). Focused actual argv/export consistency test passed. Fresh13 remains124k configured/111995bps measured; target is not a guarantee of actual mean, upper-bound output validation unchanged.
- Package-only main-app route test passed: no OPS calls, no Metricool counters, no worker/admission/import/settings routes, no DB changes from inspection. Argument test passed: explicit absolute existing root/server-only required, render-plan flags rejected. Scheduler disabled, normal exact-instance readiness/close reused.

## Exact pending app launch (not executed)

New unsigned main app: app/package-review/lifeinvader_hub.exe,11896320bytes.
SHA256 `269209c125a668d304389464420df215e8b5b78ffa4cbc9677bbe88ad4bc9ffa`.
Only new runtime behavior is explicit no-authority --package-review-root mode; normal render route unchanged. Test-only helper override additionally allows package-local bootstrap qualification. Need exact action-time approval for this changed unsigned app before launching it.

Plan tools/package-startup-close-plan-09.json SHA256 `b00caf94bea185031e06112bf615b7f832c7aaf9c72eb4db2a191de1b8997fa6`.
Driver diagnostics/package_startup_close.py SHA256 `a270a1df85cc5d97d253e103ed5ad1bd8bab80b15dc426631678feb6fa08d299`; -I no-dispatch import passed. It pins its unchanged read-only process/listener helper.

One scoped command, after approval: existing trusted tools/speech-runtime/Scripts/python.exe -I -B diagnostics/package_startup_close.py <absolute plan path> <exact plan SHA>.
It creates only absent tools/package-startup-close-09 state, launches exact new app --server-only --package-review-root <fresh root>, emptyPATH, loopback127.0.0.1:43869, no browser. Verify listenerPID, zerojobs/manualfalse/publishfalse, ready, rejected writer routes, ordinary scoped close, exit0/no descendants. No rendering/model/GPU/admission grant, security changes or alternate route. If any actual OS/tool approval blocks, stop; no bypass. No prior state reused. App startup/shutdown remains UNVERIFIED until this runs.

## Source-only PR state

Source candidate inventory tools/source-only-pr-candidates-09.json prepared, not staged/committed/pushed. Its paths are restricted to source/docs/tests/diagnostics/packaging/web; excludes tools, runtimes, media, DBs, archives and controller-test-data. Refresh hashes after final documentation before staging. Portable package and PR must not be described as finished until app lifecycle passes and final diff is reviewed. No main push/merge.

GPL corresponding-source closure remains incomplete for Gyan's static dependencies. Official FFmpeg source commit946fcce07b and retained notices are documented in EMBEDDED_PACKAGE_QUALIFICATION.md. Local Andre-PC review only; do not distribute binaries or claim licensing closure. OAuth unconfigured, deletion permanently OFF in review route. Normal baseline contains inert Metricool types, but package routes do not consult/import their status; no claim of whole-binary string absence.

## Exact batch host pins (not copied)

- `C:\Windows\System32\clbcatq.dll`: `c31a5d06a426c934fb31a14b124356fd6364bb94989cabbdab3ca28e9d9d5803`
- `C:\Windows\System32\amsi.dll`: `dd7564e3d7eaade19c5e574301445adad990c2db50cfbccc65ce0bd85d1860a5`
- `C:\Windows\System32\userenv.dll`: `0211627bed22bf4ed6881a9a1e90bdf8878045510e018b7e02629cb78c5a9bea`
- `C:\Windows\System32\profapi.dll`: `38c1c272db5dfc6b7f7a14eb9f4f062e00fad3fab9a40a642543d8f9a91e2606`
- `C:\Windows\System32\setupapi.dll`: `74a265f36ff231e64a279675bb187faf8391a727ca804384fabd2d7437457b8e`
- `C:\Windows\System32\dbghelp.dll`: `547a1a6b57ca6c3d77bf8a1ab1096377c11e7c35d28622463729daf28841e578`
- `C:\Windows\System32\msvcp140_1.dll`: `206c931bf90fdad8816de3b5e2ef80b2bcaa9406c89ecc05fe6fddffe251e982`
- `C:\Windows\System32\dxgi.dll`: `84bc502f36687bb6c4dd9763d109c6ed7339767ed50a8a70707761563dd3e816`
- `C:\Windows\System32\powrprof.dll`: `096b3c77ad7fece122be0362c72530b1edc024fe7107d1d40d8bcd8fa5428a0e`
- `C:\Windows\System32\umpdc.dll`: `8172acbaf08cbf516b8bc8e567cab5aa7576d4af913ecd3918a7ed65fdf64b3d`
- `C:\Windows\System32\devobj.dll`: `53e81d1df33d9f036c12f93d3c3ae33824d2b01e324f355ffc7517d87744c39a`
- `C:\Windows\System32\cfgmgr32.dll`: `8f1d9eb7e0315ab1b07970d15016953e22aedd44fe70337ca9c592983495506d`
- `C:\Windows\System32\DXCore.dll`: `74bd73065b06e32ddedd81ade496510921a27e647b7e99f4b6494eb2812702ba`
- `C:\Windows\System32\directxdatabasehelper.dll`: `3a262619c8050c59ede854740c9cdd0fc0afdc89d5313958ac04e96285fd50dc`

## Completed exact app lifecycle

2026-11-19 23:38:12 UTC /17:38:12 Central: exact approved candidate launched once, PID30676, PASS/exit0, ordinary UI_CLOSE_REQUESTED, owned_descendants=[], zerojobs. Package read-only route refused writer endpoints; no OPS/Metricool, browser, GPU or render. Evidence tools/package-startup-close-09/result.json. Earlier pending-launch paragraphs are historical. The exact final manifest and candidate bytes remain unchanged. Local qualification is complete for bootstrap/model and no-authority app startup/close; this is not an installer, visual-playback evidence, or permission to distribute binaries.
