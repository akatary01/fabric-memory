# Memory beacon

The beacon is a standalone Python/Git service. It stores the private append-only memory ledger and device identities. It does not run a model, search the web, or start the apps.

## Start on a server

Install Python 3.10+ and Git, check out this repository, then run:

```sh
./bin/fabric-memory install
fabric-memory launch-git --url https://akatary.com/fabric-git
```

The default listener is `127.0.0.1:8766`. Put it behind an HTTPS reverse proxy that preserves the `/fabric-git` prefix. For example, in an existing TLS-enabled nginx virtual host:

```nginx
location /fabric-git/ {
    proxy_pass http://127.0.0.1:8766;
    proxy_set_header Host $host;
    client_max_body_size 32m;
    proxy_read_timeout 120s;
}
```

Clients require valid HTTPS. For direct TLS instead of a reverse proxy, supply `--bind 0.0.0.0 --tls-cert /path/fullchain.pem --tls-key /path/privkey.pem`. Use a certificate trusted by the devices. No TLS verification bypass is enabled.

`--data-dir` selects the private persistent storage directory. Back it up as a unit, including the repository, device database, and beacon configuration. The owner must protect this directory. Memory events on the beacon are authenticated and protected in transit, but are not end-to-end encrypted at rest in this build.

## Join

Startup prints a compact, character-based QR directly in the terminal, followed by the invitation code and pairing link. No picture file or browser window is needed.

Use `fabric-memory launch-git --invite` on the beacon host to refresh an expired QR without restarting the service. Supply the same `--data-dir` if you selected custom storage.

1. In the Mac app, open **Devices & beacon**. Enter the URL, invitation code, and device name. Review the address and join.
2. Each invitation expires in five minutes and can be redeemed once. After the first device joins, it can create another QR from the Mac app. Shared access is to this beacon's entire memory ledger.

The Git repository is available at `<beacon URL>/fabric.git` with the owner bearer credential. Clients use the bounded event API over the same repository. Git pushes are checked by the append-only validation hook.

For a future physical hub, show its HTTPS address QR permanently and issue temporary join invitations while pairing is enabled. Scanning a static address QR does not grant access or change Wi-Fi networks. Local chat continues while a beacon is offline.

## Device management

The private `beacon-config.json` holds the owner token. Do not publish it or the pairing page. An owner can list devices with authenticated `GET /v1/devices` and revoke a device with authenticated `POST /v1/revoke` and `{"device_id":"…"}`. Already-synced copies remain on a disconnected device. Device credentials are stored in a mode-0600 private desktop settings file.

The Mac app runs in a native window and talks to its Rust backend over native IPC. `fabric-memory run` never starts this beacon or changes your existing server deployment.

## Local beacon button

Choose **Devices & sync → Deploy local memory beacon**. The standalone Mac app includes Git and Python, creates a private TLS identity and Git ledger, starts a LAN service, and connects this Mac. Choose **Add device** for a single-use, five-minute invitation QR. Use the invitation on a device on the same network. The QR includes the beacon's public certificate trust anchor. No system certificate installation is needed. Private memories stay on their device.

The service runs while fabric-memory is open; **Stop** or quitting the app stops it. A previously running beacon resumes when the app opens again if this Mac is still connected to its own beacon. Pressing Stop keeps it stopped until Deploy is chosen again. Saved ledger, device credentials, certificate, and port remain in `LocalBeacon` inside the app data folder for the next deployment. The Mac must be awake and on the network. If its IP address changes, deploy again and re-pair devices with the new address. This does not deploy the akatary cloud server or change firewall settings.
