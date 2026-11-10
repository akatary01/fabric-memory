# Your Mac agent

Open **Connections** for MCP and computer use. Open **Settings** for the local model, routing/privacy and usage. Chat shows a quiet working indicator, action reviews, and **Stop**. Click the current status to expand or hide live action summaries and results. Private internal reasoning is not shown. Actual action records also remain in Activity. The local model chooses tools and when to finish, without a mandatory lookup sequence.

## Models

Type `load llama3.2:3b` (or another Ollama chat-model tag) in Settings. Download progress appears there. The current model stays selected until the new one is ready; the selection survives restart. Tool-capable models use native tool calls; other models use structured action plans. Model quality affects task reliability. Tasks are limited to 64 actions/rounds.

Cloud providers, API tokens and pinned models are configured in the separate **Settings → Cloud** tab. Choose a provider, or use **Add provider** with an HTTPS Responses API base URL. Additional providers must support Responses and model discovery; this does not implement native Anthropic or Gemini protocols. Each provider keeps its own Keychain token and model settings. The pinned-model dropdown is populated from that provider’s discovered models. Tokens entered there are stored in macOS Keychain, and the UI never reads them back. Existing private environment configuration remains supported.

## MCP

Configure server JSON from Connections. Both stdio and Streamable HTTP are supported, including JSON/SSE responses, session IDs, tool discovery/calls, resources, and prompts. Example shapes:

```json
{
  "mcpServers": {
    "local": {
      "command": "/absolute/path/to/your-mcp-server",
      "args": [],
      "env": {},
      "roots": ["/absolute/path/to/your/workspace"]
    },
    "remote": {
      "url": "https://your-server.example/mcp",
      "headers": {"Authorization": "Bearer YOUR_TOKEN"}
    }
  }
}
```

Configuration and credentials stay in a private local file. `roots` advertises workspace locations to the server; server permissions determine what it can access. Remote HTTP requires HTTPS, except localhost. Interactive OAuth, server sampling, elicitation, and resumable background SSE are not implemented. Authentication headers can be configured explicitly.

The agent reviews MCP calls before execution. Local memory reads and public web searches run directly. Stop cancels pending reviews, sends cancellation notifications to active MCP sessions, and prevents further steps. An external action already dispatched may finish.

## Computer use

Enable **Computer use** in Connections, then use **macOS access** to grant Accessibility and Screen Recording to Memory Fabric. Fabric Memory owns these tools and permissions; its local agent calls the bundled helper directly. The helper accepts installed app names and bundle IDs; `browser` and `web` resolve to the default browser for observation and control. It can open HTTP/HTTPS URLs in that browser, observe app accessibility trees, open apps, clicks elements/coordinates, types, sends keys, scrolls, and captures the screen. Secure text fields are redacted in observations. The agent reviews control actions; observations run directly after computer use is enabled. Screenshots remain local. This build plans from accessibility observations; screenshot-based visual grounding is not implemented.

## Memory recall

Recall runs quietly only when the local agent chooses it for earlier personal context, preferences, or decisions. It may search once per task and insert at most two relevant notes, 400 characters each / 800 total. Matches require cosine similarity ≥ 0.50 and must stay close to the best match; duplicate text, old assistant replies, and chat questions are excluded. There is no forced second lookup after the agent declines recall. General knowledge, arithmetic, and new facts skip lookup. Background recall is omitted from the task feed and replies; the assistant uses useful facts naturally rather than reporting search mechanics.

## Memory access

The local model classifies new notes and chat memories:

- **Private:** stays on the originating device.
- **Public:** available to devices paired to your Memory beacon; never published on the internet.

Uncertain classification defaults to Private. Existing shared events remain Public. Classifications can be imperfect; previously distributed copies cannot be recalled by changing future policy. Private context is kept out of automatic cloud answers and remote tool arguments. Older experimental restricted records stay private and do not sync. The server owner retains administrative access. Back up the beacon configuration, `devices.sqlite`, and Git repository together. HTTPS protects transport; end-to-end encryption at rest is not implemented.

## Conversations and usage

Use **+ New conversation** or the titlebar plus. Recent chats appear in the sidebar. Right-click a chat to rename or delete it (or double-click its small topbar title to rename); **Recently deleted** restores it. Shared memory is independent of chat deletion. Existing history migrates without altering the original history file.

The footer retains memory/event counts and adds local/cloud request counts and estimated savings, tracked from this version onward. The first launch includes known earlier cloud replies from saved chats; older failed requests and historical savings cannot be reconstructed. Local calls include routing, privacy detection, action planning, and answer requests; failed requests count too. Local-answer token counts are valued at the selected cloud model’s standard rates. They are an estimate across different tokenizers, not a billing statement, and exclude hardware, electricity, and tool charges. GPT-4.1 mini uses $0.40 input / $1.60 output per million tokens ($0.10 cached input), checked November 9, 2026 against the [official model pricing](https://developers.openai.com/api/docs/models/gpt-4.1-mini). Current OpenAI model families have verified rates; unknown models and custom-provider pricing remain unpriced.

Protocol references: [MCP transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools), [Ollama tool calling](https://docs.ollama.com/capabilities/tool-calling).

## HTML mockups

Ask for a mockup or prototype. The Mac chat renders fenced `html` replies as an inline preview with **View HTML / Show preview**. Existing saved HTML replies render too. Previews support HTML, inline CSS, SVG, and CSS-only interactions. Scripts, external resources, form submission, native-app access, and navigation outside the preview are blocked.

## Coding and execution

Choose a project folder in **Settings → Coding workspace**; new projects default to the app's private Workspace folder. The agent can list/read/write files, run shell commands, inspect stdout/stderr and exit codes, fix errors, and run tests again. Fabric owns the tools. Edits and execution require action review; **Stop** terminates a running command and its children. Execution is sandboxed, capped at 120 seconds per command, with bounded output and network off unless an approved command requests it. Python and Git are bundled; other development runtimes must be installed.

Complex tasks can use a cloud planner after you allow workspace contents and command output to reach the selected cloud provider. That permission is off by default, with a per-task review if needed. Computer observations and private memory lookups are excluded from the cloud coding loop. MCP actions can participate after a review explicitly authorizes sending that tool result to the selected provider. Tool results are data, not instructions. The agent decides how to inspect, edit, test, and iterate; it does not follow a fixed action script.

## Automatic cloud models

Automatic selection is on by default. Fabric discovers the models available to your OpenAI account, filters for compatible text/reasoning models, and selects a capability tier from task complexity: simple, standard, complex, or expert. The pool refreshes daily or from Settings. Exact model and provider request counts are shown in the app. Turn automatic selection off to choose a pinned model from the dropdown. Discovery failure is reported rather than pretending unavailable models can be used. New model IDs are eligible without editing a whitelist. OpenAI is built in; additional Responses-compatible providers can be added. Provider-specific capability names vary, so review the selected model for unfamiliar catalogs.

Conversation context is shared across prompts within a chat: the recent transcript and conversation opening are retained within a bounded context budget. This is separate from optional retrieval of saved memories. Cloud coding continuations can reuse context already shared from the same workspace after approval; unrelated private memory remains local.

The live status shows only the current operation (selecting a model, reading a file, running a command, or writing the reply), with a small animated indicator. It does not display a thought process or a growing step list.

## When something fails

Failures remain in the conversation with the received error, the last completed operation when available, and a suggested next step. Use **Add details**, **Edit request**, **Open Settings**, or **Use Local** as appropriate. No action is automatically retried. Completed changes and action receipts remain available; continuing should inspect the current state before repeating work. Failure messages are private chat context and are not added as saved semantic memories. Workspace-sharing approval is scoped to the selected provider.

Browser-control requests stay on the Mac and do not silently fall back to background search. An app lookup, failed action, or browser launch alone does not count as a verified browser search. A failure explains the actual error and can open Connections for computer permissions.

## Approval and computer planning

**Settings → General → Auto-approve all actions** lets code, workspace edits, computer controls, and configured MCP calls run without individual reviews. It is off by default. Stop still cancels the current task. macOS permissions remain required; sharing workspace data with a cloud model still needs its own permission. Automatic approvals are recorded in Activity. Changing the setting can release an action already waiting for review.

**Settings → Cloud → Computer planning** separately allows the selected provider to receive app text and control results, which can contain private onscreen information. It is off by default and scoped to that provider. Screenshots stay local. With permission and an available cloud provider, Auto can choose a more capable model to plan the actions; Fabric's own helper still executes them. Local mode keeps planning local. Computer tasks check Accessibility before execution and discard superseded UI paths. Window contents are observed before menus. Input waits for the target app to activate.

For Chromium-based apps the helper requests the fuller accessibility tree as an assistive client after macOS trust is granted, following [Chromium's accessibility documentation](https://www.chromium.org/developers/design-documents/accessibility/). Canvas-only or otherwise inaccessible app content still limits text-based computer control; screenshot visual grounding is not implemented.

Replies and code/HTML blocks have Copy buttons. Recorded command outputs can be copied from the reply's command receipt or Activity. The last command's actual exit status is shown separately from model prose. Final answer evidence prioritizes the newest results, so corrected failures do not displace a later successful run. App controls retain their observed target across approval dialogs instead of typing into whichever window became frontmost.
