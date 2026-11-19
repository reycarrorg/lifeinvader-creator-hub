# Clipping worker handoff (design draft)

## Scope

Lifeinvader may show a local, read-only preview of a clip already rendered by a separate worker. This is a version 1 contract proposal, not an implemented API. The handoff must not discover sources, download media, enqueue work, render again, post, upload, or delete. Editorial review and any eventual queue action are separate, explicit steps; a preview is not approval.

## Version 1 evidence

- Require an exact schema version of 1. Reject malformed records, duplicate or unknown fields, invalid hashes, and unsupported versions; do not guess a fallback.
- Treat source provider, stable provider ID, and source SHA-256 as the source revision. Bind that revision to integer start/end milliseconds and a render-recipe version. The worker's durable cut key must identify that exact tuple, not just a request or display ID.
- Resolve the cut key in an independently trusted worker ledger. Require a committed result for the same source revision, times and recipe, plus a receipt containing the rendered artifact SHA-256 and byte count. A request, file name, timestamp or unsigned preview claim alone is not proof.
- Resolve the artifact beneath the configured local preview root; reject absolute paths, traversal or links that escape that root. Verify the current bytes against the committed receipt before making a preview available. Do not expose filesystem paths or file contents in diagnostics; keep private data and credentials out of Git.
- Return preview-ready only for current matching evidence. Missing, pending, failed, stale, revoked, ambiguous or changed evidence yields a read-only hold with a safe reason. A hold cannot retry, mutate the ledger or unlock a queue. Recheck at review time; any source, cut, recipe or output change requires a new review.

## Disposable acceptance cases

1. A committed matching fixture displays one preview and leaves both ledgers unchanged. The same cut under two request IDs still resolves to one committed result, including after restart.
2. A written artifact or receipt without a committed worker result stays on hold after restart. An unknown outcome does not enqueue or re-render.
3. Changing the source revision, cut times, recipe, receipt or even one output byte prevents preview. Include a same-size replaced file, a missing artifact and a path that escapes the preview root.
4. Unknown schema, duplicate keys and stale or revoked source evidence all fail closed. An explicitly held preview cannot mark a job reviewed or posted.
5. Confirm the entire flow uses fixture media and an isolated database, with no platform calls, live account access, posting or cleanup.

The preview contract complements the repository's manual posting queue; it does not replace it. No implementation or runtime validation is claimed by this document.
