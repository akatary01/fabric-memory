# Mac answering and web retrieval

Run `fabric-memory run`. The launcher prepares the existing local LFM3 runtime and a
private SearXNG installation once, then builds and opens the Tauri app.

## Answer mode

- **Local:** always generates on the Mac. A model error is reported without cloud fallback.
- **Cloud:** uses the configured provider for answers and research. Device, Blender, workspace/code and MCP action planning always stay local; this overrides Cloud mode.
- **Auto:** asks the local LFM3 model for a structured task classification. Simple
  and standard tasks stay local, including ordinary searches, writing and brief
  summaries. Complex non-action reasoning can select cloud. If classification fails, bounded keyword rules apply. Without cloud
  configuration, Auto uses local. Cloud fallback after a local generation error is
  limited to complex/expert tasks. Search freshness alone never selects cloud. Each answer displays the actual provider, model, and routing reason.

This is a capability heuristic, not a calibrated correctness or confidence detector.
Use manual mode when you need a fixed provider.

For cloud, put `OPENAI_API_KEY`, `OPENAI_MODEL` (default `gpt-4.1-mini`), and
`FABRIC_CLOUD_ENABLED=true` in `apps/mac/.env`, then quit and reopen with
`fabric-memory run`. The key stays in the Rust backend. Cloud receives the prompt,
up to two selected public memory passages, bounded recent conversation, and any
retrieved web context. Requests set `store:false`; provider retention policies
still apply. Auto can send this context to cloud when enabled.

## Web mode

- **Off:** no search or page fetching, regardless of answer mode.
- **On:** retrieve web evidence before answering.
- **Auto:** use local task classification to request web evidence for current
  information, explicit searches, research, or URLs. An explicit URL forces retrieval.

Local + Web Off supports fully local answering after setup. Local + Web On keeps
LLM inference on the Mac, while search engines and page hosts receive web requests.
The search query is derived directly from your question, so avoid including private
material in a question when Web On or Auto can search.

SearXNG is self-hosted on `127.0.0.1:8877`; it aggregates public engines without a
paid API key. Its source is pinned and hash checked by `tools/web_search.py`.
The service and private Python environment live in
`~/Library/Application Support/local.memoryfabric.mac/WebSearch`.
`python3 tools/web_search.py` repairs/starts it independently. The launch agent is
`~/Library/LaunchAgents/local.memoryfabric.search.plist`.
Advanced users can set `FABRIC_SEARCH_URL` to another compatible search endpoint.

Each search uses up to three usable results and attempts to read two pages.
A direct public HTTP(S) URL in the prompt reads that page instead of searching
(up to two URLs). The reader follows a bounded number of redirects, validates
public destinations at every hop, limits responses to 2 MB, and extracts bounded
HTML/text content. It does not execute JavaScript or log into sites. Search
snippets are labeled when page extraction is unavailable. Returned sources appear
under the answer and open in your browser. The model is instructed to cite `[S1]`
identifiers; inspect the listed sources because generated citations can be imperfect.

Upstream engines may rate-limit, block requests, or require captchas. This free
service has no availability guarantee. Search/reader failures are shown explicitly;
the app does not present an ungrounded answer as a successful web lookup.

## Chat formatting

Assistant replies render Markdown locally, including fenced code, lists, links, and tables.
Model-supplied thinking fields and inline `<think>` / `<thinking>` blocks appear in a
collapsed **Thinking** disclosure, including older saved replies. The app does not
request hidden reasoning from cloud providers. Marked and DOMPurify are bundled
locally with pinned versions and license notices; raw scripts, remote images,
forms, and embedded content are excluded.

## Verification

Rust tests cover manual routing boundaries, missing cloud configuration, cloud
response parsing, private-address rejection, URL extraction, and HTML script removal.
Live checks use the actual Mac LFM3 model, SearXNG, and public web pages. Tests
and stress runs do not make paid cloud requests. See `reports/mac-routing-web.json`
for the latest verification scope.

Live web retrieval supplies current information without retraining the local model.
The search integration and model can be updated independently.
