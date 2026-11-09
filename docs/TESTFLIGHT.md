# TestFlight build 3 handoff

The archive is prepared, not uploaded. It is signed as an Xcode development
archive; Organizer applies distribution signing during upload. No App Store
Connect validation or physical iPhone execution is claimed before your upload.

1. Open the included `.xcarchive` in Xcode Organizer.
2. Choose Distribute App → App Store Connect. Select the team and the app record
   matching bundle ID `local.memoryfabric.phone`, then complete upload.
3. Install build 3 through TestFlight and open Memory Fabric.
4. Type a prompt and tap Send. LFM3 runs on the iPhone using MiniLM-retrieved
   memory. Pairing, the Mac server and a GPT key are not required for chat.
5. For memory sync, connect the Mac and phone to a private Wi-Fi network or
   Personal Hotspot. Run `npm run mac`, click Pair iPhone, transfer the JSON to
   the phone, and select it using Connect Mac. Then tap Sync. Import a fresh
   pairing file after changing networks. Build 3 supports numeric IPv6 URLs.
6. Diagnostics runs the separate real MiniLM parity, learned adapter retrieval
   and 1,000 durable-write checks. Keep it foregrounded. Share the results JSON
   to the Mac and run `python tools/import_phone.py /path/to/result.json` using
   the prepared Python environment to complete reverse retrieval validation.

Build 3 adds approximately 1.1 GB of bundled LFM3 weights and the native llama.cpp
runtime. It generates on-device without calling a cloud model. The phone loads
MiniLM for embedding; it does not load MPNet or receive a GPT key.
Pairing credentials and private TLS keys are not bundled.

See [PHONE-LOCAL-LLM.md](PHONE-LOCAL-LLM.md) and [SYNC.md](SYNC.md).
LFM3 uses the LFM Open License v1.0, including commercial-use restrictions.
llama.cpp and ONNX Runtime are MIT licensed; the embedding models are Apache-2.0.
Public diagnostic text: WikiText / Wikipedia contributors, CC-BY-SA-3.0.
Attribution and model/runtime licenses are included in DemoAssets.
