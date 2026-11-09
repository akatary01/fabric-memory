# iPhone local chat (build 3)

Open Memory Fabric, type a prompt and tap Send. The app bundles Liquid AI
LFM3-1.7B Q4_K_M and runs it through llama.cpp on the iPhone GPU. No pairing,
Mac server, API key or model download is required to generate a reply after
installation. MiniLM retrieves local and translated memories for context.
Questions, answers and conversation history persist on the phone.

Connect Mac and Sync are separate actions for exchanging memory events. Chat
does not perform a network request. Keep the app foregrounded during inference;
leaving it cancels generation and releases its model context. The local generator
uses a 4,096-token context, two CPU support threads and at most 384 output tokens.
The first reply loads the model; later replies reuse it while foregrounded.

The model adds approximately 1.1 GB to the app. TestFlight build 3 also supports
direct numeric IPv6 pairing for IPv6-only Personal Hotspots. Import the newest
Pair iPhone JSON after changing networks.

Run npm run ios to prepare any missing assets and open the existing Xcode project.
The launcher restores missing staged links from canonical assets after an iCloud
move and preserves an existing Python environment rather than clearing it.

Model: LiquidAI/LFM3-1.7B-GGUF revision
cd0c74c575b5c416f3011b5b4c298077f26ca8c2, file SHA-256
3793e1e6e2266919d6e015287221b41100175a98eddaba92e111aed3b05c93c4.
The preparation script reuses the identical verified Mac model when available.
Runtime: official llama.cpp b12211 XCFramework; its release SHA-256 is pinned
in Package.swift. Runtime and model licenses ship with the app resources.
LFM3 has the LFM Open License v1.0, including commercial-use restrictions;
llama.cpp is MIT licensed. The app source, core and UIKit screen remain separate.

Native Swift generation and phone-session tests run on the real Mac. Physical
iPhone inference and memory/thermal behavior require installing this build
through TestFlight; no simulator or direct device installation is used.
