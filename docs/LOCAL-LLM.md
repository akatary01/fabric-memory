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

The default generator is **LFM3-1.7B**, Q4_K_M (about 1.1 GB of model weights),
with a 4,096-token context, a 512-token output cap, and thinking disabled. This is
a small starting model for the 8 GB Mac Air. Actual quality and memory use vary
by prompt and other running apps; this is not a benchmark of larger models.
The model unloads after 60 idle seconds. One model and one generation slot are
configured to bound memory use. Inference uses Ollama's native Apple Silicon
runtime. MPNet remains the separate embedding model.

Set `FABRIC_LOCAL_MODEL=lfm3:1.7b-q4_K_M` in `apps/mac/.env` to select another installed
local model, then run `fabric-memory run`. Choosing a larger model is a deliberate
hardware tradeoff. The phone has its own bundled local generator; see PHONE-LOCAL-LLM.md.

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
[LFM3-1.7B](https://ollama.com/library/lfm3:1.7b-q4_K_M), LFM Open License v1.0. Ollama verifies
downloaded model blobs; its resolved model digest is recorded in `LocalLLM/model.json`.

LFM3 uses Liquid AI’s [LFM Open License v1.0](https://huggingface.co/LiquidAI/LFM3-1.7B/blob/main/LICENSE), which includes commercial-use restrictions. It is an open-weight model, not an Apache/MIT-licensed dependency. This explicitly selected model is an exception to the earlier all-open-source dependency requirement.
