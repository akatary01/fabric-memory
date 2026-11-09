#!/usr/bin/env python3
"""Prepare a self-contained TestFlight demo and the matching Mac evaluation context."""
import json
import argparse
import shutil
import subprocess
import time
import uuid
from pathlib import Path
from urllib.request import urlopen
import numpy as np
import onnxruntime
from prepare import ROOT, ASSETS, dump, sha
from metrics import score

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--skip-stress",action="store_true",help="Prepare app assets without running the separate stress benchmark")
    args=parser.parse_args()
    binary=ROOT/"target/release/fabric-core"
    assert binary.exists(),"cargo build --release -p fabric-core first"
    runtime=next((Path(onnxruntime.__file__).parent/"capi").glob("libonnxruntime.*.dylib"))
    run=ROOT/"runs"/("mac-"+uuid.uuid4().hex[:12]); run.mkdir(parents=True)
    def cli(db,*action):
        p=subprocess.run([str(v) for v in [binary,"--runtime",runtime,"--model",ASSETS/"mac","--db",run/db,*action]],capture_output=True,text=True)
        if p.returncode: raise RuntimeError(p.stderr)
        return p.stdout
    print("Encoding the held-out corpus with the real Mac Rust/MPNet pipeline",flush=True)
    start=time.perf_counter()
    cli("encoder.sqlite","encode",ASSETS/"mac-texts.json",run/"mac-vectors.json")
    seconds=time.perf_counter()-start
    vectors=json.loads((run/"mac-vectors.json").read_text())
    reference=np.array(json.loads((ASSETS/"mac-reference.json").read_text()),np.float32)
    error=float(np.max(np.abs(np.array([v["values"] for v in vectors])-reference)))
    assert error<0.0001,f"Rust MPNet differs from the training pipeline: {error}"
    job=json.loads((ASSETS/"job.json").read_text()); n=len(job["documents"])
    actor=str(uuid.uuid4())
    events=[{"version":1,"id":str(uuid.uuid4()),"actor":actor,"clock":i+1,"memory":f"eval-{i:04d}","deleted":False,"text":text,"embedding":vectors[i]} for i,text in enumerate(job["documents"])]
    native_file=run/"mac-events.jsonl"
    native_file.write_text("".join(json.dumps(e,ensure_ascii=False)+"\n" for e in events))
    dump(run/"queries.json",vectors[n:])
    cli("native.sqlite","import",native_file,"--adapter",ASSETS/"adapters/phone-to-mac")
    cli("native.sqlite","search-batch",run/"queries.json",run/"native-rankings.json")
    # This is an adapter validation on the Mac, not a claim of iPhone execution.
    phone_manifest=json.loads((ASSETS/"phone/encoder.json").read_text())
    phone_actor=str(uuid.uuid4())
    expected=[{**e,"id":str(uuid.uuid4()),"actor":phone_actor,"embedding":{"space":phone_manifest["space"],"values":job["reference_phone_vectors"][i]}} for i,e in enumerate(events)]
    expected_file=run/"reference-minilm-events.jsonl"
    expected_file.write_text("".join(json.dumps(e)+"\n" for e in expected))
    cli("adapter-validation.sqlite","import",expected_file,"--adapter",ASSETS/"adapters/phone-to-mac")
    cli("adapter-validation.sqlite","search-batch",run/"queries.json",run/"projected-rankings.json")
    metrics=score(json.loads((run/"native-rankings.json").read_text()),json.loads((run/"projected-rankings.json").read_text()))
    cli("native.sqlite","archive",run/"git-ledger")
    stresses=[]
    for count in ([] if args.skip_stress else [1000,10000]):
        print(f"Physical Mac storage/search stress: {count} events",flush=True)
        assert shutil.disk_usage(run).free>count*80000+1024**3
        cli(f"stress-{count}.sqlite","stress","--input",native_file,"--count",str(count),"--report",run/f"stress-{count}.json")
        stresses.append(json.loads((run/f"stress-{count}.json").read_text()))
    metadata={"schema_version":1,"mac_model_space":vectors[0]["space"],"phone_model_space":phone_manifest["space"],
        "job_sha256":sha((ASSETS/"job.json").read_bytes()),"mac_events_sha256":sha(native_file.read_bytes()),
        "down_adapter_sha256":sha((ASSETS/"adapters/mac-to-phone/adapter.json").read_bytes()),"dataset":"Salesforce/wikitext; CC-BY-SA-3.0"}
    demo_id=sha(json.dumps(metadata,sort_keys=True,separators=(",",":")).encode()); metadata["demo_id"]=demo_id
    context=ASSETS/"testflight"/demo_id; context.mkdir(parents=True)
    for source,name in [(native_file,"mac-events.jsonl"),(run/"queries.json","queries.json"),(run/"native-rankings.json","native-rankings.json")]: shutil.copy2(source,context/name)
    dump(context/"demo.json",metadata)
    bundle=ROOT/"apps/iphone/DemoAssets"
    if bundle.exists(): shutil.rmtree(bundle)
    shutil.copytree(ASSETS/"phone-bundle",bundle)
    shutil.copy2(native_file,bundle/"incoming.jsonl"); dump(bundle/"demo.json",metadata)
    (bundle/"ATTRIBUTION.txt").write_text("Demo passage text: WikiText, Stephen Merity et al. (2016), derived from Wikipedia contributors.\nSource: https://huggingface.co/datasets/Salesforce/wikitext\nLicense: CC-BY-SA-3.0 https://creativecommons.org/licenses/by-sa/3.0/\nChanges: whitespace normalization and paragraph selection; first-sentence queries.\nModel assets: sentence-transformers/all-MiniLM-L6-v2, Apache-2.0; see model/README.md and model/LICENSE.\nONNX Runtime and its Swift bindings: Microsoft, MIT.\n")
    (bundle/"ONNX-RUNTIME-LICENSE.txt").write_bytes(urlopen("https://raw.githubusercontent.com/microsoft/onnxruntime-swift-package-manager/1.24.2/LICENSE").read())
    dump(ASSETS/"current-testflight-demo.json",metadata)
    report={"device":"physical MacBook Air M3, 8 GB","rust_profile":"release","mac_encoder_max_absolute_error":error,
        "mac_encode_seconds_including_load_and_verification":seconds,"expected_phone_to_mac_retrieval":metrics,
        "source_embedding_execution":"MiniLM reference embeddings computed on Mac during training; physical phone execution is not claimed",
        "stress":stresses,"demo_id":demo_id,"matching_context":str(context.relative_to(ROOT)),"mac_run":str(run.relative_to(ROOT))}
    # Launching Xcode should not replace the saved benchmark with an empty run.
    if not args.skip_stress: dump(ROOT/"reports/mac-validation.json",report)
    print(json.dumps(report,indent=2),flush=True)

if __name__=="__main__":main()
