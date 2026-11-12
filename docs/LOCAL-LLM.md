# Mac prompt answering

Run `fabric-memory run` and use **Ask this Mac → Send**. The first launch installs the
local runtime and downloads the model; later launches reuse them. No API key is
needed for Mac prompt answering. `python3 tools/local_llm.py` prepares the generator alone.

The separate Mac UI invokes Rust. Rust embeds the prompt with MPNet, retrieves
up to three memories (including compatible imported vectors), and sends that
context plus the recent conversation to Ollama on `127.0.0.1:11434`. The generated
answer appears in the conversation; the question and answer become local memory
events. Conversation history survives restarting the app. All text renders as
plain text. No cloud fallback is used by this action.

The Mac ships with **LFM3-VL**. Setup lists chat models in Fabric’s local model library. Choose the main model in **Settings → Access & local models**, or load another model in **Settings → Local models**. When useful, the main agent can delegate advisory work to other installed models. **Context → Agent Structure** shows which models actually ran and how the agents were nested.

Screen sharing is optional and requires macOS Screen Recording. Screen images go only to a local vision model. Accessibility and workspace access are separate settings. Under **Settings → Coding workspace → Clean workspace**, select old files and project folders to move to Trash.

Set `FABRIC_LOCAL_MODEL=lfm3-vl:latest` in `apps/mac/.env` for local development. The phone has its own bundled model; see PHONE-LOCAL-LLM.md.

Runtime and model data live outside iCloud Documents at:
`~/Library/Application Support/local.memoryfabric.mac/LocalLLM/`.
The managed service starts at login and binds to loopback. It disables Ollama
cloud features. If a local Ollama instance is already running, setup reuses it
without changing its settings. Mac conversation history lives alongside the
local SQLite ledger in `mac-conversation.json`.

The managed service can be stopped using:
`launchctl bootout gui/$(id -u)/local.memoryfabric.llm`.

Runtime: [Ollama 0.40.15](https://github.com/ollama/ollama/releases/tag/v0.40.15),
[MIT license](https://github.com/ollama/ollama/blob/main/LICENSE). The installer
checks the official release's pinned SHA-256 before extraction. Model:
[LFM3-VL](https://ollama.com/library/lfm3-vl), LFM Open License v1.0. Ollama verifies
downloaded model blobs; its resolved model digest is recorded in `LocalLLM/model.json`.

LFM3 uses Liquid AI’s [LFM Open License v1.0](https://huggingface.co/LiquidAI/LFM3-1.7B/blob/main/LICENSE), which includes commercial-use restrictions. It is an open-weight model, not an Apache/MIT-licensed dependency. This explicitly selected model is an exception to the earlier all-open-source dependency requirement.
