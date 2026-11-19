# Trusted registry and immutable observation provider

Incremental isolated patch atop controller-worker-evidence.patch (dee64e9337905ea86b1f96ee506ae6a99a10abfcbb251f086ee78a0088586986), itself on exact adapted d3f8f43 Controller slice. No Windows Stage 1 builds or fixtures changed.

## Implemented split

controller_registry::configure receives a dedicated protected registry Connection and an explicitly supplied owner Authority. It refuses read-only setup, foreign worker/product tables, and unsupported schema versions. Only this explicit owner setup creates registry tables. Default App construction remains controller_worker::Unavailable; no environment path, HTTP action, fallback or automatic row discovery enables it.

configure returns separate OwnerWriter and Provider objects with private internals. OwnerWriter::register requires exact independently admitted approval, bounded typed fields, expiry, monotonically increasing registry revision and compare-and-swap against the expected previous revision. Denied registration writes no row. OwnerWriter is never given to HTTP/UI code. Provider implements the existing read-only Reader; its only operation selects the review and returns a copied immutable View. The registry connection is shared internally to serialize writes and reads, but the UI object exposes no write/configuration methods.

Review schema controller-reviewed-cut-v1 contains approval ID/revision/expiry; full bridge binding (Controller job ID/revision, source binding, full SelectedCut including source hash/revision, generation, provider identity, cut interval, recipe and durable cut key); expected worker job ID and root identity; exact expected receipt digest, artifact digest and artifact size. No path inference. These expectations require independent owner review; the registry does not fill them from filenames, worker receipts or UI request bodies. Pending work without such review remains UNKNOWN. Receipt schema alone cannot grant authority.

## Immutable observation capture

Only OwnerWriter::observe may capture. It first removes any older cached observation, verifies the admitted review and current retained LeaseWitness, checks the receipt's exact raw hash plus expected worker job/artifact identity, then calls the existing bridge/unique committed receipt+artifact validator on read-only worker SQLite and supplied File handles. It rechecks owner authority and lease validity after validation before storing a copied View bound to the serialized review hash. No worker path opens, writes, queue commands or media persistence are performed by this provider. Worker database must already be opened read-only and bounded by the trusted capture adapter. The owner capture adapter must set a short SQLite busy timeout and bound filesystem operations.

The cache contains only an immutable result value, never open worker handles, credentials or mutable receipt objects. UI reads do not reopen worker data or recompute evidence; they recheck exact live owner approval, expiry, current job/source binding, review hash and observation freshness (30 seconds, no future time). Existing Controller endpoint also checks source and job currentness before/after reading. No Controller state/preview/review transition is applied. A captured READY result means verified snapshot evidence, not continued worker liveness. Failed capture invalidates old cached completion. HOLD observations may remain conservative; only a new successful capture can replace them.

Durable registry rows survive restart; cached success does not. Reconfiguration begins with an empty cache and requires an explicit new live Authority; rows alone cannot reconstruct owner authority or completion. A new capture is needed before READY can reappear. No enqueue/retry is possible through Provider.

## Live integration boundary

Authority::allows must be a bounded read-only verifier against independently admitted owner state (not a permissive callback, self-asserted field or approval inferred from the stored row). LeaseWitness::current must bind the exact protected root/source/queue/receipt/artifact identities and retain real immutable OS leases throughout capture. No live implementation of those security roots is invented here; synthetic tests use exact-review approval and explicit fake leases only inside cfg(test). Existing Windows pinning code is not rebuilt or silently wired into this Stage 2 slice. Default UNKNOWN remains until the owner supplies reviewed bindings and those concrete adapters. This implements the registry and immutable cached observation provider, not a cross-platform lease opener or production owner-auth service.

Trusted host integration sequence: configure dedicated registry with admitted Authority; retain OwnerWriter privately; register exact Review after independent approval; pass only Provider as App.controller_worker; capture from owner-opened immutable leases with current Observation; UI GET reads copied evidence. Never expose register/configure/observe or caller-supplied Review/LeaseWitness through a browser endpoint.

## Verification

Five new registry_bridge tests pass: exact owner admission/CAS, immutable read-only UI and worker-byte preservation, receipt identity/lost-lease rejection, no cached completion after registry reopen, and refusal to initialize in a worker ledger. UI provider reads succeed under registry PRAGMA query_only=ON with unchanged total_changes; worker DB bytes and write count remain unchanged. 42 previous tests filtered out. Synthetic artifact bytes are not video/playback evidence. Initial SQLite u64 conversion compile error corrected with checked i64 conversion. No native launch, old-test reruns, executable package, live service or remote writes.
