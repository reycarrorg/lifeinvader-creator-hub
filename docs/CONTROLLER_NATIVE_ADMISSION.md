# Controller native admission audit — source only

2026-11-19. No HTTP render route enabled; no hardware invocation.

## Verified source boundaries

- `controller_clip::prepare`: independently hashed local config, current Controller
  job/revision/source binding, QUEUED state, existing source guard; runs only
  `prepare.py --dry-run`. It does not create worker authority or claim a job.
- `controller.rs`: preparation/import use spawn_blocking; there is no render route.
  This is not a persistent worker supervisor outside the request lifecycle.
- `stage_queue::run_reviewed`: independent intake/proposal/rubric checks precede
  queue mutation. Existing manager/claimed/hold refuses re-entry. Native caller
  supplies GPU preclaim; later producer failures retain unresolved ownership.
- Fixed lineage gap: Queue.claim chooses oldest pending row. Adapter now refuses
  unrelated existing jobs before mutation and checks exact admitted job ID,
  source hash, cut hash and kind after claim, before producer. Mismatch holds.
- `controller_clip::import_with`: external receipt/artifact pins, source hash,
  canonical cut/recipe, read-only native receipt/proof, current Controller revision
  and source are checked before transitions. QUEUED -> PROCESSING ->
  READY_FOR_REVIEW/PENDING. Existing READY same-proof import is read-only.
- `controller_store::recover`: interrupted PROCESSING becomes HELD, retry_safe=0;
  restart pauses settings and disables autopilot. This is not resume permission.
- Native receipt reader refuses active manager/claims/hold; requires finished
  exact source/cut/kind, matching sidecar/proof and expected format metadata.

## Concrete remaining integration gap

Do not attach run_local directly to a POST or treat rubric scores/config hashes
as operator admission. There is not yet a durable Controller admission record
binding current job/revision/source, independently reviewed cut, selected isolated
workspace and exactly one worker attempt. Controller currently stays QUEUED while
an externally admitted worker runs. Also config currently identifies Python/helper
by absolute path rather than independent runtime/helper byte identity; this is
not sufficient for future executable render dispatch.

Next minimal normal-app slice: atomically record explicit manual admission plus
current revision, paused/settings/source guard and immutable input identities;
hand it to an app-owned bounded supervisor outside the HTTP request. Record worker
attempt/exit and reconcile proof to that admission. Never auto-start on restart;
ambiguous launch/claim/completion must become HELD and require investigation.
There is no rollback to QUEUED after a possible side effect. Preserve read-only
completed-proof import as a separate path and publication review as PENDING.
This document is a design handoff, not implemented render authority.

## Focused evidence

Four new deterministic tests in tests/clip_admission_lineage_test.py passed:
missing/wrong/rebound-invalid review yields no queue; foreign pending row yields
no new enqueue/claim and byte-identical queue; each wrong claimed identity holds
before producer and re-entry refuses; one fake producer gives one exact claim/
receipt/proof and duplicate reuse makes no writes or second producer invocation.
Only disposable fixtures used. Existing held V2 files and apps were not opened.


## Durable storage slice implemented (2026-11-19)

`src/controller_admission.rs` is composed into normal Controller store init and
recovery. Versioned additive admission tables retain exact serialized request
and digest, Controller job/revision/source binding, approved SelectedCut, approval
fingerprint, worker job ID, isolated workspace, runtime/helper fingerprints,
expiry, reservation state and completion receipt/artifact binding.

`reserve` requires explicit `admit_one_clip`, independently supplied Authority
(no installed allow implementation), current source and QUEUED revision, resumed
Controller and <=900s unexpired approval. An immediate SQLite transaction reserves
one durable admission and transitions the Controller job to PROCESSING together.
Worker job/cut/job identity uniqueness and a single unresolved reservation prevent
second admission. Failure rolls back the transaction; no queue or process runs.

`complete` requires independent committed-receipt verification through Authority,
exact reserved worker/selection/current source and actual artifact bytes/hash.
The receipt hash, completion record, preview and READY_FOR_REVIEW/PENDING update
commit together. Same completed evidence returns without writes. Runtime/helper
pins are stored expectations, NOT runtime verification. No production Authority
exists: Unavailable denies all admission/completion. Fake authority exists only
in tests and compares exact test request/evidence.

Recovery marks RESERVED admissions HELD while existing PROCESSING jobs become
HELD/retry_safe=false; restart pauses the Controller. No stale admission can
complete/resume automatically. Existing completed preview remains read-only.

Two targeted offline native unit tests passed: exact fake lifecycle and durable
reopen; unavailable/wrong/stale/expired approval; wrong worker receipt; duplicate
completion no writes; interrupted reservation refusal; archived source and sole
worker admission. Test command: cargo test --bin lifeinvader_hub --locked --offline
controller_admission::tests. Broad cargo test attempted a normal binary replacement
and was blocked by the open preview; dedicated test binary passed without touching
that running executable. No subprocess or GPU is launched by the fake lifecycle.

Remaining TODO: trusted operator-action adapter implementing Authority with exact
review/runtime/file leases; bounded worker supervisor outside HTTP; atomically
consumed launch intent tied to this admission and exact native queue worker ID;
real read_native_receipt bridge to complete. No HTTP real-render route, automatic
worker start, queue ownership reset, publication authority or recovery retry added.


## Supervisor/receipt source slice (2026-11-19)

controller_supervisor::start checks independently supplied Authority, exact
executable/helper/approval file hashes under retained read-only Windows leases,
source bytes/hash and current Controller revision. It atomically consumes a
unique controller_admission_launches row before spawning an app-owned thread,
not a worker inside an HTTP request. No second launch can consume the same row.
The thread rechecks authority, polls its injected child on a 1..180 second budget,
requests bounded child stop on timeout/unknown and durably holds on failure.

On success it opens the fixed workspace receipt/artifact read-only, requires
exact reserved worker job ID, uses the existing read_native_receipt verifier on
a read-only queue connection, verifies actual artifact bytes, and commits via
the durable admission completion path. Its verification Authority is private and
constructed only after these checks. Duplicate completion imports no new preview
or launch. Python run_local/stage_queue now accept the explicitly admitted worker
job ID, preserving it through claim/receipt and rejecting a different duplicate.

Three targeted native tests passed (pins reject before launch; real receipt
verifier against fake queue plus duplicate/reopen; wrong receipt/timeout HOLD).
One targeted Python test passed for explicit job-ID completion/duplicate mismatch.
No native child process, GPU, existing queue or live Controller DB was exercised.
Dedicated cargo --bin unit tests use the separate test executable; no locked
main preview binary replacement was attempted this turn.

CONCRETE BLOCKER: Launcher remains an injected interface with no installed native
process implementation. Safe normal Python worker cancellation/descendant
containment, immutable module/config inventory and path/identity qualification
must be implemented before it is connected to a real process. A trait's poll/stop
contract is not OS containment proof. Existing Python bounded direct-child cleanup
does not prove containment if its parent is killed. No HTTP render entry is enabled.
No default Authority approves actions; file hashes verify bytes, not consent.


## Native process containment qualification (2026-11-19)

Added controller_native_process::Owned as a standard Windows Job Object process
primitive, not installed as a production Launcher. It verifies interpreter,
bootstrap module and config SHA256 under read-only file leases. It uses direct
argv, no shell, Python -I -S -B and an env_clear environment containing only
SystemRoot/TEMP/TMP. The exact trusted bootstrap blocks on public stdin GO before
work/imports; assignment to a non-inheritable unnamed Job Object precedes GO.
Job assignment failure stops only that new owned child and never continues via
another launch route. KILL_ON_JOB_CLOSE is set; breakaway is not enabled.
This is a trusted-bootstrap gate, not CREATE_SUSPENDED equivalence for arbitrary
scripts. Do not expose it as an arbitrary script runner.

Cancellation sends STOP, allows 500ms for cooperative exit, then terminates only
the app-owned job if necessary and waits up to 2s for root reap and active job
process count zero. A root exit with retained descendants is failure, even after
successful cleanup. Drop retains kill-on-close containment; inability to prove
empty returns unresolved/HOLD. Existing durable launch-consumption row remains
the one-shot admission gate. No live job/queue ownership is reset.

Actual Windows qualification used the installed Python Software Foundation-signed
Python 3.12 interpreter SHA4d6f5f81a4bca11191c4c7c6b43632694d0a4ce74e068619d8fdc161d469859a
and tests/fixtures/native_process_probe.py. The synthetic script spawns only one
stdlib sleeping Python descendant. Final targeted test passed four cases: clean
exit0; root failure7 with owned-descendant cleanup; cooperative stop exit0; actual
supervisor deadline -> owned-job termination exit30. All four roots reaped and
QueryInformationJobObject ActiveProcesses=0. Final evidence at ignored
 tools/native-process-qualification/0fb2039b-21e3-4895-8741-10a333593222.
No GPU/model/media/network or existing application was accessed by the fixture.

Remaining production gap: install a normal producer bootstrap that obeys this
same pre-work gate and STOP contract, bind the full worker module/config and
runtime dependency inventory, and connect the reviewed Authority/Launcher without
weakening preclaim/postclaim/preencode GPU gates. Current -S qualification uses
stdlib only; faster-whisper site-package loading is NOT qualified here. Real
render dispatcher remains disabled. No hardware smoke admission follows from
passing process-containment tests.


## Production bootstrap preparation â€” HOLD (2026-11-19)

Added native_bootstrap.py diagnostic-only entry and worker_cancel.py. Bootstrap
uses isolated -I -S startup, exact manifest and per-job config, no .pth/site hooks,
explicit worker/site roots and an import finder refusing unlisted/changed module
origins. Render mode is deliberately rejected. It imports local ASR dependencies
but never instantiates a model or calls queue/GPU/producer in diagnostic mode.
Native gated_manifest verifies and retains read leases for the expected file set
before child launch: 8,076 files / 914,567,253 bytes across existing interpreter
Lib/DLLs, installed speech packages, worker tree, model and tools. Manifest pin:
a34606511e5b5e88116624d6463566ec6d152526cb22e13c5444885a0a270b90.
This binds listed bytes; arbitrary dynamic DLL discovery is NOT thereby proven.

Real worker cancellation checkpoints now cover pre-admission, post-preflight,
postclaim, preencode, prepublish and pre-receipt. Active owned subprocess waits
poll cancellation at <=100ms intervals and retain bounded direct-child cleanup;
Job Object remains outer containment. EOF/STOP sets sticky cancellation, never
resetting queue ownership or authorizing retry.

Three pure tests passed: unlisted/changed import rejection, preclaim cancellation
with no queue plus post-output cancellation retaining claimed/no proof, and active
owned-child cancellation cleanup. Actual pinned-config mismatch rejects prelaunch.
Native diagnostic initially rejected a mixed-separator manifest lookup before
launch; comparing Windows Path components fixed that without changing hashes.
Then actual diagnostic reached its 60s result deadline (77.75s total including
prelaunch manifest work), produced no result, and test failed. Exact bootstrap
process count was zero at 14:05:17 CT. Native PID/exit/phase were not retained;
no model/import success or cancellation-case completion is claimed. No retry.
Evidence: tools/bootstrap-qualification-01/qualification-hold.json.

Remaining blockers: localize slow/blocked phase with bounded fixed diagnostics;
qualify diagnostic imports and STOP; connect production ASR child to the same
manifest/import/cancellation policy (current child argv does not inherit parent
sys.path policy); close dynamic-library/import-path assumptions; only then enable
an explicit reviewed production bootstrap. Normal startup/real dispatcher remains
disabled. Independent QA can review source and rejection evidence now, not approve
hardware readiness. Held V2/open apps and original evidence are untouched.


## Instrumented bootstrap diagnosis and ASR identity propagation (2026-11-19)

One fixed-phase diagnostic localized the stall after FASTER_WHISPER_IMPORT_BEGIN:
native manifest leases completed in 9.825s; Python path/hash validation by 2.594s;
worker imported by 2.609s. Child6192 reached 60.004s deadline; STOP caused import
to finish at60.156s and cancellation exit31, cooperative cleanup=true and active
job processes=0. No verification was removed: hashing is not the slow stage.

A narrow validation separated the cancellation descriptor from dependency stdin
(duplicated control fd, null sys.stdin). It did NOT fix the stall: child13220,
60.019s deadline, exit31 after STOP, cooperative and active_after=0. No further
native run. Small stdlib/isatty and Click read(0) probes did not reproduce the lock;
the exact native/dependency call remains unproven. Preserve both logs. Next useful
diagnostic is bounded package-subphase/native-import stack identification, not a
longer timeout or dropping byte guarantees. No ASR/model/GPU/media dispatch ran.

Source-only ASR identity forwarding now carries exact manifest/config path+digest
from verified bootstrap to child payload. Child entry uses -I -S -B, verifies
interpreter/self/worker/module bytes and config-to-manifest binding before imports,
installs the same origin/bytecode checks and imports only explicit pinned roots.
Direct run_local/legacy ASR entry without verified bootstrap context refuses before
queue/media. Two new pure tests passed for missing identity and config tamper.
Actual ASR child was NOT executed; this newer source is not covered by prior
native manifest pins. Diagnostic manifest snapshots remain immutable historical
evidence, not silently regenerated authority for changed worker files.

Unqualified boundaries: dynamic dependency DLL selection (CTranslate2/PyAV add
DLL directories; Python import finder does not mediate every DLL load), directory
addition/alias identity, the dependency import stall, and native ASR child end-to-
end policy. Listed file leases protect listed bytes, but do not justify eliminating
validation or claiming all consumed code is proven. Real dispatch remains disabled.
Independent QA may review source and the precise failure evidence, not hardware
readiness. No queue/held V2/open app/owner checkout/live service was changed.

## 2026-11-19 import-stall diagnosis and narrow fix

Evidence is in ignored tools/bootstrap-stack-04, bootstrap-pipe-poll-05 and bootstrap-venv-fixed-06. The historical qualification-01 HOLD record has no child stack/PID; it cannot establish a 1:59-pm inner call. It has been preserved rather than retroactively rewritten.

The 3-second faulthandler capture in 04 shows the main thread in NumPy `_core.multiarray` importing `_multiarray_umath` (native create_module), and the cancellation thread in worker_cancel.listen/readline. PyAV had returned; CTranslate2 had not yet been reached. STOP at 8.0037s releases the import and produces exit31, PID25236, zero active Job processes, no forced termination. This proves the blocked boundary, not the exact internal native/CRT lock.

The only behavioral fix was Windows control monitoring via sole-reader PeekNamedPipe polling (20ms). After GO, any byte, pipe EOF/error or listener exception causes sticky cancellation; it grants no work and consumes no controller data. No CRT blocking read is held while native libraries initialize. Synchronous PeekNamedPipe is not universally wait-free in multithreaded applications: keep sole-reader ownership and the existing outer Job timeout/cleanup. Reference: https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-peeknamedpipe . Non-Windows behavior remains fail-closed readline. No manifest checks were removed.

05 imports completed in 344ms, then exposed diagnostic PackageNotFoundError: our replacement PathFinder lacks distribution metadata lookup. The result now records versions from the exact pinned imported modules instead of re-enabling unrestricted metadata discovery. 06 explicitly pins project venv Scripts/python.exe (0b471133e110cfb53a061cad528ce8e517d7b9ac41a0a396c39ad795a487fc14), pyvenv.cfg and existing base runtime; launches with -I -S -B -u, no PATH fallback. Parent venv PID30732, actual Python PID23724, exit0, active_after0, forcedfalse at 2026-11-19T20:21:46.844100Z (14:21:46 CT). Child completed in 3.0779s; imports .328s. No model instantiated, media loaded, GPU queried or queue opened. Two new control-only tests passed in .243s (quiet/STOP/disconnect/malformed/invalid handle). Full file leases and byte checks remain unchanged. ASR invocation now includes -u; local_files_only=True already exists with the configured model path.

### Remaining qualification before fresh hardware

- Real dispatcher stays disabled. This is import/cancellation qualification, not ASR or producer success. Preserve held V2 and all old preview/evidence.
- Module-origin and bytecode checks plus full leases do not mediate the native Windows loader. CTranslate2 explicitly ctypes-loads local DLLs; PyAV adds av.libs, NumPy loads its extension/BLAS dependencies. Qualify a closed PE dependency inventory and exact loader directories (including OS/runtime identities), hold actual consumed non-system DLL leases, and compare loaded module paths/identities from the actual child against that independent inventory. Post-load hashes alone are detection, not prevention of unexpected DllMain. No PATH or arbitrary environment fallback. Do not claim all DLLs qualified today.
- ASR child already receives manifest/config raw hashes and rejects missing identity before PCM; it must next prove the same venv/base identity, pinned bootstrap, guarded imports and retained parent leases inside the same owned Job. Qualify its no-media import/cancel route and loaded DLL set before model-only local CPU initialization, then independently bound model inference. No ASR child was started in this ticket.
- Fresh smoke needs separate reviewed source/cut, empty distinct Controller DB/queue/workspace, explicit authority/runtime binding, GPU ambiguity guard and exact receipt-to-READY_FOR_REVIEW proof. Neither historical synthetic approval nor these diagnostic manifests authorize rendering. Send launch preflight to Naga before any new unsigned native artifact needing approval.

## 2026-11-19 actual ASR-child CPU/model and DLL diagnostic

Implemented diagnostic-asr mode through the existing native bootstrap and actual asr_bootstrap entry, with the same raw manifest/config identity, guarded imports, pinned venv and inherited owned Job. Mode accepts no media, source, queue or producer instruction. It constructs only the exact local tiny.en CPU/int8 model with local_files_only=True. Added read-only self-module inventory (K32EnumProcessModules/K32GetModuleFileNameExW), path/SHA/file-index evidence before and after model initialization. The diagnostic reports pending system trust rather than self-approving it. Corrected real ASR version reporting to pinned module versions, matching the prior bootstrap metadata fix.

07 stopped before model: 100 manifest-leased images, 39 System32 candidates and one previously unlisted Windows WinSxS COMCTL32 image. PID31672 exit31, cleanuptrue, active0 at20:24:51.534Z. Independent Get-AuthenticodeSignature checks verified all40 as Valid Microsoft Corporation, with exact current hashes matching recorded loaded-image hashes. These40 exact paths/hashes were added to 08's retained manifest, not an arbitrary system-path exemption.

08 passed: venv launcherPID29504, actual ASR interpreterPID24312, exit0 and active_after0/no forced termination at2026-11-19T20:25:52.8084339Z (14:25:52CT). Overall child8.1006s; ASR inventory/model phase.688s. Requested CPU/int8, effective CPU/int8_float32. No transcription/media/GPU query/queue. Import snapshot140 images all manifest-leased. Model snapshot142:141 manifest-leased (new NumPy pocketfft was already in retained inventory), plus System32 kernel.appcore.dll SHA5da68e8d50b02fb8c90cf3bcd3c511723fc433612d0a441071b6e31f9544cf8d. That final Windows image independently verified Valid Microsoft Windows with matching hash. No uncontrolled non-system images observed.

Evidence tools/asr-dll-bound-08: manifest.json SHA9aa6170b26bedaac867db1f8ea7282a44f17cc589f756f2d387ef4e67a5f98fa; asr-import-dlls.json; asr-model-dlls.json; asr-diagnostic.json; model-system-signature.json; exit.json. Prior40 signature evidence retained in tools/asr-dll-qualification-07/system-signatures.json. Candidate manifest adds the final exact Windows image and is SHA cbe85fd89a6e46b95951e10c28e44f95729c64c9bf08c40f99843b9ab6707916. It is a future input, not activated permission or a claim that the final system image was leased in08. All observed non-system images were leased during08.

Qualification boundary: actual no-media CPU initialization and observed loaded-image identity passed. Snapshot APIs do not capture transient load/unload or prevent unexpected DLL initialization; they are not an independent loader admission mechanism. The real transcription path does not yet enforce this diagnostic inventory, and inference-specific lazy loads are unobserved. Keep normal dispatcher disabled. Next smallest source slice: bind explicit runtime snapshot policy/known OS images into the actual ASR invocation before/after CPU model initialization, enforce unknown/mismatch HOLD, retain same parent leases and Job lifetime; account for loader-directory resolution rather than relying on post-load hashes as permission. Do not repeat completed import tests without a changed boundary.

### Fresh E2E preflight / approval matrix (not launched)

- Runtime CPU initialization: observed pass above. Production ASR loader enforcement/lazy-load boundary: still HOLD; no hardware-ready claim.
- Fresh synthetic source/cut: not prepared or approved in this ticket while production boundary remains unqualified. Never reuse held V2 workspace/claim or original saved clip. New source needs its own SHA, envelope/cut/proposal and independent rubric review.
- State: fresh separate Controller DB, queue, work directory, exact source revision and one-use manual admission; no normal startup producer registration.
- Launch provenance: Rust unit diagnostic compiled locally from adapted working tree; Python uses pinned existing venv 0b471133...87fc14 and retained base runtime. No new main-app artifact was built/launched. Future main-app build needs exact source changes/hash and Naga's applicable launch preflight before execution.
- Existing authority: one future fresh synthetic E2E only, subject to runtime, independent cut review, GPU unambiguous-idle and receipt gates. No authority to retry V2, relax guards, rerender saved clip or publish.
- Required final evidence: one durable queue claim, real ASR/model/device record, one NVENC encode, 1080x1920 H264/AAC within bitrate limit, output hash/receipt validation, Controller READY_FOR_REVIEW and zero owned descendants. None is asserted by this diagnostic.

Windows API limitations: https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-enumprocessmodules and https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getmodulefilenameexw .

## 2026-11-19 production loader enforcement source and remaining audio gate

Both bootstrap and actual ASR child now install process-local SetDefaultDllDirectories(SYSTEM32|USER_DIRS) before native imports. Six exact inventoried DLL directories are added; no PATH/CWD/application fallback. Reject native directory reparse, unexpected DLL/PYD sibling and ambiguous different-hash DLL basename. .pyd imports remain exact-path guarded instead of globally registering PyAV's legitimate repeated package filenames. Audit hooks reject unexpected os.add_dll_directory and ctypes.dlopen paths/bare names; this is defense in depth, not arbitrary-native-code sandboxing. Current Windows system trust remains exact pinned files, not wildcard acceptance at checkpoints.

Production ASR requires runtime file policy, enforces exact loaded-image inventory before PCM, after model initialization, for every yielded inference segment, and before writing evidence/captions. Unknown system or third-party image/mismatch means HOLD. No CPU inference has yet run under these new controls, and unknown code can initialize before a post-load checkpoint; supported safe search plus pinned known package code and retained leases reduce search substitution, not prove hostile native code impossible. Native dependencies adding directories by direct native API remain outside Python audit coverage. Real dispatcher stays disabled.

Validation: three new DLL policy tests passed (unlisted/pending image, conflicting DLL names/unlisted sibling, actual lazy-generator check placement). Read-only search plan found six directories. One changed-boundary import-only native qualification in tools/asr-loader-controls-10 passed: PID2088 exit0, active0, forcedfalse at2026-11-19T20:31:16.1592312Z (14:31:16CT);3.2205s child. Model initialization was not repeated. Previous CPU model evidence remains in08.

Generated audio preparation in new tools/asr-inference-review-09 failed at installed SAPI voice enumeration, HRESULT0x80045039, before stream/file creation or speech. No audio exists; no independent approval/inference fabricated. preparation-hold.json records the proposed original text and blocker; hardware-preflight-plan.json holds a distinct fresh workspace/Controller DB plan with null source/cut/hash fields pending actual creation and independent review. No workspace/DB/queue or hardware render was created from that plan. Need supported existing voice enumeration/generation or an independently reviewed locally generated fixture before one bounded CPU lazy-inference qualification. Do not reuse held V2 or original saved media.
## Isolated UI lifetime correction (compile/test only)
The new isolated-review lifecycle does not interpret Edge bootstrap exit as window closure. It logs bootstrap PID/outcome, requires same-origin Controller UI readiness within60s, and stops on explicit Close review app or90s without heartbeat. Both paths invoke existing manual.shutdown/cancellation/join before server exit. Normal nonisolated main app and saved-preview paths are not redesigned. No Edge flags, sandbox, browser profile path or security setting changed.

A heartbeat is liveness, NOT authentication/render authority or proof of visible playback. Existing independently reviewed expiry/source/runtime/admission checks still apply; rendering is additionally refused before UI readiness. The strict close/ready endpoint changes only memory, requires same origin, and is unavailable on normal startup. Expired or closed lifetimes cannot be revived. Browser X/reload/navigation has no immediate close beacon; loss of this review page stops the isolated server after90s. Background browser timer throttling can cause safe HOLD/cancellation; this is an explicit limitation pending real Windows qualification. Use Close review app for deterministic ordinary shutdown, then close the dedicated browser window; do not claim the server can force-close an unowned browser process.

Three focused Rust tests passed: mocked exited bootstrap retained UI; readiness/disconnect/close deadlines; route same-origin/schema rejection and zero DB mutations. Dedicated offline cargo build passed; app executable NOT run. Candidate tools/ui-lifetime-build-target/debug/lifeinvader_hub.exe SHA2560ccf3820da96123a6bd4379e0186fbc480966977f2cc251f788edeadafcd7b0b (11844608bytes), new unsigned build requiring applicable exact launch approval.

Proposed ONE next launch: after exact new-build approval, retain shell/session and stdout/stderr; same isolated DB/media/port with exact old expired plan for STARTUP ONLY. No GPU or render. Observe UI readiness, bootstrap outcome, then Close review app and exact app exit/owned-child status. If no UI by60s, expect UI_NOT_READY_60S and fail-closed shutdown; preserve evidence and no retry. Stop for actual security prompt. There is no automatic browser fallback.

Supported separate qualification option, ONLY if independently selected before launch: --server-only plus the same explicit isolated plan starts bounded server without Edge; a verified local UI navigates exact loopback /controller and provides same-origin readiness/close. This is a diagnostic/render qualification route, not final native installer UX. Do not switch to it after a failed/denied launch without new scope. Expired plan remains non-rendering; hardware requires separately reviewed fresh request/config/manifest digest chain and GPU guards.

Runtime packaging source: packaging/runtime_layout.py creates a pure plan from pinned inputs, rejecting unknown files, duplicate destinations and path traversal. Two focused tests passed. tools/runtime-relocation-plan-01.json maps8078 entries and explicitly excludes54 system/job/venv entries, SHA5df91adb19896702363dff17cbc94f6afa2a1d2913803f0ecabe8d8ac09898e3. Base Python replaces nonrelocatable venv launcher; site-packages/model/FFmpeg/worker get separate relative destinations. No runtime files copied/executed. License closure, destination pins and relocated loader/ASR qualification remain mandatory; plan is not a runtime or installer success claim. NSIS DNS/trusted MSIX signing constraints unchanged; no social auth gate.
