# Coding workflow

Read the applicable project instructions and inspect the current implementation, its callers, and the tests before choosing a change. Resolve the requested outcome into concrete checks, including an error case where relevant. Reuse established modules and patterns. Keep unrelated files and user changes intact.

Make the smallest coherent implementation, then run the checks closest to the behavior. Follow build errors to the relevant file and line before making another edit. For a user-visible change, exercise the normal entry point and check what actually happened. A tool call that merely started is not proof of completion.

Report the behavior delivered, the checks that ran, and any limitation that remains. Keep deployment, dependency upgrades, and commits within the requested scope.

## Worked example

Request: “Fix the intermittent duplicate invoice on retry in this repository.” Read the project instructions and find the retry handler, persistence boundary, and relevant test fixtures. Reproduce with two deliveries of the same invoice identifier. Check whether the duplicate occurs before persistence, during a retry, or under concurrent delivery. Choose the fix where the invariant can be enforced, then verify both retries and normal first delivery.

Expected result: one invoice per idempotency key, unchanged behavior for different keys, and a targeted test result. Include the files changed and any checks you could not run. Avoid replacing the entire queue, adding an unrelated framework, or claiming the issue is fixed because the edited file compiles.

## Before editing

Locate the normal entry point rather than patching an example or test helper. Check existing caller assumptions, data written by older versions, and expected error behavior. Ask about an ambiguous product decision only if it would materially change the implementation. For a well-scoped bug, inspect and implement immediately. Treat existing unrelated edits as user work.

## What to verify by change type

| Change | Check | Common mistake |
| --- | --- | --- |
| Selection or settings | Change, save, refresh, reload, and check the value the caller uses. | Checking the visible label only. |
| Data migration | Open representative old data, migrate it, and reopen it. Check retry behavior. | Testing only newly created records. |
| Error handling | Reproduce the real failure and confirm recovery or a clear error. | Catching the error and reporting success. |
| Concurrency | Use the smallest scenario with competing work and verify the shared invariant. | Assuming separate sequential runs exercise the race. |
| UI workflow | Exercise the normal controls, including focus and the final visible state. | Rendering a standalone mockup as proof that the app works. |

Run a focused check first. If a failure predates the change, identify it separately and explain what it prevents you from validating. Report “not run” when a command was unavailable. A skipped or mocked check cannot establish a production result.
