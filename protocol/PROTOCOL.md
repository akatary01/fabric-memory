# Memory Fabric protocol v1

Transport-independent UTF-8 NDJSON: one complete event per line. Unknown event
versions and fields are rejected. No cloud key, runtime configuration, model
weights, or adapter weights belong in the ledger.

```json
{
  "version": 1,
  "id": "00000000-0000-4000-8000-000000000011",
  "actor": "00000000-0000-4000-8000-000000000001",
  "clock": 1,
  "memory": "project-note-1",
  "deleted": false,
  "text": "Associated source text or an explicit derived summary.",
  "embedding": { "space": "64 lowercase hex characters", "values": [0.1, 0.2] }
}
```

The illustration abbreviates the embedding. Actual payloads require 1–4096 finite
FP32 values and a valid model-space fingerprint. The complete pipeline identity
includes model revision, model and tokenizer hashes, dimensions, pooling,
normalization, numerical format and maximum token count. Keep these identities
stable; changing one requires rebuilding the local projection or a new adapter.

## Event semantics

- `id` and `actor`: canonical lowercase UUIDs.
- `clock`: positive Lamport integer smaller than 2^53−1. A native write allocates
  `MAX(all observed clocks)+1` inside an immediate SQLite transaction.
- `memory`: nonempty printable ASCII key without spaces, at most 128 bytes.
- `text`: at most 65,536 UTF-8 bytes.
- `deleted`: tombstones require empty text and absent/null embedding.
- `(actor,clock)` and `id` are unique. Reusing an event ID with different contents
  rejects the entire transaction. A duplicate identical event has no effect.
- An incoming batch has at most 10,000 events and 32 MiB. Larger histories need
  multiple bounded batches; pagination is not implemented in this milestone.

The event set is a grow-only set of immutable operations. Each memory key is an
LWW register, ordered lexicographically by `(clock,actor,id)`. Here "last" means
logical ordering, not physical wall-clock time or user intent. Concurrent losing
values remain in history. Tombstones participate in the same ordering, so stale
replay cannot resurrect a deleted memory. A genuinely later write can recreate it.

This is a small, explicit CRDT composition, not a claim that arbitrary concurrent
agent state or summarization is automatically conflict free. Narrative entries
should normally use distinct memory keys; mutable facts can share a key.

## Projection and retrieval

Each device stores original events unchanged. A separate head table stores the
winning event and its destination-space vector. A foreign vector requires the
matching source→destination adapter. Imported documents never call the local
encoder. Queries are embedded natively by the receiving device. Cosine search
uses normalized vectors in that one destination space.

Training pairs use identical source texts. Both source and target vectors are
normalized, centered, and fit with ridge regression:

`W = solve(XcᵀXc + λI, XcᵀYc)`

Inference: `normalize((normalize(x) − source_mean)W + target_mean)`.

`weights.f32` is an input-dimension × output-dimension row-major matrix of IEEE
754 FP32 little-endian weights. `adapter.json` binds both model spaces, matrix
dimensions, centers, training fingerprint, ridge strength and SHA-256 of weights.
The reverse adapter is fitted independently. It is not a mathematical inverse.

Changing an adapter requires rebuilding existing foreign projections; automatic
migration and multiple simultaneous adapter revisions are not implemented yet.

## Integrity and privacy boundary

SHA-256 verifies local artifacts; it does not authenticate a remote device.
This milestone trusts Apple's paired-device developer connection for exchange.
It exposes no network listener. SQLite and the local Git archive contain source
text in plaintext within the OS-protected device storage. Application-level
encryption, peer authorization, key rotation, and revocation are later work.

## Convergence evidence

`state_digest` hashes the winning memory IDs and event IDs, sorted by memory:
`memory + NUL + event_id + LF`. `event_digest` hashes all event IDs, sorted, each
followed by LF. These establish matching accepted event identities and winners;
they are not content signatures or floating-point equality checks. Numeric encoder
parity and retrieval behavior are checked separately by the device harness.
