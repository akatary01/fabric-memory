# Mac downloads

Recipients open the DMG, drag **fabric-memory** to Applications, and launch it. No Rust, npm, system Python, source checkout, Ollama installation, cloud token, or Memory beacon is required for local chat. Cloud and beacon connections are optional settings.

Requires macOS 14 or newer. The app contains native Intel and Apple Silicon binaries, separate Python/ONNX runtimes, the default local language model and memory encoder, and the macOS permission helper. Allow several GB for the app and writable local models. Intel generation uses the CPU and is slower.

## Build

On a development Mac with models prepared, Rust, Xcode Command Line Tools, Python 3.12+, and uv installed:

```sh
fabric-memory package --output ./dist
```

This builds an **ad hoc signed test installer**. It is not notarized and is not ready for normal public downloading under Gatekeeper. The package includes no personal ledger, conversations, cloud tokens, beacon credentials, or development environment file. It seeds the default model into the recipient's private Application Support folder; the signed app remains immutable. The model service binds to a private loopback port and stops with the app.

## Public release

Install a valid **Developer ID Application** certificate (Apple Development certificates do not qualify). Store notarization credentials in a `notarytool` Keychain profile, then build:

```sh
fabric-memory package --output ./release \
  --identity 'Developer ID Application: Your Name (TEAMID)' \
  --notary-profile 'your-profile'
```

The command signs nested code and the app with hardened runtime, signs the DMG, submits it to Apple, staples the ticket, and validates it. An unsuccessful submission fails the command. Keep passwords out of shell history; use an existing Keychain profile. The output includes a SHA-256 checksum and a manifest recording signing/notarization status. Use a fresh output directory for each build.

Third-party stdio MCP servers may require their own executables/runtimes; the MCP protocol host is included.

References: [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution), [Ollama Mac requirements](https://docs.ollama.com/macos), [redistributable Python](https://github.com/astral-sh/python-build-standalone).
