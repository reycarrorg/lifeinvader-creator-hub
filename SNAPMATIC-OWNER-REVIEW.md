# Snapmatic owner-review build

This is a source-only review snapshot derived from upstream Git commit 5eb2461. It is not the deployed parity-sync executable, does not include an existing workspace, and contains no creator media, account state, or production credentials. Public posting and legacy external dispatch are disabled in this review build. TikTok does not acquire remote media; with independently pinned offline evidence, Sources may explain why the stock extractor is held.

## Build and run locally

Use the stable Rust toolchain supporting edition 2024, Cargo, and an MSVC x64 development environment on Windows (Visual Studio Build Tools, Desktop development with C++, and a Windows SDK). From an x64 Developer PowerShell, in an isolated writable checkout:

~~~powershell
cargo build --release --locked --features snapmatic-hud
cargo test --release --locked --features snapmatic-hud
$env:LIFEINVADER_PORT = '3847'
.\target\release\lifeinvader_hub.exe --server-only
~~~

Open http://127.0.0.1:3847/workspace?view=studio in your browser; stop the foreground process with Ctrl+C. With a prefilled Cargo cache, add --offline to build and test. Use a clean shell with no LIFEINVADER_* configuration inherited from a production launcher. The program creates only local review data by default. If selecting a path with LIFEINVADER_MEDIA, first create a separate writable folder; do not point this review build at a real source, credential or production runtime.

Default demo starts empty, with synthetic labels and no active Python worker, import authority, verified capture, Google Drive source, or vendor credentials. Sources, Gallery, Studio, review state, and connection-disabled UI can be inspected without configuring providers. Local ingestion is source-included but requires a separately reviewed local Python/FFmpeg/ffprobe installation, approved dependencies, and exact authority evidence; the demo does not silently enroll any binaries or enable the MP4 picker. The full Windows native-process test requires an explicitly reviewed installed Python: set LIFEINVADER_TEST_PYTHON to its executable and LIFEINVADER_TEST_PYTHON_SHA256 to the digest you have independently verified. The regular build does not need Python. No supplied fixture uses production footage.

## Provenance

The original repository's history is retained. No new license grant is asserted by this readme. Chart.js is the existing upstream vendored frontend asset; see web/CHARTJS-LICENSE.md. The bundled review artwork is Snapmatic artwork, not creator footage. Rust dependencies resolve through Cargo.lock with each dependency's own license; external Python, FFmpeg, ffprobe, GPU runtimes, extractor wheels, models, release EXEs and automated-browser state are not included. This review branch is intended only for its authorized repository owner. Verify separate redistribution rights before making any public distribution.