# Review installer route - verified tool inventory, no compiler/installer execution

2026-11-19: Existing SDK x64 tools under C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0:
- makeappx.exe: Valid Microsoft Corporation; SHA25658d5e69fd0322f960afbe34afed0eda66a5d001811ee838d1c7165130c3d55e5
- signtool.exe: Valid Microsoft Corporation; SHA2560d0189af3356554a462c2a60d0d59f6006b68d743f5c0cc93c530b083706f075
System32 iexpress.exe and makecab.exe also have Valid Microsoft Windows signatures. These are cabinet/self-extraction tools, not evidence that our install/upgrade/uninstall lifecycle is implemented. Do not substitute them for a verified installer.

MSIX: build tools exist, but no suitable existing signing authority is established. Normal installation requires trusted package signing. No certificate creation/import, Developer Mode, sideload policy change, paid signing, or Store submission is authorized here. Therefore MSIX is not an immediately installable route under current constraints.
Official reference: https://learn.microsoft.com/windows/msix/package/signing-package-overview

Selected route: official NSIS3.13 portable compiler from https://sourceforge.net/projects/nsis/files/NSIS%203/3.13/ (linked by https://nsis.sourceforge.io/Download). Existing makensis/Inno/WiX not found in checked PATH/standard locations. Official download from Windows failed DNS; no bytes received/authenticated, no mirror alternative attempted. When supported official retrieval becomes available, save to create-only local tools directory, establish vendor-linked archive identity/checksum and executable authenticity/provenance before compiler execution. Do not invent a publisher signature or treat self-generated hash as authenticity.

Compile existing review-installer.nsi only after payload/runtime closure and native render/reopen proof. Per-user RequestExecutionLevel user, no autorun, no UAC/security edits, no paid service. Current draft is incomplete: it contains app/helpers but not a relocatable qualified speech/FFmpeg runtime. Choose explicit qualified local prerequisites or a separately licensed/provenance-pinned redistributable runtime, then rebind installed paths and validate. Do not call today's payload a runnable unified installer.

Acceptance: fresh isolated install, source selection and one explicit reviewed render, captioned preview and same receipt after app-only restart; normal close leaves zero owned workers. Versioned payload separate from durable DB/media/queue; upgrade preserves a consistent SQLite backup including committed WAL state, never copy live main DB alone. Preserve previous version/data for rollback. No social OAuth required; publishing off. Installer itself remains unbuilt/unlaunched/unverified.