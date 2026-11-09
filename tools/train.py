#!/usr/bin/env python3
"""CPU-only adapter training on Mac. Uses actual encoders, never dummy vectors."""
import os
os.environ.setdefault("OPENBLAS_NUM_THREADS","2")
os.environ.setdefault("OMP_NUM_THREADS","2")
import argparse
import gc
import json
import re
import shutil
import time
from pathlib import Path
import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer
from prepare import ROOT, ASSETS, dump, sha

def unit(x): return x / np.maximum(np.linalg.norm(x,axis=-1,keepdims=True),1e-12)

class Encoder:
    def __init__(self, folder):
        self.manifest = json.loads((folder/"encoder.json").read_text())
        self.tokenizer = Tokenizer.from_file(str(folder/"tokenizer.json"))
        self.tokenizer.enable_truncation(max_length=256)
        self.tokenizer.no_padding()
        options = ort.SessionOptions(); options.intra_op_num_threads = 2; options.inter_op_num_threads = 1
        self.session = ort.InferenceSession(str(folder/"model.onnx"),sess_options=options,providers=["CPUExecutionProvider"])
    def encode(self, texts):
        out = []
        for i,text in enumerate(texts):
            e = self.tokenizer.encode(text)
            feeds = {"input_ids":np.array([e.ids],np.int64),"attention_mask":np.array([e.attention_mask],np.int64),"token_type_ids":np.array([e.type_ids],np.int64)}
            y = self.session.run(None,{v.name:feeds[v.name] for v in self.session.get_inputs()})[0]
            mask = feeds["attention_mask"][:,:,None]
            pooled = (y*mask).sum(axis=1)/mask.sum(axis=1)
            out.append(unit(pooled)[0].astype(np.float32))
            if (i+1)%200 == 0: print(f"{self.manifest['model']}: {i+1}/{len(texts)}",flush=True)
        return np.array(out,dtype=np.float32)

def metrics(native_docs, query, translated_docs):
    native_scores = query @ native_docs.T
    projected_scores = query @ translated_docs.T
    native = np.argsort(-native_scores,axis=1,kind="stable")[:,:5]
    projected = np.argsort(-projected_scores,axis=1,kind="stable")[:,:5]
    relevant = np.arange(len(query))
    native_hits = (native == relevant[:,None]).any(axis=1)
    projected_hits = (projected == relevant[:,None]).any(axis=1)
    return {"queries":len(query),"documents":len(native_docs),"native_recall_at_5":float(native_hits.mean()),
            "translated_recall_at_5":float(projected_hits.mean()),
            "native_top5_overlap":float(np.mean([len(set(a)&set(b))/5 for a,b in zip(native,projected)])),
            "native_top1_agreement":float(np.mean(native[:,0] == projected[:,0])),
            "mean_paired_cosine":float(np.mean(np.sum(native_docs*translated_docs,axis=1)))}

def fit(source,target,source_val,target_val,source_space,target_space,name,train_hash):
    x = source.astype(np.float64); y = target.astype(np.float64)
    xm=x.mean(0); ym=y.mean(0); xc=x-xm; yc=y-ym
    gram=xc.T@xc; cross=xc.T@yc; candidates=[]
    for ridge in [0.01,0.1,1.,10.]:
        w=np.linalg.solve(gram+ridge*np.eye(x.shape[1]),cross)
        prediction=unit((source_val-xm)@w+ym)
        score=float(np.mean(np.sum(prediction*target_val,axis=1)))
        candidates.append((score,ridge,w))
    score,ridge,w=max(candidates,key=lambda v:v[0])
    folder=ASSETS/"adapters"/name; folder.mkdir(parents=True,exist_ok=True)
    raw=w.astype("<f4").tobytes(); (folder/"weights.f32").write_bytes(raw)
    dump(folder/"adapter.json",{"version":1,"source":source_space,"target":target_space,
         "input_dim":x.shape[1],"output_dim":y.shape[1],"source_mean":xm.astype(np.float32).tolist(),
         "target_mean":ym.astype(np.float32).tolist(),"weights_sha256":sha(raw),"training_sha256":train_hash,"ridge":ridge})
    return lambda v: unit((v-xm.astype(np.float32))@w.astype(np.float32)+ym.astype(np.float32)), {"validation_cosine":score,"ridge":ridge,"candidates":[{"cosine":s,"ridge":r} for s,r,_ in candidates]}

def main():
    parser=argparse.ArgumentParser(); parser.add_argument("--force-encode",action="store_true"); args=parser.parse_args()
    start=time.perf_counter()
    corpus=json.loads((ASSETS/"corpus.json").read_text()); split=corpus["splits"]
    docs=split["test"][:64]
    # Queries are the first sentence of each held-out paragraph. This is a controlled
    # passage-retrieval check, not a general semantic-search benchmark.
    queries=[re.split(r"(?<=[.!?])\s+",s,maxsplit=1)[0] for s in docs[:32]]
    token_texts=["Hello, world!","Café naïve résumé","你好世界。","Hello [MASK] world.","Tabs\tand\nnewlines", "" ,"x"*150,"🙂 — punctuation!"]
    texts=split["train"]+split["validation"]+docs+queries+token_texts
    text_hash=sha(json.dumps(texts,ensure_ascii=False).encode())
    vectors={}; manifests={}; cache_used={}
    for name in ["phone","mac"]:
        manifests[name]=json.loads((ASSETS/name/"encoder.json").read_text())
        cache=ASSETS/f"{name}-{text_hash}-{manifests[name]['space']}.npy"
        cache_used[name]=cache.exists() and not args.force_encode
        if cache_used[name]: vectors[name]=np.load(cache)
        else:
            enc=Encoder(ASSETS/name); vectors[name]=enc.encode(texts); np.save(cache,vectors[name]); del enc; gc.collect()
    n=len(split["train"]); v=len(split["validation"]); d=len(docs); q=len(queries)
    train_hash=sha(json.dumps({"texts":split["train"],"spaces":[manifests[k]["space"] for k in ["phone","mac"]]},sort_keys=True).encode())
    report={"anchors":n,"validation":v,"held_out_documents":d,"queries":q,"dataset":corpus["source"],"dataset_revision":corpus["revision"],
            "evaluation_note":"First-sentence queries over unseen WikiText test paragraphs. Measures controlled passage retrieval, not broad RAG quality.","directions":{}}
    golden={}
    for src,dst in [("phone","mac"),("mac","phone")]:
        x,y=vectors[src],vectors[dst]
        translate,training=fit(x[:n],y[:n],x[n:n+v],y[n:n+v],manifests[src]["space"],manifests[dst]["space"],f"{src}-to-{dst}",train_hash)
        pred=translate(x[n+v:n+v+d]); native=y[n+v:n+v+d]; query=y[n+v+d:n+v+d+q]
        report["directions"][f"{src}-to-{dst}"]={**training,**metrics(native,query,pred)}
        golden[f"{src}-to-{dst}"]={"input":{"space":manifests[src]["space"],"values":x[n+v].tolist()},"expected":{"space":manifests[dst]["space"],"values":pred[0].tolist()}}
    dump(ASSETS/"golden.json",golden)
    phone_tokenizer=Tokenizer.from_file(str(ASSETS/"phone"/"tokenizer.json")); phone_tokenizer.enable_truncation(max_length=256); phone_tokenizer.no_padding()
    job={"version":1,"documents":docs,"queries":queries,"tokenizer_cases":[{"text":t,"ids":phone_tokenizer.encode(t).ids} for t in token_texts],
         "reference_phone_vectors":vectors["phone"][n+v:].tolist(),"stress_count":1000}
    dump(ASSETS/"job.json",job)
    dump(ASSETS/"mac-texts.json",docs+queries)
    dump(ASSETS/"mac-reference.json",vectors["mac"][n+v:n+v+d+q].tolist())
    phone_bundle=ASSETS/"phone-bundle"; phone_bundle.mkdir(exist_ok=True)
    for src,dest in [(ASSETS/"phone",phone_bundle/"model"),(ASSETS/"adapters"/"mac-to-phone",phone_bundle/"adapter")]:
        shutil.copytree(src,dest,dirs_exist_ok=True,ignore=shutil.ignore_patterns(".cache","onnx"))
    shutil.copy2(ASSETS/"job.json",phone_bundle/"job.json")
    report["seconds"]=time.perf_counter()-start
    report["encoding_cache_used"]=cache_used
    dump(ROOT/"reports"/"adapter-training.json",report)
    print(json.dumps(report,indent=2),flush=True)

if __name__ == "__main__": main()
