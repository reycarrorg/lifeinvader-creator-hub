# Native review shell design - source decision only

Use an application-owned Win32 window plus Microsoft WebView2 controller for the eventual Windows review shell. This replaces reliance on a transient Edge app-mode bootstrap as the owner. Use the production WebView2 Runtime (Evergreen detection/prerequisite), not Edge Stable as a runtime. No runtime install/SDK dependency change or shell execution here.

Keep the existing Controller handlers and localhost-only transport. This design does not introduce a different transport to evade the current socket restriction; permitted loopback remains a prerequisite. Do not implement an IPC/custom scheme fallback to the denied route. No user browser profile sharing, remote-debugging port, sandbox switches or credential import.

Lifecycle: app owns HWND and WebView2 controller; successful navigation + existing UI readiness enables review controls, not rendering authority. WM_CLOSE first marks UI closing, rejects new manual admissions and invokes existing worker cancellation/join; then closes WebView2 controller and releases owned window resources. Log navigation/process failure with fixed codes and hold jobs, no automatic new browser/worker. Keep interactive message loop responsive during shutdown. Reopen loads durable completed job and verified receipt GET without a render request.

WebView2 controller.Close() is the supported browser-instance cleanup interface; actual zero owned descendants needs testing after implementation. Runtime detection/feature support must fail with clear prerequisite message; do not install implicitly or modify trust/security. All user media and mutable DB/queue remain outside versioned payload. Close and upgrade must preserve consistent SQLite/WAL ownership and backups.

Review installer remains NSIS planned per-user/no autorun, blocked on compiler retrieval and runtime/license closure. Existing MakeAppx/SignTool do not remove trusted-signing requirement. This design is not an implemented shell or validated installer, and is not a new launch proposal.

Official references:
https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller