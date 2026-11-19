# Shared to-do and project agenda

Last reviewed: 2026-11-18. This is a working agenda for both collaborators and their AI assistants. It records the owner's current direction, not a requirement that every fork look or work the same way. Your own ideas, channels, platforms and design changes are welcome. Agree on shared behavior before merging changes that affect both installations.

Use the boxes for **future work**, not as a claim that a feature is finished. Priorities are a suggested order and can change. Check the code, `PROGRESS.md`, `ISSUES.md` and recent pull requests before starting: this list is a handoff, not a substitute for the current repository.

## Product direction

Build a free, local-first Windows creator hub that helps a person manage their own posting profiles and separate content sources: collect and organize media they have permission to use, understand original posts and account performance, choose content, prepare a queue, and eventually publish and verify posts across accounts. Keep the first view simple and support categories for different projects or channels. Reuse suitable open-source components where they help, checking licenses, maintenance, platform permissions and deployment costs first. A fork may pursue a different design or use case.

The full vision is **not finished**. In particular, a scheduled item does not post itself, private account Insights are not connected, and a paid service's entire feature set is not present. Do not imply otherwise in UI, release notes or AI handoffs.

## What exists today

| Area | Current implementation | What has not been established |
| --- | --- | --- |
| Accounts and categories | Separate profile and source views, categories, link-based name/platform detection, optional local profile pictures. | Reliable automatic profile-picture retrieval and actual account authentication. Platform access and link recognition can fail. |
| Downloading | Source and profile sync use external tools and download archives; TikTok profiles can retry with an owner-verified account ID from a saved post, and Instagram can retry previously captured links with partial coverage. A separate Solo Posts hub accepts individual links. New sources can preview posts and select a cutoff. | Complete live account sync remains unverified. Captured Instagram links can become stale, and previews can be incomplete. Selection is capped at 1,500 posts. Solo posts have a separate archive; photo sync is limited for selective sources. |
| Dates and metadata | Source listings read sidecars; date audit can check locally or request missing dates online without re-downloading media. Unambiguous TikTok IDs can yield labeled UTC estimates. | Full metadata recovery for every file; estimated dates are not confirmed publishing dates and can differ around day/timezone boundaries. |
| Selection and queue | Picker shuffles source photos/videos with source, category, type, date, text and available-duration controls. Optional per-profile rules queue random matching files on a timer while the Hub is open. The handoff provides revision-checked results and persistent claims. | A working scheduled assistant and actual posting are not yet validated. Automatic success verification, a full carousel as one selection, and duplicate detection across different post IDs remain unfinished. Manual library import still copies files. |
| Analytics | Manual snapshots, template CSV import, charts, and an optional paired Brave extension for visible profile counts. | Installed-extension end-to-end verification, private Insights, full account connection/OAuth, universal platform exports and automatic metrics for all posts. |
| Collaboration | Private GitHub upstream, branch/fork and PR workflow, issue/progress notes and research documents. | A GitHub clone does not include either person's local database, profiles, Drive folders, downloader executables or media. Access/invitation state can change and should be verified. |

Evidence so far includes isolated unit/smoke tests, a bounded live YouTube test-channel download, an Instagram reel and partial browser-link fallback test, and limited metadata-only date checks. These are not proof of complete channel downloads, successful publishing, private Insights, every platform/account, or every operating system. See `PROGRESS.md` and the research documents for details.

## Suggested next work

### 1. Make the existing experience dependable

- [x] Include Solo Posts in the source picker and library with original account/platform where available, and make quick-edit controls optional. Verify remaining missing metadata and carousel behavior separately.

- [x] Resolve the bundled or configured FFmpeg executable for yt-dlp calls, including Instagram individual posts and source sync; test discovery and arguments using disposable data. Live platform retry remains to be confirmed.

- [x] Allow category renaming while preserving membership and category appearance, improve the multi-category layout, show a one-hour new-media countdown, and open linked account folders.
- [x] Provide a clearer source picker with filtering, original details and playable previews before queueing; validate real-platform coverage separately.

- [x] Show source sync states and 24-hour tracked additions, add user-set dead status, restrict Sync all to auto-enabled active sources, and expose category icon/color/image settings. Validate behavior against disposable data.
- [ ] Validate the new quick editor with a wider range of formats, portrait/landscape videos and audio on collaborators’ machines; add interactive crop positioning, richer trimming and saved logo presets if needed. The current editor uses centered crop presets and an uploaded overlay.

- [x] Add profile/source and category context menus with reversible account removal, including preservation of local files, history and category membership. Automatic sync and Brave refresh stay off on restore.

- [x] Bring each role’s categories into its Profiles or Sources view, with a profile category dashboard and a clearer source cutoff selector. Validate using disposable data; real-platform feed order may differ.

- [ ] Verify the Brave extension end to end on an authorized test installation: installation, pairing, initial refresh, schedule, restart, disconnect/revoke, rounded counts, errors and after-post refresh. Keep cookies and platform credentials out of logs and Git; the current extension does not extract them.
- [ ] Validate solo-post links and source previews against explicitly authorized real accounts, including restricted posts, thumbnails, photos and source ordering; add safe pagination beyond 1,500 and cross-archive duplicate detection.
- [ ] Validate source and own-profile sync on explicitly authorized test accounts and representative media, including TikTok, Instagram videos/photos, YouTube and restricted/unavailable cases. Document what actually downloaded, failed or required access. The current Instagram visible-link fallback is partial.
- [ ] Investigate remaining missing thumbnails and profile pictures; use existing local files for previews and do not re-download archived videos just to create a thumbnail. Add a safe automatic avatar lookup only where it is reliable, with a manual fallback.
- [ ] Verify complete real-account TikTok and Instagram sync with the owner, including post/photo access and refreshed browser links; the metadata-only checks do not establish complete coverage.
- [ ] Verify upgrades against disposable copies of real-format databases and media layouts; preserve settings, archives and existing media. Test disconnected/moved folders, interrupted sync, and concurrent operations.
- [ ] Improve source date coverage and other missing metadata (titles, descriptions, links, duration) without overwriting originals. Preserve provenance and distinguish confirmed dates, ID estimates, file/download dates and unknowns. Keep online audits opt-in and cancellable.

### 2. Make the content picker and queue a complete workflow

- [x] Add opt-in per-profile auto queue rules with saved filters, interval and queue limit; show the countdown and supply per-profile assistant instructions. Automatic and manual length filtering use metadata or matching scanned durations. Validate live social posting separately.
- [x] Add a structured handoff result input and persistent per-post claim with review recovery after interruption. Validate the complete scheduler, browser session and real upload before relying on unattended operation.
- [ ] Connect source downloads, the manual library, and own posted content using stable platform/account/post/asset identities and **references to existing files** where possible. Do not silently duplicate video files or claim that different post IDs are the same video. Handle moved or unavailable files.
- [ ] Represent a multi-photo carousel as one post with its ordered assets; make photo and video selection, preview, duration and date filters work with incomplete metadata. Keep filtering and shuffle straightforward.
- [ ] Improve the queue manager: edit/remove/reorder or reschedule safely, distinguish draft/queued/posted/failed/needs review, show original title/description and source/post links, copy useful text, and preserve history. Items remain active until their status is recorded or checked off.
- [ ] Make the launch cutoff and date/timezone handling explicit and configurable. Do not assume the example cutoff is the actual GTA launch time, and do not treat date-only or estimated timestamps as exact.
- [ ] Build and validate an AI-assisted workflow that inspects media, drafts captions and operates an authorized posting flow. The local handoff and activity log do not themselves publish or verify anything. Persistent claims coordinate compliant operators; they do not prevent manual or external posting and require human recovery if interrupted.

### 3. Build real analytics and secure connections

- [ ] Choose the first provider and define the metrics it actually exposes, account eligibility, developer registration, scopes, authorization, refresh/revoke, limits and app-review requirements. Build and validate one connector end to end before generalizing.
- [ ] Use official OAuth where available and protect credentials with the operating system's credential store. The owner is open to signing in and saving credentials when needed, but this does not authorize putting secrets, browser-cookie exports or personal account data into Git, logs or shared issues. Support disconnect and revoke.
- [ ] Separate account vs post metrics, lifetime totals vs daily activity, actual vs rounded values, unavailable vs zero, and observation time vs post time. Show source and freshness; never synthesize missing data or combine incompatible platforms into a misleading total.
- [ ] Add real provider-backed profile and category dashboards, useful date ranges, post performance, follower trends and export/import mappings where authorized data supports them. Add corrections, deletion and backup/restore for analytics history.
- [ ] Continue evaluating open-source building blocks. `ANALYTICS_RESEARCH.md` and `BROWSER_ANALYTICS_AND_PUBLISHING.md` record candidates and tradeoffs; validate current project health, license obligations and service costs before adopting code.

### 4. Prepare reliable scheduled publishing

- [ ] Pick one provider and a concrete, authorized test account. Research its current posting API, supported media, scopes, account eligibility, privacy rules and app approval before implementing. Consider an approved browser-assisted workflow only if explicitly requested and appropriate for that platform.
- [ ] Implement durable per-target jobs with timezone-aware due times, opt-in profile/category settings, a clear preview/approval path, and separate preparing/uploading/published/failed/needs-review states. Decide whether jobs run only while the app is open or through an explicitly configured background service.
- [ ] Make retries and restarts safe: use leases/idempotency where supported, reconcile an ambiguous upload before retrying, and never mark a post successful merely because a button was clicked. Save the actual provider post ID/link and confirmed status independently for each destination.
- [ ] Add tests with fake providers and deliberate failures first. Perform a real upload only when the user has explicitly chosen the account, content and visibility; verify it on the platform and then record the result. Nothing in the existing queue authorizes an immediate post.

### 5. Make sharing and releases easier

- [ ] Validate a credential-safe Windows-to-Mac transfer and path remap on disposable fixtures, coordinate a single owner for queue/download writes, then separately verify a real handoff with the owner's data.
- [ ] Provide a repeatable Windows build/release and a friend-friendly first-run setup with no preloaded personal profiles or paths. Validate database migration, backup and recovery before replacing an installed executable. macOS support needs separate verification.
- [ ] Decide whether the collaborators need shared remote queues/data. The current server is localhost-only and unauthenticated; a GitHub fork shares code, not app data. Design authentication, authorization, backups and private networking before exposing any shared service.
- [ ] Improve accessibility, small-window layout and usability without forcing either collaborator's personal workflow on the other. Track different ideas in branches or issues and agree on what belongs upstream.

## How to use this list together

1. Before a task, read `AGENTS.md`, this file, `COLLABORATION.md`, `PROGRESS.md` and `ISSUES.md`; inspect current code and Git state. Ask the person you are helping for their own goal when it differs from this agenda.
2. Choose a small outcome and note dependencies. Create or reference an upstream GitHub issue when useful; do not invent issue numbers. Work in your fork or a focused branch and open a PR so the other person can review it.
3. When you finish or learn something, update this list, `PROGRESS.md` and relevant `ISSUES.md` entries in the same change. State what was implemented, what was actually tested and what remains uncertain; link real PRs/issues when known.
4. Do not check off an entire item just because a prototype or one account works. Split it into smaller items when needed. Keep personal branches and preferences distinct from agreed shared defaults.
5. Never commit local data, media, downloader binaries, account exports, credentials, cookies, logs or archives. Use synthetic fixtures for routine tests; keep live-account tests and publishing explicit and narrowly scoped.

Detailed references: [collaboration](COLLABORATION.md), [known issues](ISSUES.md), [progress](PROGRESS.md), [analytics research](ANALYTICS_RESEARCH.md), [posting research](POSTING_RESEARCH.md), and [browser analytics and publishing](BROWSER_ANALYTICS_AND_PUBLISHING.md).


## Adapted Stage 2 Controller integration
Controller/review state, read-only worker evidence, and owner-authorized registry are available as isolated app functionality. Live worker/owner/OPS/publishing adapters default unavailable. Stage 1 runtime/transport/routes excluded. See STAGE2_INTEGRATION.md for dependency extraction, validation and limitations.

- [ ] Qualify the main-app saved-video preview with actual playback/captions and ordinary close/reopen of the same bytes; do not rerender. See SAVED_PREVIEW.md.

- [ ] Complete the normal standalone worker dispatch and receipt-to-Controller bridge after dry-run review; preparation currently does not render.

- 2026-11-19: Complete new synthetic speech source and independently reviewed intake/cut/rubric before enabling one CPU-int8 ASR + NVENC smoke; no original clip reuse.

- 2026-11-19: New ignored synthetic source created via installed SAPI and CPU fixture FFmpeg only; source 1e9ef1f7...02b16b0, proposed cut 2770-42430ms. Independent rubric/visual review remains HOLD (static abstract visuals do not demonstrate spoken movement). No intake/review approval, queue or target render.

- 2026-11-19: Separate ignored V2 synthetic source adds observed movement/shot/exit payoff; SHA d47f1620...0652bef. Cut bounds pure check passed 2770-42430ms. Review remains independent; seven labels have no descriptive anchors locally and verifier minimum is 5/7, not authority to lower user threshold. No target worker/render.

- 2026-11-19: Independent review must establish safe actual GPU UUID/telemetry before any fresh separately admitted smoke; preserve held attempt permanently. Native query execution remains unqualified.

- 2026-11-19: Before HTTP render integration: durable exact Controller admission/worker mapping and independently pinned runtime/helper, supervised outside request; retain fail-closed restart.

- 2026-11-19: Implement independent operator Authority/runtime verification and outside-request one-shot worker supervision bound to controller_admissions; keep HTTP real render disabled until qualified.

- 2026-11-19: Implement qualified cooperative Python cancellation/descendant containment and module/config identity manifest before installing native Launcher; fresh E2E plan is disabled.

- 2026-11-19: Bind full production worker/module/config/dependency inventory and cooperative bootstrap to the qualified process primitive before installing real Launcher; keep renderer disabled.

- 2026-11-19: Instrument/localize native diagnostic phase/PID/exit and qualify imports/STOP; propagate manifest policy to ASR child before fresh smoke. Do not extend deadline or assume pass.

- 2026-11-19: Localize dependency/native import within faster_whisper using bounded stack/subphase evidence; qualify DLL loading and ASR child. No timeout inflation or verification removal.

- 2026-11-19: Before fresh approved synthetic smoke, qualify closed native DLL loader identities and actual ASR child using retained manifest leases/pinned venv; then independent fixture review and Naga launch preflight. Import stall resolved; no real dispatch enabled.

- 2026-11-19: Integrate observed DLL policy into real ASR path before fresh fixture/smoke; preserve parent leases and fail-closed unknown load. No inference/lazy-load qualification or real render claimed by no-media model diagnostic.

- 2026-11-19: Resolve local SAPI voice enumeration or obtain reviewed generated audio; qualify one bounded CPU inference with new DLL controls, then complete fresh hardware source/cut review and launch preflight. No new queue/render enabled.

- 2026-11-19: Resolve tone inference HOLD with fixed phase/error diagnostics under separate scope; no claim decoder exercised. Bind production Launcher/reviewed-job bootstrap only after this qualification. Spoken hardware fixture still independently unavailable.

- 2026-11-19: Complete strict reviewed-job Python bootstrap adapter for new native Launcher; preserve disabled startup. Qualify corrected diagnostic PCM inference under subsequent scope. Fresh spoken fixture and independent review remain required; OPS publishers separate.

- 2026-11-19: APP review exact fresh flite source/cut entryc5832d9d...; then assemble immutable changed runtime/job config and isolated Controller preflight. No hardware until independent approval and unchanged GPU guards.

- 2026-11-19: Before final launch approval, implement concrete explicit isolated manual entry and independently bound Authority/RuntimeAuthority; rebuild and repin. APP review/config/job/GPU identity fields pending; no automatic/startup route.

- 2026-11-19: Provision final isolated source/queued Controller job/request/config+GPU UUID/expiry and get Naga plan/new-app launch binding before single hardware attempt. Run actual shutdown/restart/GET reuse acceptance; installer compiler and packaging qualification remain pending.

- 2026-11-19: Await exact a7428a46...48126e launch go-ahead and Naga planSHA dbac1cc5...d96f92 review before21:12:20Z. One render then same-plan normal-window restart/GET reopen; no repeat render. Installer compiler absent; NSIS draft uncompiled.

- 2026-11-19: Separately dispatch bounded same-build startup capture with retained parent session and exit status before any fresh render authorization/plan. If needed review unapplied two-site UI lifetime diagnostics; no rebuild yet. Installer DNS issue independent.

- 2026-11-19: Obtain separate startup-only dispatch; capture app exit with launching shell retained, no render. Then independently review a new exact expiry/digest chain before hardware. Resolve official compiler retrieval and installed-path runtime qualification before claiming unified installer.

- 2026-11-19: Review normal Edge-child lifetime/startup evidence before another run; proposed fixed diagnostic patch remains unapplied. No retry/render ticket renewal inferred from startup-only authorization.

- 2026-11-19: Review/authorize new0ccf3820...cd7b0b startup-only candidate once, then real render packet renewal separately. Verify explicit UI close and zero owned descendants. Qualify relocation plan under fresh absolute pins before installer claim.

- 2026-11-19: Naga to relay one combined startup/render/close/read-only-reopen ask and independently bind packet57e40ad...a279a before expiry; no separate startup/render approval churn. Installer needs license obligations, relocated runtime qualification and official compiler retrieval.

- 2026-11-19: Independently bind fresh05 server-only packet before single combined render/reopen qualification. Stop Edge retries; normal Brave navigation via supported CUA only. Desktop shell and installer remain separate work.

- 2026-11-19: Root to resolve supported permitted loopback execution context through platform policy before any further app launch. Do not retry TCP/Brave/Edge or change network/security. Preserve failing targeted test and historical05 evidence; visible playback separate.

- 2026-11-19: Root may dispatch exact one-command loopback approval request per docs/LOOPBACK_APPROVAL_OPTION.md; stop on denial. No global access/network/Windows changes. Native shell design and packaging still require implementation/qualification.

- 2026-11-19: Root to separately scope any real app execution through supported approval flow and fresh plan; successful inert loopback probe is not render authorization or expired-ticket renewal.

- 2026-11-19: Root review/dispatch fresh06 via official command approval while unexpired; no silentrenewal. WebView2 SDK/bindings procurement and actual shell compile separate from current app qualification. Private runtime staging must remain disabled until relocation/license proof.

- 2026-11-19: If root dispatches read-onlyGPUdiagnostic separately, qualify exactinstalledNVMLABI/identity and onebounded targetUUID snapshot; unsupported/unknown remainsHOLD. No retries/processchanges or session-only admission policy inferred.

- 2026-11-19: Review docs/WDDM_QUALIFICATION_PROPOSAL.md: choose strict zero-row requirement versus separately authorized read-only telemetry qualification; no render permission implied. Resolve packaging/PREREQUISITE-REVIEW.md license/SDK/relocated-runtime gates before installer.

- 2026-11-19: Nine offline WDDM qualification cases PASS; no GPU query/policy change. Relocated Python -I -S -B stdlib startup PASS with payload-confined prefix/import paths. ONNX Runtime LICENSE/ThirdPartyNotices collected create-only in runtime-license-inventory-02; three notice sets still missing. Source-only WebView2 window seam added, NOT compiled: official SDK bytes/license/checksum absent; no restore/install. Third-party relocated imports/DLLs remain unqualified.

- 2026-11-19: Authorized encoder policy implemented (UUID, fresh <=10%, zero NVML count/session count, agreement, no WDDM residency gate). Five policy and four queue/race tests pass; headless configuration Rust test passes and new candidate built (955b9a01...487387), not executed. Explicit --headless-review requires pinned plan/server-only, suppresses scheduler and uses Controller-only routes. Fresh07 staged pending exact new EXE approval; no GPU query/render. See docs/HEADLESS_REVIEW_07.md. Prior GPU diagnostic proposal is superseded only by explicit operator policy, not automatic admission.

- 2026-11-19: Final-folder portability acceptance now requires original checkout/venv/model/PATH fallback excluded from resolution, consumed import/DLL/model/tool paths verified, and missing copied dependencies HOLD. Recreate a venv at final path from pinned cached official wheels only if needed and complete; no reprovisioning/test performed for this refinement. See packaging/PREREQUISITE-REVIEW.md.

- 2026-11-19: Fresh08 exact readonly predicate trace proves source_unavailable at shared account-profile host validation: platform youtube with lifeinvader.local URL. Prior DB/config/workspace/worker pins pass. Correct next synthetic fixture metadata/source binding only; original guards unchanged. Focused metadata correction test and durable prelaunch diagnostic test PASS, two sanitized HTTP error tests PASS. New diagnostic app compiled 938dc803...7101f4; not launched. Fresh08 held evidence untouched; no ticket renewal or worker/GPU retry.

- 2026-11-19: Truthful local synthetic scheme implemented in Controller resolver; remote account/download path still rejects it, source custody and synthetic provider/ID guards retained. Old YouTube-shaped fixture label superseded. Exact Fresh09 readonly source/config/pin trace PASS with zero DB mutations/dispatch. New unsigned ca0b21f...ab16e7 compiled; packet431fe2...7861c pending new exact approval, expiry22:34:37Z. No GPU/app run. Static payload02 package-path resolution passed, not full relocation execution proof.

- 2026-11-19: One scoped vendor environment A/B proved minimal worker environment reproduces NVML_UNKNOWN_ERROR exit255/hashdc7f...; inherited control succeeds exit0 with same x64 pinned tool/cwd/approval context. Specific required variable not isolated; no production environment/guard change or Fresh09 retry. Three new final-folder resolver synthetic tests pass (reject original same-byte fallback, relative paths, missing/changed files). Full relocated runtime still unqualified.

## Vendor environment qualification remaining
- Diagnose the single source-only matrix helper exit1 using fixed phase/error categories before any further invocation. Do not infer a WINDIR/PATH fix from this failed experiment.
- Actual Rust gated_manifest bootstrap needs a reviewed vendor-only diagnostic entry to qualify any minimal candidate without invoking imports/model/job dispatch. No new executable was compiled or launched for the matrix; production guard and env unchanged.

## Next vendor-only qualification (not render)
Use existing Rust instrumented_bootstrap_once with fresh root and fresh manifest containing current native_bootstrap.py and vendor_diagnostic.py plus already-pinned runtime files. Set diagnostic-vendor mode, vendor_cases allowlist, wait_cancel=false, output exact fresh root/vendor-result.json. This bypasses neither leases nor DLL/import guard. No request/producer config is accepted. Scope one reviewed contained vendor-only command; do not reuse held matrix/Fresh09 or silently repin any live job. No production environment fix until real containment result. Historical matrix source has deterministic uppercase-dict bug; preserve it as evidence.
Packaging final bindings now static-verified; relocated full runtime/DLL use and missing license texts remain open, not a portable/redistributable release claim.

## Vendor environment candidates exhausted for this scoped test
Contained diagnostic proves WINDIR and/or fixed System32 PATH do not resolve NVML_UNKNOWN_ERROR. Do not apply either as a production fix or repeat matrix. Exact missing environment dependency remains unknown despite prior inherited-control success. Next investigation requires evidence-driven different OS/driver requirement, scoped separately; guard stays fail-closed and render blocked.

## Next scoped work
Review docs/VENDOR_OS_DIRECTORY_PLAN.md before any new vendor call. No WINDIR/PATH repetition. Packaging requires one justified origin capture before full runtime qualification; first03 evidence remains intact. No production/render authority implied by either task.

## Next exact qualification
1. ProgramFiles/ProgramW6432 category now causally implicated. Qualify the pair alone and individual keys only in separately scoped contained vendor-only test after addressing immediate Job-exit accounting; no guessing other env keys/full inheritance. No production env patch yet.
2. Stage1 real rendering remains blocked; all held jobs unchanged.
3. Replace package target with official Python3.12.10 embeddable runtime/relative _pth and pinned offline --target wheels. Existing system-image policy can admit only exact reviewed OS image pins, including known signed WinSxS COMCTL32; full new-folder qualification still required. No current release/portable-pass claim.

## Fresh10 ready for exact root/user approval
See HEADLESS_PROGRAMFILES_REVIEW_10.md; new unsigned build/packet, no implicit authority. Combined single render+close/read-only reopen only if unexpired and every live gate passes. Do not reuse any held job.
Final embedded review folder blocked on24 exact wheel artifacts (normal pip cache absent); official runtime saved/verified but unexecuted. Do not substitute copied installed site-packages/venv as completed offline wheel provisioning. Exact COMCTL pin policy available; remaining full final-folder DLL/import/OAuth exclusion and render/reopen qualifications pending.

## Fresh10 held; next preparation requirement
Do not reuse Fresh10 or extend its ticket. Source-only seed-row correction ready. Need independently scoped fresh fixture using same approved binary, exactstartup validation including both versionrows, then new pendingapproval packet as directed by root. No retry on this turn. Finish pinned offline wheelhouse acquisition separately.

Pinned wheelhouse blocker cleared:24 exact wheel artifacts now local+hashverified. Next packaging step is offline --target into final embedded folder and re-pin manifest, followed by scoped final-folder/OAuth-disabled verification. No current portable-pass claim.

## Held Fresh11 / next gate
Stop current attempt; appclosedcleanly, onebootstrap/noqueue/render. Before requesting another plan, verify entire reviewed-job input closure and real adapter predicates with no dispatcher, not only Rust admission checks. Preserve Fresh10/11. Packaging can continue independently from embedded offline target; qualify zip-stdlib policy, finalpaths, exactWindowsimagepins, OAuthdisabled and licenses before delivery. No commit/push untilPhase1proven.

Preworkerchain12 proof now complete withoutdispatch (docs/PREWORKER_CHAIN_12.md). Next hardwareonlyafterfreshrootpacket/expiry+independentupdatedruntimepinreview; samecdc8ea appcanbereused. Fresh11andallheldinputsuntouched. Embeddednativeimageidentity and actualbootstrapZIPloaderqualification required; no broadtrust/fallback. NoPhase2/OAuth/livepublishing.

## Current priority after Fresh13 success (supersedes earlier pending-run entries)
No further render needed for this single headless test. Finish exact embedded Windows-image pin review, ZIP-stdlib bootstrap lease policy, final-folder runtime/app qualification and licenses before portable review delivery. Preserve Fresh13 proof and all held attempts. Then source-only adapted commit/push/PR preparation; no main merge/push. Visual playback/caption quality remains unobserved for this artifact.

Embedded package next gate: exact clbcatq host pin review and complete inventory via one bounded capture; no signature/directory-only trust. Then final-folder production bootstrap/ASR/tool consumption and app startup/close. See EMBEDDED_PACKAGE_QUALIFICATION.md. No source commit/push until required Phase1 package qualification; no rerender Fresh13.

Package09: embedded bootstrap andCPUmodel/native inventory now PASS (no inference/GPU). Await exact new unsigned269209...bc9ffa startup/close approval and scoped test, then final source-only diff/staging/PR. See PACKAGE_09_HANDOFF.md. Do not repeat Fresh13 or claim whole-binary Metricool absence/license closure.

Package09 actual startup/close now PASS: PID30676 exit0/zeroowneddescendants/zerojobs at23:38:12Z. No further launch/render needed. Complete source-only commit and adapted push/PR, no main merge. Local folder only; static FFmpeg corresponding-source closure and visual observation limitations remain explicit.

- [ ] Independently review the installed macOS generated-preview cache integration, especially unknown third-party mirror/backup cases, cross-machine queue state, and its timed runtime behavior; validate further on disposable media before relying on or expanding its use. Do not claim Google Drive file eviction from thumbnail cleanup.

PR16 candidate: review detached normal merge commit before any fast-forward of adapted. Windows offline compile and16focusedtests passed; no macOSruntimeclaim. Keep cachedeletion permanentlyOFF and source/autoqueue timersOFF inreview. Preserve existingpackage/heldjobs; no rerender. Update PR16 only after promotion, no mainpush/PRmerge.
