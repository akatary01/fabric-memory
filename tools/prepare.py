#!/usr/bin/env python3
"""Prepare the Mac memory encoder."""
import hashlib
import json
import os
import ssl
import certifi
from pathlib import Path
from urllib.request import urlopen
from huggingface_hub import hf_hub_download

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets"
TLS = ssl.create_default_context(cafile=certifi.where())
PIPELINE = "attention-mask-mean-pool+l2;float32;max-tokens=256;v1"

def sha(data): return hashlib.sha256(data).hexdigest()
def dump(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")

def encoder(name, model, dimension):
    folder = ASSETS / name
    if (folder / "encoder.json").exists():
        manifest = json.loads((folder / "encoder.json").read_text())
        assert all(sha((folder / f).read_bytes()) == h for f,h in manifest["files"].items())
        return manifest
    revision = json.load(urlopen(f"https://huggingface.co/api/models/{model}", context=TLS, timeout=30))["sha"]
    folder.mkdir(parents=True, exist_ok=True)
    hashes = {}
    for remote, local in [("onnx/model.onnx", "model.onnx"), ("tokenizer.json", "tokenizer.json")]:
        path = Path(hf_hub_download(model, remote, revision=revision, local_dir=folder))
        if path.name != local or path.parent != folder:
            if (folder / local).exists(): (folder / local).unlink()
            os.link(path,folder / local)
        hashes[local] = sha((folder / local).read_bytes())
    # These model repositories declare Apache-2.0 in their cards but do not
    # consistently ship a standalone LICENSE file. Preserve both the declaration
    # at its pinned revision and the standard license text.
    hf_hub_download(model, "README.md", revision=revision, local_dir=folder)
    (folder / "LICENSE").write_bytes(urlopen("https://www.apache.org/licenses/LICENSE-2.0.txt", context=TLS, timeout=30).read())
    identity = {"model":model,"revision":revision,"dimension":dimension,"max_tokens":256,"files":hashes,"pipeline":PIPELINE}
    manifest = {"version":1, **identity, "space":sha(json.dumps(identity,sort_keys=True,separators=(",",":")).encode())}
    dump(folder / "encoder.json",manifest)
    print(f"Prepared {model}: {dimension} dimensions", flush=True)
    return manifest

if __name__ == "__main__":
    encoder("mac", "sentence-transformers/all-mpnet-base-v2", 768)
