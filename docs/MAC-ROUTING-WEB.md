# Mac routing and user tools

Run `fabric-memory run` to start the local model and open the Mac app. The main agent always runs locally.

- **Local** keeps worker requests on this Mac.
- **Auto** can consult a cloud worker for complex, shareable requests.
- **Cloud** prefers cloud advice for eligible requests. Local action planning and tool results stay on the Mac.

Configure providers and tokens in Settings → Cloud. Public research runs automatically when requested. A small local model prepares the query, and the web reader retrieves public pages through SearXNG. Set `FABRIC_WEB_MODEL` to another installed local model or `FABRIC_SEARCH_URL` to a compatible SearXNG endpoint.

Enable Computer use in Connections and grant macOS Accessibility to open and control apps. Enable screen capture in Access & local models and grant Screen Recording to record video, with optional microphone or system audio. Recordings are saved in the selected workspace. Add other integrations under Connections → MCP servers. If you want a reusable Python helper, ask the agent to create and test it in the selected workspace. Saved helpers run under the local workspace sandbox.

Assistant replies render Markdown locally. Expand Task activity for actions and results.
