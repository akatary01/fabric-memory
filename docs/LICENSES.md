# Dependency and data licensing

Project source is MIT licensed. Dependencies must be open source. Open-weight
models with restrictive custom terms are not an automatic substitute.
The user specifically selected Liquid AI LFM3 for Mac generation; its custom
license is recorded below as an explicit model choice, separate from the
open-source runtime and embedding models.

| Component | License | Source |
|---|---|---|
| SearXNG | AGPL-3.0-or-later | https://github.com/searxng/searxng |
| scraper (HTML extraction) | ISC | https://github.com/rust-scraper/scraper |
| reqwest (HTTP client) | MIT OR Apache-2.0 | https://github.com/seanmonstar/reqwest |
| Marked (Markdown renderer) | MIT | https://github.com/markedjs/marked |
| DOMPurify (HTML sanitizer) | Apache-2.0 OR MPL-2.0 | https://github.com/cure53/DOMPurify |
| Tauri | MIT OR Apache-2.0 | https://github.com/tauri-apps/tauri |
| Ollama runtime | MIT | https://github.com/ollama/ollama |
| LFM3-1.7B generation model | LFM Open License v1.0; commercial-use restrictions | https://huggingface.co/LiquidAI/LFM3-1.7B/blob/main/LICENSE |
| Rust | MIT OR Apache-2.0 | https://github.com/rust-lang/rust |
| Swift | Apache-2.0 with Runtime Library Exception | https://github.com/swiftlang/swift |
| SQLite | Public domain | https://www.sqlite.org/copyright.html |
| rusqlite | MIT | https://github.com/rusqlite/rusqlite |
| ONNX Runtime, including Swift bindings | MIT | https://github.com/microsoft/onnxruntime |
| ort Rust bindings | MIT OR Apache-2.0 | https://github.com/pykeio/ort |
| Hugging Face tokenizers | Apache-2.0 | https://github.com/huggingface/tokenizers |
| NumPy | BSD-3-Clause | https://github.com/numpy/numpy |
| Apache Arrow / PyArrow | Apache-2.0 | https://github.com/apache/arrow |
| all-mpnet-base-v2 | Apache-2.0 | https://huggingface.co/sentence-transformers/all-mpnet-base-v2 |
| WikiText / Wikipedia text | CC-BY-SA-3.0 / GFDL as declared by upstream | https://huggingface.co/datasets/Salesforce/wikitext |

Model revisions and hashes are recorded in each encoder manifest. Model cards
and the standard Apache license are preserved with downloaded assets.

WikiText was released by Stephen Merity, Caiming Xiong, James Bradbury and Richard
Socher (2016), "Pointer Sentinel Mixture Models". Its text comes from Wikipedia
contributors. Prepared corpus files normalize whitespace and select paragraphs;
this modified text remains under CC-BY-SA-3.0. Preserve upstream attribution and
licensing when distributing the corpus or phone job. The source dataset revision
is stored in `assets/corpus.json`. Dataset licensing is separate from code licensing.

The deliberately accepted exceptions are Apple's proprietary macOS/iOS platforms,
SDKs, frameworks, Xcode and signing infrastructure, plus the optional OpenAI cloud
service, and the explicitly requested LFM3 open-weight model under its custom
license. They are not misrepresented as open-source dependencies.

No proprietary model runner or paid vector database is required. Mac generation
uses the open-source Ollama runtime with the quantized LFM3 model. The resolved
model digest is recorded under the private LocalLLM runtime directory and in
the real-device generation verification report.
