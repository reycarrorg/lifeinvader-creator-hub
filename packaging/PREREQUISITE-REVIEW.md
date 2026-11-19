# Private review packaging prerequisites

2026-11-19; no install, SDK restore or security change. Later stdlib-only relocated Python startup passed; no third-party/model/tool execution was included.

## Verified static slice

`check_runtime_payload.py` checks exact manifest digest, disabled flags, complete file inventory, reparse rejection, safe relative names, lengths and SHA256, required layout, and exclusion of venv launcher configuration. Actual private payload: 8079 files / 924724041 bytes PASS, exit 0. Manifest SHA256 `da144a1864e713d8206b56cd3e7d7cdabdbefcf3ccea01696ae6b9625e68a2e8`.

This proves static copied bytes/layout only. It does not prove relocated imports, DLL resolution, executable runtime behavior, license compliance or an installer. Launch remains disabled. Next runtime qualification must independently pin the relocated import/DLL roots, avoid original-path fallback and use a no-media diagnostic before any application dispatch.

### Final-folder acceptance: no original-path fallback

Python documents virtual environments as generally non-portable because installed scripts encode absolute interpreter paths: https://docs.python.org/3.12/library/venv.html#how-venvs-work . The current staging deliberately uses base Python, excludes venv Scripts/pyvenv.cfg, and maps site-packages separately. That is a candidate layout, not a portability guarantee.

Before labeling a review folder portable, qualify its exact final location and final pinned configuration:

- Use absolute final-folder interpreter and worker paths, a neutral working directory and process-local environment without original checkout, venv, model, user-site or PATH-based tool fallback. Do not rename, remove or change permissions on originals to simulate this.
- Verify sys.executable, prefix/base_prefix, all import origins, script entrypoints and loaded non-system DLL identities resolve only to the final payload. Explicitly permitted Windows system/driver modules remain separately pinned/classified; never copy those into a fake private runtime.
- Use exact copied model path with local_files_only and offline settings; copied FFmpeg/ffprobe must be explicit absolute paths with final inventory hashes. No model cache, download or PATH resolution fallback. Their execution still requires its applicable scoped authorization.
- Keep DB/media/worker mutable state outside versioned payload; OAuth configuration absent/default-deny. Missing final-folder dependency must HOLD, not silently use an installed original.
- Negative synthetic resolver checks must reject original absolute paths and absent copied tools/models. Actual qualified imports and authorized end-to-end execution must report consumed paths, not merely configured strings.

If this candidate cannot meet those conditions, create a new venv at its final path using the exact pinned base and locally cached, provenance/hash-verified official wheels, without index/network/global install or activation-script/security changes. First establish complete cache availability; do not claim that wheels are already complete. Regenerate final-path manifests and requalify. Do not copy an existing venv and call it portable. No reprovisioning or new runtime test performed for this acceptance refinement.

## Redistribution inventory and blocking gaps

Local inventory `tools/runtime-license-inventory-01/inventory.json`, SHA256 `d7966d3331171361a6d8c41313fddba8f472dd803b67ffcdeba8f27f36e3f84c`: 25 Python distributions; 21 have collected notices. Base Python license and FFmpeg license also retained. Metadata is inventory evidence, not legal approval.

Missing collected notices: ctranslate2 4.9.0 (metadata MIT), flatbuffers 25.12.19 (Apache-2.0), tokenizers 0.23.3 (no license classification recorded). ONNX Runtime LICENSE and ThirdPartyNotices were subsequently collected from its local installed distribution in tools/runtime-license-inventory-02. Resolve exact distribution and bundled native third-party notices before shipment. Review transitive native licenses for PyAV, NumPy and ONNX Runtime, not just top-level classifiers.

Installed FFmpeg license begins GPLv3: do not label the payload wholly permissive. Resolve exact build configuration, corresponding source and required notices/distribution obligations before release. Model revision/license provenance, Python bundled components and Microsoft runtime redistribution terms also remain open. No legal clearance asserted.

## Microsoft-supported shell sourcing

Installed signed WebView2 Runtime is not the SDK. The official Microsoft.Web.WebView2 NuGet page currently supplies stable 1.0.4314.35 for native Win32 C/C++; choose exact stable version, not prerelease or floating latest. Future acquisition must record package hash/signature and license before extraction/build. No SDK acquisition or restore performed here.

Native integration options: import the official native MSBuild targets in a small in-project Win32 host component, or explicitly bind the official headers and x64 loader into the existing Rust application build. Neither requires the cancelled external owner wrapper. Select one build integration before dependency acquisition; no claim that installed Runtime alone enables compilation. Runtime deployment options are Microsoft Evergreen prerequisite or separately licensed Fixed Version distribution; existing machine Runtime is sufficient only for local qualification, not a redistributable payload guarantee.

Source: https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.4314.35 and https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution

Installer remains blocked: NSIS not acquired, MSIX tooling exists but trusted signing/deployment route is not established. Do not substitute a directory payload for a qualified installer. Publisher/social configuration stays disabled.
