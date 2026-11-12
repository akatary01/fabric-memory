# Memory sync

fabric-memory synchronizes public memories between devices through a Memory beacon. Private memories and chat history stay in the local profile. See [Memory beacon](BEACON.md) for deployment, pairing, and revocation.

Open **Devices & sync** on the Mac and connect to a beacon using a fresh invitation. For a Mac-hosted beacon, select **Deploy local memory beacon**. The Mac retries sync when the beacon is reachable.

The beacon exchanges immutable events over HTTPS. Devices with the same embedding space can sync directly. For older foreign embeddings, Mac can use a compatible saved translation adapter. The adapter is optional and is only needed when ingesting a different embedding space.
