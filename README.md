# fabric-memory

A local-first AI workspace for Mac, with shared memory through your own Memory beacon.

## Features

- Separate conversations, local chat, and semantic memory search
- Coding agent with workspace edits, code execution, action review, and Stop
- User-installed MCP tools and reusable workspace Python helpers
- Configurable models, project agent instructions, and complexity-based cloud selection
- Agent-classified public and private memories
- Automatic memory sync, QR pairing, and one-click local beacon
- Inline HTML mockups and six paper themes
- Separate app, Memory beacon, and cleanup commands

## Mac download

Open the disk image, drag **fabric-memory** to Applications, and launch it. A local model is included. Requires macOS 14+; Intel and Apple Silicon supported.

[Build and sign an installer →](docs/DISTRIBUTION.md)

## Develop

Requires Python 3.10+, Rust, and Xcode Command Line Tools. Install uv to prepare local model assets.

```sh
./bin/fabric-memory install
fabric-memory run
```

The first build compiles the native Mac app. On a fresh checkout, use `fabric-memory run --prepare` to download and prepare the models first.

## Memory beacon

```sh
fabric-memory launch-git --url https://akatary.com/fabric-git
```

Or choose **Devices & sync → Deploy local memory beacon** to host on your Mac while the app is open.

Run the command on your server behind HTTPS. It starts only the Memory beacon and prints a compact text QR in the terminal. Enter its URL and invitation code in the Mac app. The default cloud address is not deployed automatically.

A printable address QR identifies a future room hub; a current invitation is still required to join.

[Beacon setup →](docs/BEACON.md)

## Clean

```sh
fabric-memory clean --dry-run
fabric-memory clean
```

Moves unused build caches to Trash. Keeps source, models, archives, settings, and memories. Files in use are skipped.

## Configuration

Choose models, MCP servers, and privacy options in Settings and Connections. Settings → Cloud manages providers and model selection. Tokens use Keychain. Add web, browser, or other integrations through your own tools. Local chat works without a beacon or cloud key.

[Agent setup →](docs/AGENT.md) · [Architecture and diagnostics →](docs/ARCHITECTURE.md)
