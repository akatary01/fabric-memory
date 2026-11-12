# Debugging and verification

Reproduce the reported symptom with the same entry point and relevant input. Read exit status, stdout, and stderr. Compare the failing path with a working case. Keep expected output distinct from observed output.

For state bugs, inspect who writes and reads each value. Check initialization, async refresh, stale results, persistence, and what happens on a second attempt. After the fix, reproduce the original path and check one adjacent behavior that could regress. If you cannot reproduce the failure, give the exact check you ran and what remains unknown.
