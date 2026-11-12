# Debugging and verification

Reproduce the reported symptom with the same entry point and relevant input. Read exit status, stdout, and stderr. Compare the failing path with a working case. Keep expected output distinct from observed output.

For state bugs, inspect who writes and reads each value. Check initialization, async refresh, stale results, persistence, and what happens on a second attempt. After the fix, reproduce the original path and check one adjacent behavior that could regress. If you cannot reproduce the failure, give the exact check you ran and what remains unknown.

## Worked example

Request: “Cloud model selection keeps reverting.” Observe which provider and model are selected before saving, after saving, and after catalog refresh. Trace the stored value separately from what appears in the picker and the model actually sent to the provider. If a late catalog response overwrites the selection, fix the update path and repeat the same sequence, including switching providers mid-request.

Expected evidence: the saved model survives refresh and reload, and the request metadata identifies the same model when pinned mode is active. Avoid a cosmetic label change that leaves dispatch unchanged. Avoid treating a mocked provider as proof that live credentials or billing work.

## When reproduction fails

Record the exact inputs and environment that you did check. Find the smallest missing fact that could distinguish the likely causes. For an intermittent failure, collect observations across retries and look for a stable invariant before changing code. Never erase state or reset the workspace to make the symptom disappear.

## Gather useful observations

- For a crash or timeout, identify the operation, exit code or error, and last completed step. Avoid guessing which subsystem failed from an unrelated warning.
- For a stale UI, inspect the source value, when the refresh started, and the value that was actually saved. Distinguish what the interface displays from what the backend executes.
- For unexpected cloud use, check the selected mode, eligibility, task type, actual model, and provider receipt. A model listed in the catalog was not necessarily invoked.
- For tool failure, check that the request matched the tool schema, then check permissions and the target's current state. Retry only after something relevant has changed.

A useful report is “After refresh, the display stayed on X and the dispatch test selected X.” An unhelpful report is “probably fixed” with only a successful syntax check.
