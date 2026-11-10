# Architecture and diagnostics

Historical implementation and benchmark notes. The current launch and pairing workflow is documented in the main README and BEACON.md.

## Chat on the Mac

Run `fabric-memory run` and type into **Ask this Mac**. Choose **Local**, **Cloud**, or
**Auto**, plus **Web Off**, **Auto**, or **On**. Local uses LFM3-1.7B and never
falls back to cloud. Auto classifies task complexity locally and can choose GPT
when configured. Web search runs through a local, open-source SearXNG service;
both generation providers can use its search results and fetched pages.
Setup downloads the runtime, model, and search service once. Conversations persist.
See [MAC-ROUTING-WEB.md](docs/MAC-ROUTING-WEB.md) for routing, privacy, and setup.

## Phone prompts and live Git sync

TestFlight build 3 bundles LFM3-1.7B Q4_K_M with llama.cpp for on-device chat.
The phone retrieves memory with MiniLM and generates its own reply. Chat requires
no Mac pairing, GPT key or network connection after installing the app.
See [PHONE-LOCAL-LLM.md](docs/PHONE-LOCAL-LLM.md).

To exchange memories, run `fabric-memory run`, click **Pair iPhone**, transfer the JSON
to the phone and import it with **Connect Mac**. Memories sync automatically after local changes and every ten seconds while
the apps are active; **Sync** also refreshes immediately through the private Mac Git host. Use a private Wi-Fi network or Personal Hotspot.
IPv6-only hotspots require build 3 and a fresh pairing file.
See [SYNC.md](docs/SYNC.md) for transport details.

## Open the Mac app

```sh
fabric-memory run
```

This builds and opens the native Tauri window, automatically locating the prepared
models and ONNX Runtime. Save notes, search by meaning, forget memories, import
the JSON shared by the iPhone app, or export the event ledger to Finder. Cards
distinguish native MPNet memories from translated MiniLM memories. No cloud request
is made by these actions. The UI is in `apps/mac/ui/`; the Rust core remains independent.

## Open the iOS app

```sh
fabric-memory run --phone
```

This opens the app project in Xcode. No `npm install` is needed. In this prepared
workspace it opens immediately; on a fresh checkout the same command downloads
the models, prepares the adapters and stages the demo assets once. Subsequent
runs reuse those assets and preserve your Xcode signing settings.

Choose your signing team in Xcode, then use **Product → Archive** for TestFlight.
The manual preparation commands below are for reproducing individual build steps.

This first implementation covers real embeddings, learned bidirectional adapters,
local retrieval, durable event reconciliation, and a separate stress harness.
It is not yet the complete persistent agent described in the original blueprint.
The Mac app has a small, separate Tauri UI; the iPhone app has a conversation screen, Mac pairing, and separate diagnostics.

## Implemented

- Mac: Rust core, Tauri v2 shell, SQLite, native MPNet ONNX inference.
- iPhone: Swift core, SQLite, native MiniLM ONNX inference with a WordPiece tokenizer.
- Two separately trained centered ridge adapters: 384→768 and 768→384.
- Destination-native queries against translated document embeddings. Imported
  documents are **not re-embedded**. Their text travels as content for RAG; vectors
  alone cannot reconstruct the source text.
- Immutable versioned events, logical clocks, deterministic LWW registers,
  tombstones, deduplication, batch rollback, and restart persistence.
- Private bare Git host on the Mac, authenticated HTTPS smart-HTTP, append-only
  pre-receive validation, and atomic event commits. Phone event exchange uses a
  paginated HTTPS API backed by that same Git repository; it does not clone Git.
- Native phone prompt/conversation UI, local MiniLM retrieval, Keychain pairing,
  live sync, and independent on-device LFM3 generation. A separate diagnostics screen
  retains the self-contained embedding comparison and shareable benchmark.
- Separate native and translated evaluation indexes, encoder parity checks,
  native-vs-translated retrieval metrics, and durable-write/search stress workloads.
- Backend-only OpenAI Responses adapter, called only when sending prompts and never
  by tests or stress runs. Local LFM3 generation and SearXNG web retrieval are also available.

## Layout

```text
core/                        Rust protocol, encoders, adapters, SQLite, CLI
apps/mac/src-tauri/           Tauri backend and native window
apps/mac/ui/                  Separate HTML/CSS/JS interface
apps/mac/.env.example         Backend-only cloud configuration
apps/iphone/Sources/          Swift protocol, encoder, adapter, SQLite
apps/iphone/Runner/           Physical iPhone lifecycle/test host
protocol/                    Language-independent wire contract
tools/                       Asset preparation, training, real-device experiment
reports/                     Saved verification and performance results
assets/                      Downloaded pinned models and trained adapters (ignored)
runs/                        Per-experiment databases and exchange files (ignored)
```

## Reproduce

Requirements for building: Apple Silicon Mac, Rust/Cargo, Python 3.12+, Xcode,
and an Apple Developer team for TestFlight signing. The phone installs through
TestFlight; it needs no development cable, Developer Mode, or CLI launch. Dependencies are open source;
Apple's OS, SDK, signing infrastructure and development tools are platform exceptions.

From this directory:

```sh
uv venv .venv --python 3.12
uv pip install --python .venv/bin/python -r tools/requirements.lock
.venv/bin/python tools/prepare.py --anchors 2000
.venv/bin/python tools/train.py
CARGO_BUILD_JOBS=2 cargo build --release -p fabric-core
.venv/bin/python tools/stage_testflight.py
```

Archive, substituting your Apple Developer team:

```sh
FABRIC_TEAM_ID=YOUR_TEAM_ID sh tools/archive_iphone.sh
```

Open the resulting `MemoryFabric.xcarchive` in Xcode Organizer, choose
**Distribute App → App Store Connect**, finish distribution signing, and upload.
Create/select an App Store Connect app with bundle ID `local.memoryfabric.phone`.
No upload is performed by the build scripts. App Store Connect validation and
TestFlight processing happen in your upload step.

Install through TestFlight and open the app. The default screen accepts prompts.
Tap **Diagnostics** to encode the held-out passages with MiniLM, validate the
encoder, translate/search bundled Mac vectors, and test 1,000 durable writes.
Keep diagnostics in the foreground.
Tap **Share results to Mac**, save the JSON on the Mac, then run:

```sh
.venv/bin/python tools/import_phone.py /absolute/path/to/MemoryFabric-testflight-RESULT.json
```

This imports actual iPhone-created vectors, translates them into MPNet space, and
searches using native Mac queries. Both directions are then reported in
`reports/testflight-device-results.json`. Before that step, physical-phone results
are pending. The same Swift encoder and adapters can be verified on the Mac,
but those checks are not represented as iPhone hardware measurements.

Keep `assets/testflight/` with the project: it binds each archived demo to its
matching Mac vectors and native baseline. Each phone launch gets fresh databases.

The Mac staging tool tests 1,000 and 10,000 durable single-event writes; the
TestFlight phone demo tests 1,000, followed by searches. This workload reuses **real** corpus
embeddings to isolate storage and index costs from encoder inference. It does
not claim 10,000 newly encoded passages or establish the device's maximum capacity.

Tests and host build:

```sh
cargo test -p fabric-core
swift test --package-path apps/iphone -j 2
CARGO_BUILD_JOBS=2 cargo build -p fabric-mac
```

Set `FABRIC_TEST_ASSETS` to the absolute `assets/` directory for both test commands
to include real encoder/adapter parity tests. `swift test` runs on the real Mac;
it is not an iPhone hardware benchmark.

## Run the Mac backend

The launcher locates the prepared models and ONNX Runtime automatically, builds
the Rust backend, and opens the Mac application:

```sh
fabric-memory run
```

The UI delegates storage, embeddings, translation and search to Rust. It shows
the 200 most recent live memories, and semantic search considers the entire index.
Import accepts TestFlight result JSON, event arrays, or NDJSON ledgers. Export
writes `outgoing.jsonl` to the app's exchange folder and selects it in Finder.
Forget creates a synchronized tombstone; it does not erase immutable history.

Model files and the native inference library remain in the prepared workspace;
self-contained distribution of the Mac app is not part of this development launcher.
Advanced overrides: `FABRIC_ASSET_DIR`, `ORT_DYLIB_PATH` and `FABRIC_DATA_DIR`.
The core CLI and its stress runner remain independently usable.

## Cloud key

Copy `apps/mac/.env.example` to `apps/mac/.env`, add `OPENAI_API_KEY` and your
chosen `OPENAI_MODEL`. The Mac launcher automatically points the Rust backend at
that file at runtime. Do not bake it into the frontend, resources,
or a distributable binary. Change `FABRIC_CLOUD_ENABLED` to `true` only when you
want to use the cloud adapter. Cloud mode and Auto routing may then invoke it when you send a prompt.

Cloud requests use only the caller-supplied selected context, have a bounded
output, and set `store:false`. That flag is not a promise of zero provider-side
retention. Local memory and model inference otherwise stay on the devices.

## Model and evaluation limits

- Mac text generation uses Liquid AI LFM3-1.7B Q4_K_M under the LFM Open License
  v1.0, which has commercial-use restrictions. See [LOCAL-LLM.md](docs/LOCAL-LLM.md).

- MiniLM: 384 dimensions. MPNet: 768 dimensions. Both model repositories declare
  Apache-2.0. The deployed model files, tokenizers, revisions, pooling and 256-token
  limit are fingerprinted. These are English short-passage encoders.
- Training runs on the Mac, temporarily loading both encoder architectures in
  sequence. The phone receives only MiniLM and the down-projection adapter.
- Adapters are approximately 1.125 MiB of FP32 weights per direction, plus centering
  metadata. Expansion cannot recover information absent from the source embedding.
- The default experiment uses 2,000 WikiText training paragraphs, 128 validation
  paragraphs, and 64 held-out test paragraphs with 32 first-sentence queries.
  Hyperparameters use validation only. Test passages and queries never train the
  adapter. This is a controlled retrieval experiment, not a general RAG benchmark;
  shared pretraining data between the encoders is outside this experiment's control.
- The vector index is intentionally an exact brute-force baseline using SQLite
  records. Search is O(N × dimensions), useful for exposing the scaling limit.
- The phone journal is **not bounded or compacted yet**. It keeps all events in
  this milestone. No pending changes are silently discarded to meet a budget.
- Live sync is local-network HTTPS with certificate pinning and a private Git
  host. Continuous iOS background sync, application-encrypted archives, SLM
  summarization and autonomous task handoff remain future work. Both devices have
  local LFM3 generation; the Mac additionally supports optional GPT answering.

See [PROTOCOL.md](protocol/PROTOCOL.md), [LICENSES.md](docs/LICENSES.md), and
[reports](reports/) for the current evidence.
