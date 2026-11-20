# Embedded package qualification: current HOLD

Local folder: `tools/embedded-review-folder-01` (Andre's PC only; not a redistributable release).
Qualification inventory `manifest.package-check.sha256` SHA256:
`92a66a722357e123df2edcac1b7eca5da68b84079a9fa72c269d81bc3cc1cc65`.
Official CPython3.12.10 x64 embedded with relative _pth, 24 exact offline wheels, copied reviewed model/FFmpeg/ffprobe. No pip at runtime, OAuth unconfigured, publishing/deletion OFF. No Windows or driver DLL is copied.

## Completed new slice

Exact host requirement for `C:\Windows\System32\uxtheme.dll` records SHA256 `e2e972e21a16c62e7c678334dc50f33c2f327c1b6854a2fb096ca244ae650f27`, valid Microsoft Windows Authenticode, copy_allowed=false. Existing host inventory is still exact file pins, never Windows-directory trust.

Actual native_bootstrap.py and asr_bootstrap.py origin guards now accept ZIP modules only from a pinned exact container, with a genuine zipimporter, member containment, matching loader filename and unchanged container digest. Existing Rust retained manifest leases remain required; a matching hash alone is not launch authority. Plain file/cache checks remain unchanged. Twelve focused cases passed using the actual function ASTs: both bootstrap functions accept the exact ZIP and reject absent pin, changed pin, external file, member escape and fake loader. No bootstrap entry or worker started in this test.

Scoped package import05 (PID32364) exited2 normally at2026-11-19T23:20:28.190856Z, no timeout. All six dependency imports and post-import Python origin checks completed with exact package-only sys.path, empty PATH, no site hooks, denied network/subprocess and OAuth imports excluded. The paired absent-pin ZIP/wheel checks rejected both. This qualifies the origin function, not a full production bootstrap/ASR child run. Import04 was preserved: diagnostic hash adapter used string.read_bytes; corrected adapter accepted Path/string, not a runtime/security change.

Native inventory correctly HELD on another exact image:
`C:\Windows\System32\clbcatq.dll`
SHA256 `c31a5d06a426c934fb31a14b124356fd6364bb94989cabbdab3ca28e9d9d5803`
Authenticode Valid, Microsoft Windows. NOT admitted. No broad allowlist or rerun. Diagnostic source now accumulates all unlisted image metadata and then HOLDs; not executed. This avoids serial first-image-only observations. Full remaining inventory unknown until a scoped pass collects it.

## AAC correction (no rerender)

Fresh13's exact producer code explicitly uses `-c:a aac -b:a 124k`, not128k. Retained probe measures111995bps over35seconds. This is under the128kbps ceiling; no claim of128kbps configured target. Encoder target and measured stream average are different evidence. The existing artifact is unchanged. Official AAC documentation describes b as the bit-rate setting: https://www.ffmpeg.org/ffmpeg-codecs.html#aac .

## Local-tool license inventory / no distribution claim

FFmpeg9.0.2-full_build-www.gyan.dev exact binary SHA589e50b766d251afdf181dd664d40bd94407e200b019989fd7468c7d118a28d0; ffprobe SHA02264dfa4f56baaa31e443ece8aa16d397f6b008200e7fd184d22b9009e75fd3. Retained -version/-buildconf/-L outputs are in tools/embedded-ffmpeg-license-evidence-01, GPLv3-or-later/static/no enable-nonfree.
Official builder release metadata https://www.gyan.dev/ffmpeg/builds/ identifies upstream commit946fcce07b. This is NOT the complete corresponding-source set for its static dependency build. OPS confirms no complete public set identified; do not contact builder, upload binaries or claim closure. FFmpeg guidance requires exact corresponding source: https://www.ffmpeg.org/legal.html . For now this is local reuse of Andre's existing tools only.

Wheel license files remain in their original package/dist-info locations; onnxruntime LICENSE and ThirdPartyNotices are in the package directory. A dist-info-only scan found no license files for ctranslate2, flatbuffers, tokenizers and tqdm; their metadata/upstream notice closure is still pending. Python embedded license remains in runtime/python. No notices stripped.

## Remaining release gates

1. Independently review exact additional host-image requirements and finish complete loaded-image inventory; do not trust signature or location alone.
2. Qualify the final-folder production bootstrap and ASR child/model/tool path consumption with retained leases; current proof is imports and origin policy only, not model initialization or encoder execution.
3. Stage final unchanged approved app plus newly pinned worker source; run narrowly scoped isolated no-render startup/ordinary shutdown with source timers disabled and zero owned descendants. Current embedded folder has no staged app and is NOT runnable delivery.
4. Verify review-path Metricool exclusion rather than claiming binary absence: baseline still contains inert Metricool projection/schema code. OAuth remains unconfigured. Do not merge old social patches.
5. Complete local notice inventory. Redistribution remains prohibited absent exact corresponding-source/license closure. No binary/model/wheel/media/DB archives in source PR.

Fresh13 render/reopen is preserved and must not be repeated. Source-only PR cannot be represented as a verified portable package yet. No commit/push/main changes performed.
