# Windows first-review delivery target

Scope: choose the reviewed original source, explicitly render one captioned clip, close/restart and reopen the same verified artifact. No social OAuth dependency, publisher panel, account setup, upload or posting.

Target: per-user Windows x64 installer with versioned app/worker payload and durable DB/queue/media outside it. Current deliverable is a create-only local review payload, NOT an installer. No Inno Setup/NSIS/WiX compiler was found on the current PATH or checked Inno locations; no compiler was installed.

The reviewed runtime remains explicit installed Python plus pinned packages/model/FFmpeg and process-local DLL search controls. Do not blindly relocate the venv or silently download runtime. Bind final installation paths and hashes before launch. Normal startup denies native admission. A scoped session requires externally supplied `--isolated-review-plan <absolute plan.json> --isolated-review-sha256 <independently approved raw SHA>`; the UI action is separate and never fires on loading the plan.

The plan schema `isolated-controller-manual-review-v1` contains exact request, RuntimeBinding, executable/helper/approval paths, isolated DB/media, seconds<=180 and publishing_enabled:false. The independently supplied plan digest is not taken from the plan itself. It must describe the exact already queued Controller job/revision, and the bootstrap config must bind the same request. Use a fresh isolated DB and source registry, never the user's normal database or V2. No completed evidence may be fabricated during provisioning.

## Checks to run once authorized

1. Fresh per-user isolated data root: startup with no plan has no render capability and no social prompts/network work. Explicit reviewed plan must match DB/media/root and raw pins before migration/opening.
2. Make a clip -> Controller -> Render reviewed clip: one exact manual admission, one durable worker claim, CPU offline ASR and NVENC, verified receipt -> READY_FOR_REVIEW. Wrong job/revision/expiry/config stays HOLD.
3. Close while idle: exact window/app exits. Close during owned work: cooperative cancellation, bounded Job cleanup, zero descendants and durable HOLD; no retry. Do not substitute completed-result restart for in-flight recovery evidence.
4. After completion, close/restart and use the unchanged GET preview route to reopen the same job/artifact. Verify receipt/media hashes and zero new submissions/render starts; repeated GETs must remain read-only.
5. Upgrade/rollback: close ownership first; preserve full durable workspace or supported consistent SQLite backup including uncheckpointed WAL transactions. Never copy a live main DB alone, erase a claim or overwrite evidence. Keep old payload available; no automatic rerender.
6. Retain UNKNOWN/HOLD and verified receipt status. APP's native-status slice may be integrated separately; no Connect UI, publisher panel or native-mode handler. Existing preview functions stay unchanged.

These checks are planned; live hardware/restart/install acceptance has not passed yet. Packaging must wait for that evidence before being advertised as finished.

Installer compiler route: official NSIS3.13 Windows compiler, per upstream download page https://nsis.sourceforge.io/Download and licensing https://nsis.sourceforge.io/License . It is a free/open-source route suitable for a user-level installer; no paid subscription is needed. No compiler is currently installed in checked locations and none was downloaded or run. packaging/review-installer.nsi is draft source only, uses RequestExecutionLevel user, versioned per-user payload and no autorun/settings/DB/media writes. Installer compilation, signature/provenance check, clean isolated install and rollback acceptance remain unperformed. This source is not a verified installer.

## Current qualification (supersedes early planned-status text above)
See docs/PACKAGE_09_HANDOFF.md and docs/HEADLESS_RENDER_13_RESULT.md. Actual production headless one-render/restart passed with installed pinned runtime; official embedded final-folder bootstrap/CPUmodel and exact new package-only app startup/close passed separately. No social OAuth or Metricool runtime call in package routes. Local review folder is not an installer or redistributable package; FFmpeg full static corresponding-source closure remains unavailable. Fresh13 used124k AAC, future source default128k is argv-tested only, no repeat render.
