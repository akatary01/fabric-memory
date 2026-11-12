# Architecture

Memory Fabric is a native Mac app and an independent iPhone app. Public memories can sync through a Memory beacon; private memories stay on their originating device. The beacon is a sync service, not the model executor.

## Mac

`apps/mac/ui` contains the local Tauri WebView. `apps/mac/src-tauri/src/backend.rs` owns IPC, routing, conversations, the ledger and tool execution. API tokens stay in Keychain. Conversation state and semantic memory are separate; failed turns stay in the chat without being added as semantic memories.

`agent.rs` runs a bounded, observation-driven tool loop. The model chooses actions; Fabric reviews or auto-approves eligible actions, executes them, and records receipts. Stop cancels pending reviews and running code. Repeated errors or unchanged observations trigger a no-progress failure. The expandable status displays action summaries and results, not private internal reasoning.

`workspace.rs` and `tools/workspace.py` provide confined file operations and sandboxed execution. Root and scoped project instructions are loaded with a bounded context budget. `agent_docs.rs` embeds the shipped Markdown guides, which remain available without the source checkout. Code/file reviews apply locally; cloud delegation of actions is disabled.

`computer.rs` and `apps/mac/automation/ComputerUse.swift` implement Fabric-owned app controls and accessibility observations. macOS Accessibility is required; Screen Recording is needed for captures. Browser navigation is separate from background web search. Local planning is mandatory. Cloud models cannot receive action tools or direct local controls. Screenshots stay local; visual grounding from screenshot pixels is not implemented.

`mcp.rs` and `tools/mcp_bridge.py` handle configured stdio and HTTP/SSE MCP servers. Remote arguments remain subject to privacy checks. `web.rs` and the bundled SearXNG service provide web search and page reading; compatible cloud models can also use provider web search.

`generation.rs`, `routing.rs`, `cloud.rs`, and `cloud_models.rs` keep the main agent local. It can answer directly or ask installed local models and provider-discovered cloud models for advice through `delegation.rs`. Local mode keeps workers on the Mac. Auto offers cloud advice for complex requests, and Cloud offers it for eligible requests of any complexity. Cloud workers receive only the current shareable user request and have no tools. Actual local/cloud generation requests and estimated priced usage are tracked by `usage.rs`. Unknown model/provider costs are not invented.

The standalone universal app bundles the native executable/controller, per-architecture Python and ONNX Runtime, Ollama, the default local model, Git, search service, assets, licenses and agent guides. User data lives outside the signed bundle. Developer ID signing and notarization are separate distribution steps.

## Memory and sync

`core` contains the Rust ledger, encoders and learned translation adapters. Mac memory embeddings use MPNet; the phone uses MiniLM. Destination-native search can use translated vectors without re-embedding imported documents. Immutable events, Lamport clocks, LWW resolution, tombstones and atomic ingestion support convergent sync.

`beacon.rs`, `sync.rs`, and `local_beacon.rs` connect devices to a Memory beacon. `tools/local_beacon.py` and `tools/sync` host the HTTPS/Git service. QR invitations pair devices; a locally deployed beacon runs while its owning Mac app is open. Private notes are excluded from sync. Transport is encrypted; end-to-end encryption at rest is not implemented.

Device usage is separate from the memory ledger. Updated clients submit their most recent system CPU, estimated memory, and storage-volume readings through their authenticated beacon connection. The beacon binds readings to the credential and timestamps receipt. The device directory shows the latest reading while the beacon is running. Older clients and beacons may show no remote readings. Screen previews and observed controls stay on the Mac.

## iPhone

`apps/iphone` contains the native Swift app, local llama.cpp generation, MiniLM encoder, ledger and sync client. It can chat locally and pair/sync independently. Mac cloud/MCP/computer-control features do not imply phone parity.

## Guides

[Agent behavior and setup](AGENT.md) · [Coding contract](CODING-AGENT.md) · [Beacon](BEACON.md) · [Distribution](DISTRIBUTION.md)

Historical benchmark and first-implementation notes are retained in `history/IMPLEMENTATION-NOTES.md`; they do not define the current app workflow.
