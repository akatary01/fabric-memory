#!/usr/bin/env python3
"""Complete reverse translation using the JSON shared by the TestFlight iPhone app."""
import argparse
import json
import subprocess
import time
import uuid
from pathlib import Path
import onnxruntime
from prepare import ROOT, ASSETS, dump
from metrics import score

def main():
    p=argparse.ArgumentParser();p.add_argument("result",type=Path);args=p.parse_args()
    assert args.result.stat().st_size<=32*1024*1024,"result file exceeds limit"
    result=json.loads(args.result.read_text())
    assert result["schema_version"]==1 and result["type"]=="memory-fabric-device-export"
    demo_id=result["demo_id"]
    assert len(demo_id)==64 and all(c in "0123456789abcdef" for c in demo_id)
    context=ASSETS/"testflight"/demo_id
    assert context.exists(),"Matching Mac context missing. Keep assets/testflight with the archive you upload."
    metadata=json.loads((context/"demo.json").read_text())
    run=ROOT/"runs"/("testflight-import-"+uuid.uuid4().hex[:12]);run.mkdir(parents=True)
    events=result["events"]
    assert 0<len(events)<=10000
    assert all(e["embedding"]["space"]==metadata["phone_model_space"] for e in events)
    incoming=run/"phone-events.jsonl";incoming.write_text("".join(json.dumps(e)+"\n" for e in events))
    runtime=next((Path(onnxruntime.__file__).parent/"capi").glob("libonnxruntime.*.dylib"))
    binary=ROOT/"target/release/fabric-core"
    def cli(db,*action):
        r=subprocess.run([str(v) for v in [binary,"--runtime",runtime,"--model",ASSETS/"mac","--db",run/db,*action]],capture_output=True,text=True)
        if r.returncode: raise RuntimeError(r.stderr)
        return r.stdout
    start=time.perf_counter()
    cli("foreign.sqlite","import",incoming,"--adapter",ASSETS/"adapters/phone-to-mac")
    elapsed=time.perf_counter()-start
    cli("foreign.sqlite","search-batch",context/"queries.json",run/"rankings.json")
    metrics=score(json.loads((context/"native-rankings.json").read_text()),json.loads((run/"rankings.json").read_text()))
    cli("foreign.sqlite","archive",run/"git-ledger")
    duplicate=cli("foreign.sqlite","import",incoming,"--adapter",ASSETS/"adapters/phone-to-mac")
    assert "inserted=0" in duplicate
    # A fresh process reopens the durable index after import and duplicate replay.
    status=json.loads(cli("foreign.sqlite","status"))
    report={"demo_id":demo_id,"phone_execution":"physical iPhone, user-installed TestFlight build",
        "phone_to_mac":{**metrics,"import_seconds_including_process_start":elapsed,"documents_reembedded_during_import":0},
        "mac_to_phone":result["mac_to_phone"],"phone_stress":result["stress"],
        "phone_encoder":{"reference_max_absolute_error":result["encode"]["reference_max_absolute_error"],
            "tokenizer_cases_passed":result["encode"]["tokenizer_cases_passed"],"peak_rss_bytes":result["encode"]["peak_rss_bytes"]},
        "duplicate_replay_insertions":0,"mac_import_status":status}
    dump(ROOT/"reports/testflight-device-results.json",report)
    print(json.dumps(report,indent=2))

if __name__=="__main__":main()
