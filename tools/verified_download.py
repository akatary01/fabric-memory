"""Download pinned Mac service archives with system TLS verification."""
import hashlib
import subprocess


def archive_digest(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download_archive(url, archive, expected_sha256, name):
    # The system curl uses macOS trust independently of Python's CA bundle.
    partial = archive.with_suffix(".partial")
    try:
        subprocess.run([
            "/usr/bin/curl", "--fail", "--silent", "--show-error", "--location",
            "--proto", "=https", "--proto-redir", "=https", "--connect-timeout", "30",
            "--speed-limit", "1", "--speed-time", "60", "--output", str(partial), url,
        ], check=True)
        if archive_digest(partial) != expected_sha256:
            raise SystemExit(f"{name} archive checksum mismatch.")
        partial.replace(archive)
    except subprocess.CalledProcessError:
        raise SystemExit(f"Could not download {name}. Check the network and macOS certificate trust.") from None
    finally:
        partial.unlink(missing_ok=True)
