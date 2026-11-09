#!/usr/bin/env python3
"""Stage the pinned official Liquid AI model for on-device iPhone generation."""
import hashlib
import json
import subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
REVISION="cd0c74c575b5c416f3011b5b4c298077f26ca8c2"
SHA256="3793e1e6e2266919d6e015287221b41100175a98eddaba92e111aed3b05c93c4"
SIZE=1099627808

def digest(path):
    value=hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda:source.read(8*1024*1024),b""):value.update(chunk)
    return value.hexdigest()

def main():
    folder=ROOT/"apps/iphone/DemoAssets/generation";folder.mkdir(parents=True,exist_ok=True)
    model=folder/"model.gguf"
    if not model.exists() or model.stat().st_size!=SIZE or digest(model)!=SHA256:
        cached=Path.home()/"Library/Application Support/local.memoryfabric.mac/LocalLLM/models/blobs"/("sha256-"+SHA256)
        if cached.exists() and cached.stat().st_size==SIZE and digest(cached)==SHA256:
            print("Reusing the verified LFM3 model for the iPhone…",flush=True)
            # APFS clone avoids duplicating the 1.1 GB weights where supported.
            result=subprocess.run(["cp","-c",str(cached),str(model)],capture_output=True)
            if result.returncode:
                import shutil
                shutil.copyfile(cached,model)
        else:
            print("Downloading the pinned iPhone LFM3 model (1.1 GB)…",flush=True)
            url="https://huggingface.co/LiquidAI/LFM3-1.7B-GGUF/resolve/"+REVISION+"/LFM3-1.7B-Q4_K_M.gguf"
            subprocess.run(["curl","--http1.1","-fL","--retry","3","-o",str(model),url],check=True)
        if model.stat().st_size!=SIZE or digest(model)!=SHA256:raise RuntimeError("iPhone LFM3 checksum mismatch")
    license=folder/"LICENSE.txt"
    if not license.exists():
        subprocess.run(["curl","--http1.1","-fsSL","--retry","3","-o",str(license),
            "https://huggingface.co/LiquidAI/LFM3-1.7B-GGUF/raw/"+REVISION+"/LICENSE"],check=True)
    runtime_license=folder/"LLAMA-LICENSE.txt"
    if not runtime_license.exists():
        subprocess.run(["curl","--http1.1","-fsSL","--retry","3","-o",str(runtime_license),
            "https://raw.githubusercontent.com/ggml-org/llama.cpp/b12211/LICENSE"],check=True)
    manifest={"model":"LFM3-1.7B-Q4_K_M","filename":"model.gguf","sha256":SHA256,"bytes":SIZE,
        "repository":"LiquidAI/LFM3-1.7B-GGUF","revision":REVISION,"runtime":"llama.cpp b12211","license":"LFM Open License v1.0"}
    (folder/"generation.json").write_text(json.dumps(manifest,indent=2)+"\n")
    print("On-device iPhone LFM3 model prepared",flush=True)

if __name__=="__main__":main()
