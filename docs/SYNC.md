# Private Mac Git hosting and phone prompts

The host runs on this Mac, under your user account, not a hosted Git provider.
`fabric-memory launch-git` installs/reuses it; `fabric-memory run` also ensures it is running.
It starts again when you log in. The Mac must be awake and reachable on the same
local network as the iPhone. No router port forwarding or Remote Login is enabled.

## Use

1. Run `fabric-memory run`.
2. Click **Pair iPhone**. AirDrop the selected private pairing JSON to your iPhone.
3. Install the updated phone build through TestFlight. Tap **Connect Mac**, select
   that JSON from Files, and allow local-network access when iOS asks.
4. Memories sync automatically after pairing while both apps are running. **Sync**
   remains available for a manual refresh. **Send** retrieves memory
   and runs LFM3 on the phone; pairing is not required for chatting.

Put `OPENAI_API_KEY` in `apps/mac/.env`. `OPENAI_MODEL` defaults to `gpt-4.1-mini`
and can be changed. Re-run `fabric-memory run` or `fabric-memory launch-git` after editing
the `.env` file to refresh the private service settings. `FABRIC_CLOUD_ENABLED=false` disables cloud prompts. The key
never goes to the phone, Git, pairing file, or WebView. Existing `.env` files are
not modified. If you previously set cloud execution to false, change it to true
when you want to send prompts. Merely syncing never calls an LLM.

Normal phone launch opens a conversation view. The old benchmark is available
through **Diagnostics**. Chat is saved locally and its memory events are synced.
The phone uses no cloud embedding endpoint. The GPT call uses `store:false`, which
does not promise zero provider retention. No paid API call is performed by tests.

## Storage and transport

Runtime data is in `~/Library/Application Support/local.memoryfabric.mac/Sync/`:

- `repositories/fabric.git`: actual bare Git event repository, with linear append-only history.
- `server.pem` / `server-key.pem`: private host TLS identity.
- `ca.pem` / `ca-key.pem`: local certificate authority; the public root is included in pairing.
- `config.json`: local service configuration and pairing credential, mode 0600.
- `cloud.env`: private copy of Mac backend settings, mode 0600.
- `Pair iPhone.json`: credential and public certificate to import on the phone.
- `runtime/host.py`: local service code, copied outside iCloud-managed Documents.

The login service is `~/Library/LaunchAgents/local.memoryfabric.sync.plist`.
The pairing file uses the Mac’s current LAN IPv4 or IPv6 address on port 8765,
never localhost. The Mac’s `.local` hostname remains in the server certificate.
Re-run `fabric-memory run` or `fabric-memory launch-git` after changing networks, and import
the newly generated pairing file if the address or certificate changed.
The Mac’s own client deliberately uses loopback; that address is not sent to the phone. All routes require
the pairing credential. The phone pins the exact certificate and still checks
TLS hostname and validity. Credentials are stored in the phone's Keychain.
Treat the pairing JSON as a password: it grants access to the private memory repo.

Two interfaces share the same repository:

- `/fabric.git/…` is Git's actual authenticated smart-HTTP transport. Standard Git
  clients can clone, fetch and push using the certificate and bearer credential.
  A pre-receive hook rejects deletion, rewriting, merges, invalid events and
  actor/clock collisions. The API's Git updates use atomic ref updates.
- `/v1/events` is a paginated HTTPS event exchange for the phone and Mac cores.
  The phone does not clone Git or keep an unlimited object database. Every new
  accepted batch is committed to the same Git repository before acknowledgement.
  Its SQLite journal retains events across network failures, and duplicates are
  safe. Each receiving device translates vectors into its own embedding space.
- `/v1/prompt` forwards only the explicit prompt, recent conversation and selected
  retrieved context to OpenAI, using the Mac's key.

The Mac starts a sync on launch, wakes its worker after saving a note, answering a
prompt, forgetting a memory, or importing events, and polls every ten seconds for
incoming events and retries. A single worker coalesces writes made during a transfer.

The updated iPhone source starts automatic sync on launch/return to foreground,
after pairing, and after a locally persisted prompt/answer (including a question
whose generation failed). It also polls every ten seconds while active to receive
new Mac events and retry failed connections. Manual and automatic exchanges share
one in-flight transfer; writes during a transfer request a follow-up pass. Sync
errors leave local data intact and do not prevent local chat. Both apps display
automatic sync status; manual Sync remains available.

This is immediate outbound triggering plus periodic incoming polling, not server
push. Healthy-network propagation to the other active app normally takes up to a
poll interval plus transfer time. The Mac must be awake and the phone must reach
its paired address. The phone stops polling when inactive; iOS suspension prevents
guaranteed continuous background sync. It catches up on return to foreground.
Install a newly built iPhone app to obtain this behavior; older TestFlight builds
do not gain it automatically. Phone generation stays local and needs no Mac connection.

The repo and SQLite files are plaintext in OS-protected local storage. TLS protects
transport; application-level encrypted archives, per-device credential rotation,
and journal compaction remain future work. The current server validates event
sets in memory; very large histories will need a derived disk index and delta
push optimization. The existing stress harness can expose that scaling limit.

## Manage

```sh
fabric-memory launch-git
launchctl print gui/$(id -u)/local.memoryfabric.sync
```

No credential values should be pasted into shell commands or committed to source.
Stopping the login service: `launchctl bootout gui/$(id -u)/local.memoryfabric.sync`.

## IPv6-only Personal Hotspots

Use iOS build 3 or later. Some cellular hotspots assign the Mac only an IPv6
LAN address plus a synthetic IPv4 translation address. Setup selects the stable
IPv6 address, includes it in the pinned TLS certificate, and writes a bracketed
numeric HTTPS URL into the pairing JSON. The service supports IPv4 and IPv6;
non-private IPv6 clients are restricted to the advertised LAN /64 and must still
authenticate. Reimport the latest pairing JSON after changing networks.
Build 2 can use IPv4 pairing but does not accept numeric IPv6 pairing addresses.

The legacy /v1/prompt cloud endpoint remains available to explicitly configured
clients but is not called by the build 3 phone conversation screen.
