# Mac routing and user tools

Run `fabric-memory run` to start the local model and open the Mac app. The main agent always runs locally.

- **Local** keeps worker requests on this Mac.
- **Auto** can consult a cloud worker for complex, shareable requests.
- **Cloud** prefers cloud advice for eligible requests. Local action planning and tool results stay on the Mac.

Configure providers and tokens in Settings → Cloud. Add search, browser, and other integrations under Connections → MCP servers. If you want a reusable Python helper, ask the agent to create and test it in the selected workspace. Saved helpers run under the local workspace sandbox.

Assistant replies render Markdown locally. Expand Task activity for actions and results.
