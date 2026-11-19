# Staged local producer

Entry point: `helpers/clip_worker/local_producer.py::run_local(preparation,
workspace, config)`. This is a normal in-process Python worker adapter, not a
recovery executable or owner transport. The main app's preparation endpoint still
cannot render. An explicitly admitted caller is required; there is no render CLI.

The producer config has exactly these fields:
`schema` (`local-clip-producer-v1`), `enabled` (default must be false),
`source_sha256`, `cut_key`, `model` (absolute local directory), `model_files`
(exact leaf-name to SHA256 inventory; requires model.bin/config.json/tokenizer.json),
`device`/`compute_type` (`cpu`/`int8` or `cuda`/`float16`), `ffmpeg`,
`ffmpeg_sha256`, `ffprobe`, `ffprobe_sha256`, `nvidia_smi`,
`nvidia_smi_sha256`, `gpu_uuid` (independently reviewed exact NVIDIA UUID), `asr_timeout` and `render_timeout` (integers 1..180 seconds).
The reviewed preparation config is unchanged. Config is not a substitute for
user admission, intake/rubric verification or a queue claim.

Runtime behavior:
- Recheck independently pinned source/cut, tools and complete local model files;
  retain Windows read leases denying writes/deletes through the attempt.
- Require existing faster-whisper/ctranslate2/numpy; never install or download.
- Refuse any reported GPU compute process or nonzero/unknown encoder use.
- Decode only the exact reviewed cut to exclusive PCM, then a bounded ASR child
  uses local_files_only with offline environment and word timestamps. Record
  actual backend device/compute type, installed versions and model file pins.
- Child exits before NVENC; no CPU encoder fallback. Output is 1080x1920 H.264,
  yuv420p, stereo AAC 48 kHz at 124 kbps; standalone export validator enforces
  audio <=128 kbps, plus duration, faststart and full-decode checks.
- Create-only intermediates and `.partial` output; flush and validate before
  Windows same-directory no-replace rename, then existing queue finalizer writes
  the exact receipt and durable completion proof. Partial/ambiguous states HOLD.
  Nothing is cleaned, adopted or automatically retried. Not power-loss qualified.
- Normal timeout/cancel terminates only the owned direct child and waits; crash
  containment of abrupt parent death remains a native qualification item.

Evidence as of 2026-11-19: five pure/fake contract tests passed (disabled and
missing dependencies, model inventory/tool pins, exact encoder/no-overwrite args,
owned-child timeout and busy/unknown GPU refusal). Source syntax checked after
adding native read leases. No actual model, child encoder, GPU or original media
was used. Windows leases, model load and producer hardware path are UNTESTED.

Local speech dependencies are now installed in ignored `tools/speech-runtime`,
with exact wheel pins in requirements.lock and model pins in model-provenance.json.
Offline model-only loading passed; no transcription was performed. Requested CPU
int8 resolves to int8_float32 on this installation. The backend gate accepts only
CPU int8 -> CPU int8 or int8_float32, and CUDA float16 -> CUDA float16; all other
pairs fail closed. ASR evidence retains both requested and effective fields.
A focused backend regression passed without loading a model or invoking hardware.

The ignored `tools/native-smoke-cpu-int8-01` preparation is NOT runnable authority:
producer.disabled.json is disabled, source/cut pins are null, work is empty, and
plan.json records the pending source/review steps. New speech text is prepared;
no WAV/video, intake attestation, rubric approval or queue has been manufactured.
Proposed cut is 2000-32000ms, subject to actual speech/source review. Source
creation and independent review must complete before a separately dispatched
single hardware smoke. Original completed clip/key are excluded.


## Preventive GPU admission change (2026-11-19)

`stage_queue.run_reviewed` invokes the native caller's read-only preclaim callback
only after unresolved/completed queue checks and before Queue construction,
manager start, enqueue or claim. Completed read-only reuse does not scan GPU.
`run_local` always supplies that callback. Postclaim and preencode guards remain
independent observations; a later failure retains the durable claim with no retry.

`gpu_guard.py` requires an explicit UUID, exactly one reported GPU at index zero,
recognized WDDM/TCC mode, known zero encoder utilization and no compute rows.
NVENC now explicitly selects index zero. Multiple/mismatching targets, nonzero
utilization, foreign compute PID, WDDM N/A, unexpected output, timeout, missing
query or nonzero exit all fail closed. No PID exception or process termination
of foreign workloads exists. This does not prove GPU exclusivity or classify
all Windows graphics-engine use; unsupported telemetry remains a blocker.

Each stage creates an exclusive `gpu-{preclaim,postclaim,preencode}.jsonl` in the
isolated workspace. Query output is drained with at most 4096 captured bytes;
10-second query timeout, bounded owned-child shutdown. Log includes exit,
byte count, overflow/timeout/unavailable and allowlisted numeric/UUID/driver
stdout only; arbitrary stdout is redacted with a digest of captured bytes.
No process names, command lines, stderr, credentials or media are logged.

All older producer configs missing gpu_uuid now fail closed. No existing held
config or evidence was repinned. A future independently reviewed target UUID
is required; no real GPU query was performed for this source change.

Evidence: four new synthetic tests cover zero queue on ten preclaim rejection
cases; postclaim race retains one claim and refuses re-entry; preencode race
never starts encoder; explicit target and exclusive diagnostics. Two affected
config/encoder tests pass. Native telemetry and thread/timeout behavior remain
unqualified by these mocked-query tests.

Held smoke preservation: leave the whole v2 work directory, manager/claimed
row, once-claim, decisions and logs in place permanently. Do not reset, replay,
or reinterpret a missing output as retry authority. An optional archival copy
must be separately authorized and use a consistent SQLite backup/snapshot or
retain the entire quiescent workspace and all sidecars, never main DB alone.
A future test requires genuinely safe observed GPU state, reviewed UUID, new
create-only workspace and a separately approved new attempt (prefer a distinct
synthetic source/cut identity). The existing held job is never its input queue.
