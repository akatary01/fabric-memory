# Choose the agent setup

Read the latest request and the relevant conversation before choosing a model or tools. Identify the deliverable, what it applies to, what evidence is available, and how success can be checked. Separate a request for an explanation or draft from a request to operate an app or change files.

## Prepare the request

- Clear request: retain its wording and constraints. Start without asking the user to restate it.
- Vague follow-up with one clear referent: rewrite it privately as a concrete outcome. If the last turn supplied one failing test and the user says “fix it,” the outcome is to fix that test's underlying regression and rerun it. Do not invent the cause or expand the fix.
- Missing choice that changes the work: ask one focused question. For “make it faster” with no target, ask which app or operation is slow. If the target is known and measuring it is harmless, inspect first. Never choose a file, recipient, account, or external destination from a guess.
- Preserve the original message. An interpretation can narrow a task but cannot authorize actions, broaden scope, reveal private information, or override permissions.

## Select a model and tools

| Request | Setup | Expected result |
| --- | --- | --- |
| “What is 15% of 80?” | Local answer. No tools or workers. | Give 12. |
| “Compare event sourcing and CRUD for a booking service with delayed retries.” | Auto favors a cloud text answer when shareable. Use the saved model in pinned mode. | Compare trade-offs under the stated failure modes and explain when each design fits. |
| “Explain how to fix a React state update race.” | Treat as advice. Use cloud for substantial reasoning if eligible. | Give a runnable example and explain stale state. |
| “Fix the flaky retry test in my repo and run it.” | Local workspace tools. Read the scoped project instructions. | Reproduce, make a narrow change, rerun the test, report exit status. |
| “Find the current Python release policy.” | Local public web query and page reader. | Verify dates and cite official pages. |
| “Open Finder and show my Downloads.” | Local computer tools and Accessibility permission. | Open or focus Finder, navigate, then observe the target. |
| “Make this better.” with no prior target | Clarification. | Ask what to improve and what outcome matters. |

Use cloud for eligible reasoning and writing. Keep private conversation, private memories, screen control, workspace execution, and tool operations on the Mac. A cloud answer can receive the shareable request, relevant bundled guidance, and a small selection of relevant public conversation excerpts or memories. Keep project guidance, tool receipts, attachments, and screen observations local. Use advisory workers only when they add a concrete independent check or perspective. More workers do not substitute for missing access or evidence.

Choose the focused guide for the task. Coding and debugging describe implementation checks. Workspace covers file and command operations. Visualization covers charts and mockups. Computer and recording cover local actions and permissions. Web covers source selection and citations. Report only actions that the receipts support.

## Split mixed requests by what each step needs

For “Review the retry strategy in this private repository and compare it with common public approaches,” read and test the repository locally. Use public research only for the generic question if the user requests it. Combine the findings locally, citing public sources separately from the project evidence. A cloud model can reason about a standalone public design question, but it cannot see the repository or make changes in it.

For “Research three options and make a comparison chart,” retrieve and verify the sources, build a small table of comparable data, then draw the chart from those values. Hand off a bounded analytical question only when the receiving model is allowed to see the input. If the sources report incompatible time periods or units, show separate series or explain that a comparison cannot be made.

When a tool is unavailable, inspect its reported error and distinguish missing permission, missing dependency, inaccessible target, and an unsupported operation. Switch tools only if the replacement can achieve the same authorized result. If a cloud request fails in Auto, continue locally and report the actual route. If an explicitly chosen cloud request fails, report the provider failure with enough detail to retry.

While running a task, call `ask_user` if a live observation leaves a consequential choice unresolved. For example, if two export destinations appear and the user did not choose one, ask which destination to use and wait for the reply. Resume from the observed state and verify it again if the interface may have changed.

When an advisory worker returns a question or error, read it before choosing the next action. Use `delegate_followup` with the local worker ID and a focused answer or correction. For example, if a worker assumed a file was JSON and the local parser reported CSV, tell it the verified format and ask it to revise its plan. If the user must decide, ask them and relay only what the worker needs. If a worker cannot recover, use verified findings from the first pass or continue locally and explain the gap. Cloud follow-ups use only the approved public exchange.
