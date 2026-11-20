# Supported execution approval option (not requested or executed)

The current exec_command interface documents sandbox_permissions=require_escalated with a justification for a single command. Current approvals_reviewer=auto_review reviews such escalation. This is an official per-command approval path, not a Windows UAC instruction or a recommendation to weaken firewall/sandbox settings. It is NOT a technically loopback-only permission grant: the requested command must itself be tightly bounded and its broader execution context disclosed.

Proposed one-command request ONLY after root dispatch:
Run the exact existing Rust test executable tools/ui-lifetime-build-target/debug/build/lifeinvader_hub/fba5cdfc2d7a9ad0/out/lifeinvader_hub-fba5cdfc2d7a9ad0.exe with:
--exact controller::tests::controller_reachability_same_process_loopback_no_worker --nocapture
Test executable SHA2560f7574fd8e3e736b31860dc09c4604931a6cf670399b2382c81dde216b1cff19.
No Cargo build/download or prefix-wide approval. Explicit working directory adapted checkout. Exact test only, not all tests. It binds127.0.0.1 ephemeral port, connects only to that address within same test process, uses inert in-memoryDB/testfiles, GETs true Controller HTML/JS/status and readiness/close, bounded sockets2s/client12s/serverclose2s. No app main, real media/GPU/model/queue/publication or external address. On denial/error stop; no alternative transport/manual desktop workaround.

Suggested tool justification: Allow this one bounded local-only Controller test outside the restricted executor context to determine whether its existing Windows10013 denial is sandbox-specific? It uses only fresh inert test data and127.0.0.1, creates no worker and changes no security settings. The tool exception is per-command, not a network-only capability.

Exact user action IF a normal tool approval card appears: approve only that named test command once, without Always Allow/prefix/global Full Access. Automatic review may resolve it without user interaction. Availability of the parameter is proven by active tool schema; acceptance/success is not guaranteed. No approval requested in this turn. No need for user to launch app or alter settings at this stage.

Official Windows sandbox guidance says command/child network permissions are enforced and approvals independently govern commands with more access. MXC docs mention host loopback with applicable effective network policy and child cleanup on command exit. We have not established which sandbox implementation this session uses and do not change it. Current environment says network restricted; actual test10013 proves denied connect, not the name of the enforcing control.
https://learn.chatgpt.com/docs/windows/windows-sandbox

Independent progress: one new in-process UI/manual-dispatch test PASS, no sockets. Denied before readiness; one fake call after readiness; denied after close; zero DB changes/admissions. This proves route gate composition only, not real worker/GPU/visible playback. Existing approved application executable unchanged.