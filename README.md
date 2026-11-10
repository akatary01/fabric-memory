# Memory Fabric

A local-first AI workspace for Mac and iPhone, with shared memory through your own Memory beacon.

## Features

- Separate conversations, local chat, and semantic memory search
- Coding agent with workspace edits, code execution, action review, and Stop
- MCP connections and built-in Mac computer use
- Configurable models, project agent instructions, and complexity-based cloud selection
- Agent-classified public and private memories
- Independent on-device iPhone chat
- Automatic memory sync, QR pairing, and one-click local beacon
- Inline HTML mockups and six paper themes
- Separate app, Memory beacon, and cleanup commands

## Mac download

Open the disk image, drag **Memory Fabric** to Applications, and launch it. Local models and web search are included. Requires macOS 14+; Intel and Apple Silicon supported.

[Build and sign an installer →](docs/DISTRIBUTION.md)

## Develop

Requires Python 3.10+, Rust, and Xcode Command Line Tools. iPhone development also requires Node.js and Xcode.

```sh
./bin/fabric-memory install
fabric-memory run
```

The first build compiles the native Mac app. On a fresh checkout, use `fabric-memory run --prepare` to download and prepare the models first.

For iPhone, run `fabric-memory run --phone`, then build or archive in Xcode.

## Memory beacon

```sh
fabric-memory launch-git --url https://akatary.com/fabric-git
```

Or choose **Devices & sync → Deploy local memory beacon** to host on your Mac while the app is open.

Run the command on your server behind HTTPS. It starts only the Memory beacon and prints a compact text QR in the terminal. Enter its URL in the apps, or scan the QR on your phone. The default cloud address is not deployed automatically.

A printable address QR identifies a future room hub; a current invitation is still required to join.

[Beacon setup →](docs/BEACON.md)

## Clean

```sh
fabric-memory clean --dry-run
fabric-memory clean
```

Moves unused build caches to Trash. Keeps source, models, archives, settings, and memories. Files in use are skipped.

## Configuration

Choose models, MCP servers, and privacy options in Settings and Connections. Settings → Cloud manages providers and model selection. Tokens use Keychain. The standalone Mac app includes web search. Local chat works without a beacon or cloud key.

[Agent setup →](docs/AGENT.md) · [Architecture and diagnostics →](docs/ARCHITECTURE.md)
