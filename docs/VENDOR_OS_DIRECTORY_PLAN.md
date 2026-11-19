# Remaining OS-directory diagnostic plan — no execution

Prior evidence: full inherited context succeeded outside the added app Job; explicit minimal env failed. Actual Rust-contained minimal, WINDIR, System32-only PATH, and both all failed identically. Those conditions will not be repeated. This identifies environment dependence but not the necessary key.

Machine-only keys confirmed present and matching OS-known folder/volume categories (no values logged): ProgramData, ProgramFiles, ProgramW6432, SystemDrive. ProgramW6432 is included because NVIDIA documents the standard-driver NVML directory using it; this system's pinned DLL is the DCH System32 path, so this is a hypothesis, not an established dependency. ProgramData and SystemDrive are machine data/root anchors. No USERPROFILE, LOCALAPPDATA, APPDATA, account tokens or arbitrary inherited values.

One proposed bounded contained diagnostic, at most four vendor invocations:
1. Add those four OS-verified keys together to existing minimal env; TEMP/TMP remain a fresh owned directory. No WINDIR/PATH retest or full inheritance.
2. Only if step1 returns valid exact UUID inventory, omit ProgramData.
3. Only if outcomes remain classified/bounded, omit ProgramFiles and ProgramW6432 as one category from step1.
4. Only if outcomes remain classified/bounded, omit SystemDrive from step1.
Stop immediately on failed combined candidate, timeout, malformed/unclassified output or cleanup ambiguity. Each omission starts from identical step1 env. Prior minimal control is retained evidence, not rerun. Five seconds per invocation, existing Rust Job/manifest/GO + phase capture, zero owned descendants required. No queue/model/media/render. This plan is not implemented or dispatched and does not change production admission.

A category whose omission reproduces failure is a candidate dependency, not sufficient proof for production until the exact minimal retained combination is separately qualified. Do not assert the combined set is minimal merely because it succeeds.

Architecture comparison:
- Preferred: identify the missing machine-only environment dependency; least code change, preserves existing dual sensor/race checks in worker.
- In-process pinned NVML: absolute System32 DLL loading avoids nvidia-smi's executable-specific resolution path, but NVML may have the same init dependency. Existing stats/session/util calls can be reused only after contained qualification. Replacing current nvidia-smi/NVML agreement would change the sensor contract; not silently authorized or implemented. A native DLL call needs the existing bounded owner Job controller for hangs, not an unbounded HTTP call.
- Owner-context query at each gate: possible only as a new narrowly reviewed request/reply mechanism, exact job/request+phase+single-use challenge, immediate freshness, pinned owner/worker lifecycle and zero authority beyond a one-time telemetry answer. Worker still enforces thresholds and HOLD independently; no cached startup reading, automatic authorization, inherited environment or general command runner. Adds an authentication/liveness boundary and is less desirable than the causal fix. No bridge implemented.

Sources:
https://docs.nvidia.com/deploy/nvml-api/latest/nvml-api-reference.html
https://learn.microsoft.com/windows/win32/shell/knownfolderid
https://learn.microsoft.com/windows/deployment/usmt/usmt-recognized-environment-variables
