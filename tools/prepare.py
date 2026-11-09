#!/usr/bin/env python3
"""Download ONLY the pinned public assets needed for the real-device experiment."""
import argparse
import hashlib
import json
import random
import os
from pathlib import Path
from urllib.request import urlopen
from huggingface_hub import hf_hub_download

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets"
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
    revision = json.load(urlopen(f"https://huggingface.co/api/models/{model}"))["sha"]
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
    (folder / "LICENSE").write_bytes(urlopen("https://www.apache.org/licenses/LICENSE-2.0.txt").read())
    identity = {"model":model,"revision":revision,"dimension":dimension,"max_tokens":256,"files":hashes,"pipeline":PIPELINE}
    manifest = {"version":1, **identity, "space":sha(json.dumps(identity,sort_keys=True,separators=(",",":")).encode())}
    dump(folder / "encoder.json",manifest)
    print(f"Prepared {model}: {dimension} dimensions", flush=True)
    return manifest

def corpus(train_count):
    import pyarrow.parquet as pq
    repo = "Salesforce/wikitext"
    revision = json.load(urlopen(f"https://huggingface.co/api/datasets/{repo}"))["sha"]
    seen = set(); splits = {}
    for split, count in [("train",train_count),("validation",128),("test",128)]:
        path = hf_hub_download(repo, f"wikitext-2-raw-v1/{split}-00000-of-00001.parquet", repo_type="dataset", revision=revision, local_dir=ASSETS / "wikitext")
        records = []
        for text in pq.read_table(path, columns=["text"])["text"].to_pylist():
            text = " ".join(text.split())
            if len(text) < 160 or text.startswith("="): continue
            fingerprint = sha(text.encode())
            if fingerprint in seen: continue
            seen.add(fingerprint)
            records.append(text)
        random.Random(20261108).shuffle(records)
        assert len(records) >= count
        splits[split] = records[:count]
    dump(ASSETS / "corpus.json", {"source":repo,"revision":revision,"license":"CC-BY-SA-3.0", "splits":splits})
    return splits

def main():
    parser = argparse.ArgumentParser(); parser.add_argument("--anchors",type=int,default=2000)
    args = parser.parse_args()
    assert 800 <= args.anchors <= 10000
    encoder("phone", "sentence-transformers/all-MiniLM-L6-v2",384)
    encoder("mac", "sentence-transformers/all-mpnet-base-v2",768)
    corpus(args.anchors)

if __name__ == "__main__": main()
