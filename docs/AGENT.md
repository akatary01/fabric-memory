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

The agent reviews MCP calls before execution. Local memory reads run directly. Stop cancels pending reviews, sends cancellation notifications to active MCP sessions, and prevents further steps. An external action already dispatched may finish.

## Built-in web and computer access

Choose Web in the composer to control public research. Computer use is in Connections. The selected workspace, screen capture, and macOS permissions are in Access & local models. The agent uses the relevant guides for coding, visualization, computer actions, recording, and web research.

## User tools

Add integrations in Connections through MCP. To create a reusable Python helper, ask the agent to build and test one in the selected workspace. Helpers live in `fabric-tools/`. You can also put your own Python scripts there.

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

All action planning stays local: computer use, Blender, workspace/code execution and configured MCP tools. Cloud selection and older sharing preferences cannot override this rule. Cloud models remain available for ordinary answers and research. Tool results and local-control conversation turns stay out of later cloud context.

## Automatic cloud models

Automatic selection is on by default. Fabric discovers the models available to your OpenAI account, filters for compatible text/reasoning models, and selects a capability tier from task complexity: simple, standard, complex, or expert. The pool refreshes daily or from Settings. Exact model and provider request counts are shown in the app. Turn automatic selection off to choose a pinned model from the dropdown. Discovery failure is reported rather than pretending unavailable models can be used. New model IDs are eligible without editing a whitelist. OpenAI is built in; additional Responses-compatible providers can be added. Provider-specific capability names vary, so review the selected model for unfamiliar catalogs.

Conversation context is shared across prompts within a chat: the recent transcript and conversation opening are retained within a bounded context budget. This is separate from optional retrieval of saved memories. Local-control continuations stay local; their file results and observations are excluded from cloud conversation context.

The live status shows only the current operation (selecting a model, reading a file, running a command, or writing the reply), with a small animated indicator. It does not display a thought process or a growing step list.

## When something fails

Failures remain in the conversation with the received error, the last completed operation when available, and a suggested next step. Use **Add details**, **Edit request**, **Open Settings**, or **Use Local** as appropriate. No action is automatically retried. Completed changes and action receipts remain available; continuing should inspect the current state before repeating work. Failure messages are private chat context and are not added as saved semantic memories. Workspace-sharing approval is scoped to the selected provider.


## Approval and computer planning

**Settings → General → Auto-approve all actions** lets code, workspace edits, computer controls, and configured MCP calls run without individual reviews. It is off by default. Stop still cancels the current task. macOS permissions remain required; cloud models cannot plan or direct local actions. Automatic approvals are recorded in Activity. Changing the setting can release an action already waiting for review.

Computer planning is always local. The cloud computer-planning switch has been removed and older opt-ins are ignored. Computer tasks still check actual macOS permissions before execution.

For Chromium-based apps the helper requests the fuller accessibility tree as an assistive client after macOS trust is granted, following [Chromium's accessibility documentation](https://www.chromium.org/developers/design-documents/accessibility/). Canvas-only or otherwise inaccessible app content still limits text-based computer control; screenshot visual grounding is not implemented.

Replies and code/HTML blocks have Copy buttons. Recorded command outputs can be copied from the reply's command receipt or Activity. The last command's actual exit status is shown separately from model prose. Final answer evidence prioritizes the newest results, so corrected failures do not displace a later successful run. App controls retain their observed target across approval dialogs instead of typing into whichever window became frontmost.



Connections now has a local-command/remote-URL setup form, optional credentials and a connection test. Tests do not save drafts or replace existing sessions. Saved connections can be edited, enabled/disabled, reconnected, removed and restored with Undo. Raw JSON remains under Advanced; interactive OAuth is not implemented.
