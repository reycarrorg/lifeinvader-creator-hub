# Fresh08 rejection: proven input mismatch

**Superseded next-run mapping:** Do not use the YouTube-shaped label below. The latest implementation uses truthful `lifeinvader-local://synthetic-review` with platform local; see HEADLESS_LOCAL_REVIEW_09.md. Historical diagnosis is retained.

Fresh08 is retained HELD and must not be reused. No live worker/GPU retry occurred during diagnosis.

An explicit read-only Rust predicate trace against the retained plan/database passed DB clean-path/scope, media scope, exact bootstrap config, empty workspace, and interpreter/helper/review pins. It then reproduced `source_unavailable` from `controller_source::controller_file`. Calling the same shared `source::account_profile` predicate established its precise failure: platform `youtube` paired with `https://lifeinvader.local/synthetic-review`. The source resolver accepts YouTube account hosts only. The admission's current-source binding check had accepted that serialized profile; it did not establish that file resolution would pass. Thus one reservation was held before `consume_launch`, consistent with zero recorded launches and an empty worker directory.

## Minimal correction and tested evidence

Do not weaken source resolution or replay Fresh08. The next isolated synthetic fixture changes only its profile metadata to the schema-valid offline fixture label `https://www.youtube.com/@lifeinvader-synthetic-review`, and derives a new source binding and job. This string is a synthetic schema label, not a claim that a channel exists, that the source came from YouTube, or that account/publication permission exists. Provider remains synthetic; approved source bytes/cut/review stay unchanged; headless source scheduling and external activity remain off.

Focused disposable regression reproduces invalid-host rejection, corrects only metadata, verifies the same inert file resolves, old binding becomes invalid, traversal stays rejected, and zero launches occur. PASS. No actual fixture mutation or network lookup.

Durable diagnostic now records allowlisted `prelaunch_source_unavailable` instead of generic supervisor_unresolved at this boundary; other known prelaunch categories are fixed codes and unknown text becomes prelaunch_unclassified. Test verifies HELD state, fixed diagnostic and zero launch count: PASS. Driver captures only bounded allowlisted HTTP reason/status/stage and captured-byte hash; unknown/private fields are omitted. Two sanitization tests PASS. Driver now resolves the packet path before URI use.

Changed source: src/controller_admission.rs, src/controller_manual.rs, src/controller_source.rs (regression only), src/controller_supervisor.rs (explicit ignored read-only diagnosis test only), diagnostics/headless_review_run.py and diagnostics/test_headless_error_capture.py. Source resolver policy itself is unchanged. tools/prepare-headless-review-09-DRAFT.py contains the next metadata correction but fails immediately if executed; no new plan/expiry has been issued.

## One next combined approval, not a startup-only retry

New known-source unsigned app:
`tools/headless-diagnostic-build-target/debug/lifeinvader_hub.exe`
11851776 bytes, SHA256 `938dc803bdb29d70558e635a6aaf6c9b4a5628438887a1993f78afe9087101f4`.
Build-only completed; no execution. Previous 487387 executable remains unchanged at its original path.

Updated acceptance driver SHA256 `95077deb571534bc66b18b12e6431e7771c3e8c20340cd55f139f3f602a595dc`.

Root can request one exact approval for this build, corrected fresh synthetic profile and fresh bounded plan: permitted local headless readiness, same one independently reviewed 35-second render through real worker/CPU ASR/NVENC, verified receipt/READY_FOR_REVIEW, graceful close/no descendants and one read-only restart/reopen with no second render. Publish/OAuth remain absent. New plan fingerprints must be finalized and reviewed after applicable authorization, with a new explicit deadline; old tickets are not renewed. No browser or security bypass. Any failure preserves the new state and stops without retry.

## Independent review-folder staging

`tools/relocated-review-payload-02`: create-only staging completed, 8080 pinned files / 924735727 bytes. Manifest SHA256 `9113b6d8e5b6d31baa960a7c62bc9c7dbc8ef62298dff3e975fe8465ef3a72fb`. Updated GPU/producer source pins and new NVML helper included; Windows driver DLL stays external, not redistributed. Every copied file checked during staging. No payload executable launched. runtime_qualified=false, redistribution_reviewed=false, launch_enabled=false, installer=false. Full final-folder fallback exclusion/import/DLL/model/tool qualification and outstanding license obligations remain unresolved; this folder is not yet portable or release-qualified.
