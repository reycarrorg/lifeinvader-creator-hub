# Known issues and planned work

## Open — Solo Post metadata and picker coverage

The picker and library hide an image only when its filename stem and post ID match a Solo video; other image files remain selectable, including possible unmatched thumbnails. Original account, platform and dates depend on the saved metadata; missing fields can remain unknown. Solo downloads use a separate archive and can duplicate downloads from other source archives. Multi-asset carousels are not yet represented as one post. The quick-editor visibility setting is stored in browser-local storage, so it may differ between browsers.

## Open — Instagram download tool validation

The app now passes its discovered FFmpeg location to yt-dlp. Offline detection and synthetic processing worked with FFmpeg and FFprobe 7.1; the owner’s next real Instagram run has not yet been verified. Archives still skip already-recorded videos, so restarting a batch does not force a higher-quality re-download. Other platform restrictions and photo support remain separate issues.

## Open — Quick editor coverage

The first editor supports centered aspect presets, one contiguous trim, mute and one uploaded PNG/JPEG overlay at 50% opacity. It does not provide arbitrary crop positioning, multi-cut timelines, built-in branding, overlay placement, cancellation or edit-project history. Encoding depends on FFmpeg and codec support; large inputs can take time and use storage. An edited copy is separate from the original, and publishing is not automated. Validate additional formats and long or rotated media before treating this as a full editor.

24-hour source counts and one-hour new-media highlights include only files tracked by Hub sync after tracking began; they are not a complete historical import count. The one-hour period begins at recorded download time, even if the user opens the source later. Original post dates may be unknown or estimated.

## Open — Profile recovery and content-library coverage

Related work: [PR #8](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/8).

TikTok account-ID recovery requires a valid post belonging to the profile and can still fail when platform access is restricted. Instagram captured-link fallback only covers posts previously visible in Brave and must be refreshed to include new posts; successful fallback is labeled partial. Photo and full account coverage have not been verified in a real sync. Library length scans read local media metadata and store lengths in the app browser's local storage; unsupported or unavailable files can remain unknown.

## Open — Computer-use posting and concurrency

Related work: [PR #7](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/7).

The queue exposes a local JSON handoff, durable activity notes and persistent per-item claims. An interrupted claim does not expire; a person must verify the platform and release it for review. Claims coordinate clients that use the contract but cannot prevent manual or external uploads. Scheduled times do not start publication, and `posted` remains an operator report rather than independent platform verification. The scheduled assistant, browser access, provider reconciliation, confirmation flow and real uploads have not been tested end to end. Older items may lack activity history from before logging was added.

Automatic selection runs only while the Hub is open. Scanned durations are matched to the source root, path, file size and modified time; moved or changed files need to be scanned again. Videos without a known duration are excluded when a length filter is active. Unknown original dates and missing metadata still limit selection.

This file tracks durable context in Git. Add reproduction steps, evidence, status, and links to GitHub issues or pull requests when available. Do not add private credentials, cookies, account exports, or personal media.

## Open — Social platform access

Profile and source sync depend on external download tools and platform access. TikTok and Instagram can restrict public profile pages or require login; the application does not currently implement social-account sign-in. Isolated fake-downloader tests do not prove a real account can sync. Preserve logs locally and avoid publishing sensitive output when investigating.

## Planned — Reference existing files in the content chooser

Profile sync records post and asset identifiers and their local file locations, but the existing manual library import still copies selected files. Design a future chooser around stable platform/post/asset identities and existing files, handle missing or moved files safely, and distinguish reuploads with different platform IDs from true duplicates. Do not claim cross-account content deduplication is already implemented.

## Planned — Secure shared use

The app currently binds to localhost and has no authenticated shared-user access. A GitHub fork shares code only, not profiles, claims, source archives, or media. Design and test authentication, authorization, backups, and private networking before offering a shared hosted deployment.

## Open — Platform and environment validation

Windows builds and synthetic tests have been exercised. A bounded live YouTube test-channel download and an Instagram reel/fallback test succeeded in disposable folders; these did not validate complete channel downloads or every account. The Apple Silicon macOS build, isolated tests and launcher startup have been verified locally. Windows data transfer and source-path migration, full GUI use, real social downloads and publication on macOS remain unverified.

## Open — Stalled cloud folder enumeration

On macOS a source-summary scan was observed blocked in a directory open while other local endpoints timed out. A candidate isolates background and interactive scans into separate bounded slots; it has been tested with simulated stalls only and needs verification after a controlled restart. A stalled interactive scan still prevents opening another source until that filesystem call finishes or the app is restarted. Other filesystem operations are not covered by this bound. The Terminal-launched app and a read-only Terminal probe also stalled opening the Solo archive, while an independent Codex-shell check succeeded; the underlying launch-context/File Provider cause is not established.

## Planned — Real account connections and analytics history

The dashboard supports manual snapshots, CSV, and opt-in Brave refresh of visible profile counts. The Brave extension must be installed and paired by the user; the real extension flow has not yet been verified end to end. It does not fetch private platform Insights, automatically fill profile photos, or publish posts. Official API access depends on each platform's permissions, account eligibility and app approval; see `ANALYTICS_RESEARCH.md`. Expand snapshot semantics and secure OAuth before advertising live analytics. Add corrections/deletion and platform-specific export mappings.

## Planned — Automated publishing and complete source picker

The source picker and local queue reference existing photo or video files; auto publishing, category-level posting schedules, AI caption drafting, whole-carousel queueing, copying video files to the clipboard, and verification of published posts against real social accounts remain unimplemented. Date filters use available original metadata or clearly labeled TikTok ID estimates; old posts may still lack dates. A bounded live YouTube test-channel download succeeded, but full-channel and account coverage remain unverified. See `POSTING_RESEARCH.md`.

## Open — Instagram account download coverage

The unauthenticated Instagram profile extractor failed during a live test. An individual owner reel succeeded, and a bounded app test verified the fallback using a supplied captured link. The Brave link capture only covers visible posts and does not establish a complete account download. Photo/carousel coverage and the installed extension flow still need validation. See `BROWSER_ANALYTICS_AND_PUBLISHING.md`.

## Open — Older post dates and online availability

The date audit can use existing sidecars, verify missing dates online where supported, or derive a clearly labeled UTC estimate from an unambiguous TikTok post ID. It does not invent a date for unknown/ambiguous IDs. Platforms may block metadata-only lookups; source ID timestamps are estimates, not guaranteed publishing dates. Audit currently checks date-related metadata, not complete missing thumbnails, captions or all other post fields.

## Open — Initial source selection and solo-link coverage

Related work: [PR #6](https://github.com/reycarrorg/lifeinvader-creator-hub/pull/6).

Metadata-only previews depend on the provider and can fail, omit thumbnails or dates, or return a nonchronological/incomplete feed. Selection currently stops at 1,500 posts and includes the center post. Filtering preserves skipped IDs outside the downloaded archive; later unseen posts can be included for an after selection. A before selection stays limited to selected IDs. Photo sync is blocked where it could bypass the selection. Solo links use the original script's separate folder/archive, so duplicates across other source archives are not detected. Real-platform end-to-end validation, login support, retry/cancel controls, and large-account pagination remain open.


## Adapted Stage 2 Controller integration
Controller/review state, read-only worker evidence, and owner-authorized registry are available as isolated app functionality. Live worker/owner/OPS/publishing adapters default unavailable. Stage 1 runtime/transport/routes excluded. See STAGE2_INTEGRATION.md for dependency extraction, validation and limitations.

## Adapted saved-preview qualification
Strict recipe normalization is implemented with four native synthetic regressions. Production authenticated selection adapter remains unavailable. Saved-video viewing verifies file integrity only, not worker authority; real playback/captions/reopen remain unverified. See SAVED_PREVIEW.md.

## Staged native worker bridge
Synthetic standalone queue-to-Controller completion and persisted preview reopen passed. Real ASR/NVENC producer and normal render dispatch remain unimplemented; no hardware smoke or production-worker pass is claimed.

## Local producer dependencies
Configured Python 3.12 lacks faster-whisper, ctranslate2 and numpy. Local model manifest, native read leases and actual NVENC path remain unqualified. No downloads or real render occurred. See LOCAL_PRODUCER.md.

- 2026-11-19: Speech dependencies resolved. Hardware smoke still held for new source/review bindings; requested CPU int8 effective int8_float32 is now explicitly allowed and recorded. No hardware execution this turn.

- 2026-11-19: New ignored synthetic source created via installed SAPI and CPU fixture FFmpeg only; source 1e9ef1f7...02b16b0, proposed cut 2770-42430ms. Independent rubric/visual review remains HOLD (static abstract visuals do not demonstrate spoken movement). No intake/review approval, queue or target render.

- 2026-11-19: Separate ignored V2 synthetic source adds observed movement/shot/exit payoff; SHA d47f1620...0652bef. Cut bounds pure check passed 2770-42430ms. Review remains independent; seven labels have no descriptive anchors locally and verifier minimum is 5/7, not authority to lower user threshold. No target worker/render.

- 2026-11-19: Current claimed v2 evidence remains untouched. Older configs lack required gpu_uuid and fail closed. Query output from historical failure was not captured; new diagnostics cannot reconstruct it.

- 2026-11-19: No HTTP render dispatch is linked. Existing prepare/import does not establish durable Controller-to-worker start lineage; see CONTROLLER_NATIVE_ADMISSION.md. Held V2 untouched.

- 2026-11-19: Durable admission storage now exists, but executable pin verification and worker launch/reconciliation remain uninstalled. Broad cargo test hits open-preview executable lock; use --bin targeted unit tests without replacing it.

- 2026-11-19: Supervisor source is wired to receipt verification but native Launcher remains unavailable; no live GPU attempt. Preserve held V2 forever; do not reuse its workspace or admission.

- 2026-11-19: Process containment qualified only for pinned stdlib fixture. -S production site-package loading and actual producer STOP integration remain unqualified; no GPU or V2 action.

- 2026-11-19: Production bootstrap unqualified: diagnostic result timeout, exact process absent afterward, no phase/PID/exit evidence. Real dispatcher disabled; V2 untouched.

- 2026-11-19: Control-stdin isolation did not resolve import stall. Precise call unknown; final ASR identity source has not been native-qualified and old manifests intentionally do not authorize changed source.

- 2026-11-19: Import-stall fix validated: avoid blocking CRT control read during NumPy native init via sole-reader Windows pipe polling. Diagnostic metadata lookup corrected to pinned module versions. Native DLL loader/ASR-child identity remains unqualified; preserve hardware HOLD.

- 2026-11-19: ASR model diagnostic passed after exact Microsoft WinSxS dependency pin; final kernel.appcore signature verified separately. Snapshot-based inventory is evidence, not prevention of unknown DLL initialization. Production enforcement remains a hardware gate.

- 2026-11-19: New audio fixture not generated: installed SAPI GetVoices enumeration failed HRESULT0x80045039 before output. Safe DLL imports pass; actual inference lazy dependencies remain unqualified.

- 2026-11-19: Tone CPU attempt PID9772 exit31 at20:33:58Z after model evidence; child stderr suppressed, precise failing predicate unknown. Preserve tools/asr-tone-inference-11; no retries. New capture source not executed.

- 2026-11-19: Proven tone diagnostic incompatibility: faster-whisper1.2.1 file decoder passes unsupported metadata_errors to PyAV19. New diagnostic PCM-array source fix avoids unused file decoder; production already consumes floatPCM. Post-fix inference not yet run.

- 2026-11-19: Tone PyAV incompatibility resolved in diagnostic by matching production floatPCM path; actual encoder/decoder now verified. SAPI bypass not attempted: existing FFmpeg flite supplied new original source. Native E2E/render remains unrun.

- 2026-11-19: Current new app candidate compiles receipt consumer/launcher/adapter but cannot initiate isolated hardware test: start_isolated_manual has no concrete caller. Do not present hash3959458c... as launch-ready or request approval for ineffective build.

- 2026-11-19: Missing manual caller resolved in source/build. New candidate not executed; actual native E2E and shutdown/restart unproven. Staged payload is not an installer; no supported installer compiler found in checked locations.

- 2026-11-19: Launch packet prepared but no actual unsigned app authorization yet. Expiry15:12:20CT may require explicit reviewed refresh; no silent renewal. Read-only GPU idle is time-specific. Installer remains unverified/unbuilt.

- 2026-11-19: Native UI startup HOLD before admission. Existing logs cannot distinguish tool-session descendant termination from direct Edge child exit causing graceful app shutdown. No exit code captured; loopback text alone is not readiness. Preserve candidate and evidence; no blind relaunch.

- 2026-11-19: SDK packaging tools exist but MSIX trusted signing not established; no trust-store workaround permitted. NSIS official download DNS failed. Current payload lacks relocated qualified speech/tool runtime, so installer readiness remains blocked independently of social auth.

- 2026-11-19: Startup-only app21564 exits0 within6.76s while retained shell waits. Normal app Edge direct-child wait triggers graceful shutdown; child status is not instrumented. Loopback text not UI readiness. Exact Edge behavior remains unknown; do not claim original tool-teardown cause or successful startup.

- Startup-only-02 stderr adds concrete Edge diagnostic: CreateAppContainerProfile HRESULT0x80070057 at15:16:03.880CT; Edge PID31004 also logs model catalog/language updater error5 and renderer task fallback. These errors precede app exit0 but causality is not established. No AppContainer/security-policy workaround or alternate launch performed.

- 2026-11-19: Edge lifetime coupling corrected for explicitly isolated review in source; Windows UI still unqualified. Missing UI fails after60s, heartbeat loss90s cancels safely. Browser throttling/navigation may produce HOLD; no browser security changes/fallback. Packaging relocated runtime still needs license and loader closure.

- 2026-11-19: Captured actual Edge bootstrap crash0xC0000005. AppContainer profile creation0x80070057 also logged; causality not proven. New bounded UI lifecycle correctly held60s and exited without worker. Security/profile modifications or automatic server-only fallback not attempted.

- 2026-11-19: Existing ordinary Brave cannot reach current isolated server origin (connection timed out); cause not proven, no firewall/proxy/AppContainer changes or alternate transport. Server safely expired. Test tab closure unconfirmed due CUA coordinate input geometry unavailable.

- 2026-11-19: Concrete executor loopback socket restriction10013 reproduced in bounded isolated test beforeHTTP. Routes pass direct service test. Underlying control unidentified; no safe causal app fix proven, no bypass/alternate transport. TCP qualification remains FAIL.

- 2026-11-19: Executor socket blocker qualified through official per-command approval: unchanged exact TCP test passes outside restricted context. Do not change global settings or infer this exception covers production app. Actual browser/render/cleanup still unqualified.

- 2026-11-19: Fresh06 real-context GPU preflight returned WDDM_UNKNOWN, failclosed beforelaunch. Earlier sandbox-visible idle must not be treated as equivalent visibility; no process whitelist or unknown-memory relaxation. No actualrender attempted.

- 2026-11-19: Existingguard requires zeroGPUprocessrows; WDDM N/A alone cannot distinguish activeencoder vs othercontexts. This may be impractical with a liveGPUdesktop, but no relaxation is authorized. Encoder sessionAPIqualification can explain contention, not clear currentguard automatically.

- 2026-11-19: Zero encoder sessions/utilization cannot establish WDDM exclusivity or global available VRAM. Static payload validation passed but runtime relocation and legal redistribution remain unqualified; four distribution notice sets missing from collected inventory.

- 2026-11-19: Nine offline WDDM qualification cases PASS; no GPU query/policy change. Relocated Python -I -S -B stdlib startup PASS with payload-confined prefix/import paths. ONNX Runtime LICENSE/ThirdPartyNotices collected create-only in runtime-license-inventory-02; three notice sets still missing. Source-only WebView2 window seam added, NOT compiled: official SDK bytes/license/checksum absent; no restore/install. Third-party relocated imports/DLLs remain unqualified.

- 2026-11-19: Authorized encoder policy implemented (UUID, fresh <=10%, zero NVML count/session count, agreement, no WDDM residency gate). Five policy and four queue/race tests pass; headless configuration Rust test passes and new candidate built (955b9a01...487387), not executed. Explicit --headless-review requires pinned plan/server-only, suppresses scheduler and uses Controller-only routes. Fresh07 staged pending exact new EXE approval; no GPU query/render. See docs/HEADLESS_REVIEW_07.md. Prior GPU diagnostic proposal is superseded only by explicit operator policy, not automatic admission.

- 2026-11-19: Fresh08 exact readonly predicate trace proves source_unavailable at shared account-profile host validation: platform youtube with lifeinvader.local URL. Prior DB/config/workspace/worker pins pass. Correct next synthetic fixture metadata/source binding only; original guards unchanged. Focused metadata correction test and durable prelaunch diagnostic test PASS, two sanitized HTTP error tests PASS. New diagnostic app compiled 938dc803...7101f4; not launched. Fresh08 held evidence untouched; no ticket renewal or worker/GPU retry.

- 2026-11-19: Truthful local synthetic scheme implemented in Controller resolver; remote account/download path still rejects it, source custody and synthetic provider/ID guards retained. Old YouTube-shaped fixture label superseded. Exact Fresh09 readonly source/config/pin trace PASS with zero DB mutations/dispatch. New unsigned ca0b21f...ab16e7 compiled; packet431fe2...7861c pending new exact approval, expiry22:34:37Z. No GPU/app run. Static payload02 package-path resolution passed, not full relocation execution proof.

- 2026-11-19: One scoped vendor environment A/B proved minimal worker environment reproduces NVML_UNKNOWN_ERROR exit255/hashdc7f...; inherited control succeeds exit0 with same x64 pinned tool/cwd/approval context. Specific required variable not isolated; no production environment/guard change or Fresh09 retry. Three new final-folder resolver synthetic tests pass (reject original same-byte fallback, relative paths, missing/changed files). Full relocated runtime still unqualified.

## 2026-11-19 vendor environment matrix evidence limit
One scoped diagnostic helper exited1 before writing any condition result, with stderr discarded. Evidence tools/vendor-environment-matrix-01/exit.json. This does not establish whether a vendor subprocess ran or why helper failed. Owned Job was closed; helper PID19572 subsequently absent. No repeat or production fix selected. Need bounded sanitized helper phase capture and actual Rust bootstrap vendor-only seam before declaring containment qualification.

## Matrix root cause refinement
Inert reproduction demonstrates pre-Popen KeyError: dict(os.environ) uppercases SYSTEMROOT on Windows; historical helper accesses SystemRoot. Historical stderr unavailable, so exact earlier launcher events remain unobserved. No GPU matrix repeated. New real-bootstrap vendor-only seam reads os.environ directly (case-insensitive), has bounded separate stderr classification, and awaits contained qualification. Existing production minimal environment remains unchanged.

## Actual containment result: WINDIR/PATH insufficient
22:37:45Z four explicit candidates (minimal, WINDIR, System32 PATH, both) under actual Rust gated_manifest all return identical Fresh09 exit255/output hash. Helper and entire owned Job exited cleanly, no forced cleanup. Diagnostic mechanism is qualified; vendor init dependency still unresolved. No production patch or rerun.

## Packaging native-origin HOLD
Relocated import-only03 completes all six target imports, then post-import verification ValueError. Source-only improved exact fixed code and module-category/digest capture, not rerun. Need identify rejected native origin and independently pin/qualify it; do not widen DLL trust or claim portable runtime success. Existing modules are not inferred safe from import success alone.

## GPU category / containment and package limits
Combined OS dirs succeeds; omission of ProgramFiles+ProgramW6432 reproduces NVML failure. Other individual category omissions succeed. Minimal single-key set unproven. Final contained helper exit0/active0 but forcedtrue; preserve this failure evidence, do not portray clean completion. No production policy change.
Relocated native-origin HOLD resolved to previously pinned/signed Windows WinSxS COMCTL32, not arbitrary project fallback. Strict exact path+hash policy prepared, remaining inventory beyond first rejected image unobserved. Final package switches to requested official embeddable Python3.12.10, not copied-base release claims.

## Environment causal fix and lifecycle resolution
ProgramFiles alone sufficient in actual contained vendor test; ProgramW6432 alone also sufficient. No need for full env, WINDIR,PATH,ProgramData,SystemDrive. Production allowlist uses OS-known ProgramFiles. Inert delayed descendant test validates500ms natural drain before forced cleanup; failure/timeouts stillcontained. Fresh10 render remains unattempted and pending exact new-build approval.
Official embedded runtime provisioned, but normal pip cache absent and24 exact pinned wheels not staged. Existing copied payload not final. Metricool inert schema and HF OAuth library files prevent an absence claim; review config remains unconfigured/no social workflow.

## Fresh10 startup seed-row bug
Fresh10 schema-only provision omitted controller_meta(singleton1,version1) and controller_admission_meta(version1). Exact app startup fails before readiness/storage_unavailable; read-only evidence proves missing first row, source proves predicate. Generator corrected source-only; never invoke against Fresh10. Before next approved plan, create fresh fixture and validate startup seedrows + actual init contract, rather than only admission predicates. No worker/render attempt occurred.

## Fresh11 manifest input-closure defect
Runtime manifest retained six Fresh09-local paths although config/preparation were rebound. New preparation and producer paths absent, so immutable leased-input guard stops beforeworker. Both filehashes themselves correct. Future preparer now rebases six whitelist paths and checks no old-root entries; no Fresh11 repair/replay. Need actual reviewed_job validation with refusing fakeworker to prove all predispatch checks before another launch; no extra render attempted.
Embedded offline target installed; stdlibzip import-guard qualification, full final-path DLL/model/tool loader evidence and GPLcorresponding-source/notices still open. No portable releaseclaim.

Fresh11manifestclosure defect resolved in futurepreparer and provenbyactualproductionvalidator with8137leases plus staleFresh09negative. No liveconfirmation, no retry. Embeddedverifierfrozenoriginbug corrected; realnewunknownWindowsimage pin remainsHOLD (digest in PREWORKER_CHAIN_12.md), notpermissionproblem or solvedCOMCTLcase. No path/signaturecaptured fornewimage, no identityguess.

## Fresh13 resolves production headless rendering; package gate remains
Full no-dispatch closure proof followed by one actual successful render/restart removes the earlier preworker/runtime blocker for this exact installed runtime and approved synthetic input. It does not qualify the new embedded package. Its first unlisted native image is exact signed Microsoft System32/uxtheme.dll SHAe2e972e21a16c62e7c678334dc50f33c2f327c1b6854a2fb096ca244ae650f27; not admitted. ZIP-stdlib bootstrap, remaining native image inventory, final-folder execution and source/license closure still block portable-package claims. See HEADLESS_RENDER_13_RESULT.md.

Embedded package05 correctly HOLDs at signed host clbcatq.dll after ZIP/import checks pass; no allowlist widening. Production embedded bootstrap/model/tool and no-render app lifecycle not yet qualified. Fresh13 AAC is124k configured/111995bps actual, below ceiling but not128k configured. FFmpeg complete static dependency source set unavailable; local PC review only, no binary distribution/closure claim.

Prior embedded host-image blocker resolved by complete exact14image signed batch, realbootstrap+CPUmodelpassed142nativepins. No broadtrust. Remaininggate is new no-authority package app269209...bc9ffa exact launch qualification; compiled/tested only. Corresponding-source redistribution limit unchanged. AAC futuretarget128k nowexplicit/tested, historicalFresh13 stays124k.

Package09 pending executable gate cleared by exact approved scoped run: no-authority startup/read-only writer-refusal/ordinary close PASS, no worker or OPS consultation. Network and publication remain out of scope. No installer/redistribution or128k-render proof inferred.

## Mac preview cache safety and external sync

The installed Mac worker manages only newly generated, registered previews in the macOS per-user cache. It preserves legacy/unregistered previews and does not evict Google Drive media. Independently review root-locality checks, including non-File-Provider third-party mirror/backup configurations and the non-atomic check/unlink race, and test real-world timer behavior with disposable media before relying on or expanding its use. Source identity uses file metadata, not a content hash; a replacement preserving those values or a stale path-only posting report may remain ambiguous. An unresolved pending path, over 500 outstanding paths, a database snapshot over 64 MiB, a database change, or inspection taking over three seconds prevents a sweep. Queue state is local and does not prove that another machine has no pending work. Browser IndexedDB previews are separate.

One streamed preview was observed without a cache registration; the initial registration failure was not logged, so its exact cause is unknown. The isolated fix retries changed source metadata, avoids adopting or overwriting old unregistered previews, and reports a generic reason. Further real File Provider validation remains necessary.

## Open — Parallel macOS test interference

On 2026-11-19, the full Rust test suite failed seven preview-cache tests under default parallel execution; all 40 passed with `cargo test -- --test-threads=1`. Investigate shared test state before relying on parallel runs. This does not establish Windows behavior.

PR16 semantic conflict safeguards: upstream unconditional auto_queue startup is not accepted in review modes; upstream Mac cache cleanup worker is not activated on adapted. Added policy/no-effect tests, fixed cross-branch App constructors. macOS-specific runtime remains unverified; existing upstream cautionary history is retained and does not describe enabled cleanup in this adapted candidate.
