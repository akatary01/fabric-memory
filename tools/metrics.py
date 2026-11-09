"""Controlled retrieval metrics shared by the Mac evaluation tools."""
def score(native, translated):
    assert len(native) == len(translated) and len(native) > 0
    n = len(native); hits = 0; baseline = 0; overlap = 0.; rr = 0.
    for i, (a, b) in enumerate(zip(native, translated)):
        a = [v["memory"] for v in a]; b = [v["memory"] for v in b]
        relevant = f"eval-{i:04d}"
        baseline += relevant in a; hits += relevant in b
        if relevant in b: rr += 1 / (b.index(relevant) + 1)
        overlap += len(set(a) & set(b)) / 5
    return {"native_recall_at_5":baseline/n,"translated_recall_at_5":hits/n,
            "translated_mrr_at_5":rr/n,"native_top5_overlap":overlap/n}
