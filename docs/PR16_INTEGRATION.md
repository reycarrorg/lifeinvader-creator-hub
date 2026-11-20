# PR16 integration candidate

Prepared in detached worktree lifeinvader-adapted-pr16-integration. Verified adapted branch/package remain at d1d0296fa1edd9aafd9361db2f77d52c9bc5b3f3 and their existing pinned bytes. Incoming main is pinned5eb2461fb22ce30cfcb1155520c226eaa7572e93; no main mutation, force/rebase or PR merge.

Resolved Cargo.toml, main.rs and the three status documents by retaining both legitimate feature sets. sha2 remains global; libc remains Unix-only; rusqlite keeps backup+bundled. Controller initialization/recovery and upstream cache_scope initialization both survive. All App test constructors now include fields from both sides, including macOS-only fixtures.

BackgroundPolicy controls BOTH source and auto-queue schedulers. Both are OFF for headless/package review. Permanent cache-deletion policy is false on every OS, has no environment/config override, and is checked before the cache worker can start a timer and before its sweep entry. main never spawns that worker. Upstream cache implementation and synthetic planning tests are retained, but cleanup is not activated. Package-only routes continue to avoid OPS/Metricool and writer endpoints; no OAuth added.

Offline Windows Rust test compile passed with --locked --offline. Sixteen targeted tests passed:3mode/background-policy,3admission,4recipe,1source-binding,1package-route,3supervisor receipt/pin/timeout/reopen and1cache-worker/sweep no-effect test. The source-scan JavaScript test passed with synthetic delayed responses. No GPU, application process, real queue, runtime manifest, OAuth or network platform workflow was used. Native environment-specific tests remain ignored and were not run.

Limits: macOS-specific compilation/runtime was not available or claimed. Constructor repairs were inspected and parsed by the Windows build, but platform cache behavior needs owner review; the candidate intentionally disables cleanup rather than qualifying it. The shipped Windows review executable/package was not rebuilt, launched or modified. No rerender or deletion performed by this integration task.

Promotion proposal: after review, fast-forward andre/creator-hub-adapted from d1d0296 to this normal two-parent merge commit, then push only that branch to update existing PR16. Do not merge PR16 or touch main. Recheck remote tips first; if advanced, inspect again rather than force. YouTube remains independent.
