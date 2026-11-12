# Computer use

For a requested local action, resolve the intended app and open it if needed. Observe its current window before entering input. Match a visible accessibility label to the user's intent, act once, then observe the changed state. Refresh the observation after a navigation, dialog, or layout change. Use coordinates only with a recent screen capture of the same display.

Keep the target app explicit. Check which field has focus before typing. Verify a navigation by observing its loaded URL or page title. If a permission or missing control blocks the task, report it and stop repeating the action. A permission prompt must be handled by the user in macOS.

## Worked example

Request: “Open Safari, search for the Rust book, and show me the result.” Resolve Safari, open or focus it, navigate or search, then observe the window. Use the loaded address or matching title to verify that the result is visible. Report what was seen. An app launch alone proves only that Safari opened.

For “click Export,” inspect the current window, find the accessible Export control, act once, and inspect the next state. A changed dialog or window invalidates old element paths. Never guess coordinates, click through a permission dialog, enter input into the frontmost app without checking its identity, or repeat an action whose effect is uncertain.

For a partial result, say which app or screen was reached and where progress stopped. Ask for the missing permission or the intended target when needed. Keep observations local.

## Tool call sequence

For a file already visible in Finder, start with `computer(action="observe", app="Finder")`. Confirm the expected window and element label. Use the returned element path and label in `computer(action="click", app="Finder", element=..., expected=...)`, then observe again. When opening a URL, provide the requested URL and browser explicitly. Verify the destination before reporting that it opened.

If accessibility reports no suitable control, a recent screenshot may help orient the user. Use screen coordinates only when the active model can ground the target in that exact capture. If the app, target, or selection remains uncertain, ask the user instead of clicking a guess. Before acting on anything that sends, publishes, deletes, or changes an account, check that the user authorized that exact outcome.

## Worked tool calls

- Open Chrome with `computer {"action":"open_app","app":"chrome"}`, then use `computer {"action":"observe","app":"chrome"}` if the task depends on a visible window. A launch receipt does not establish the page or input state.
- To click a visible control, take its path and label from the most recent observation of that app. For example, if the observed line is `0.2 AXButton · Continue`, call `computer {"action":"click","app":"com.example.app","element":"0.2","expected":"AXButton · Continue"}`. Replace the example target with the observed target. If the label changes, observe again and choose the new path.
- To capture a screenshot, call `computer {"action":"screenshot"}` and check the result for the saved image path. Opening Screenshot alone does not complete capture.

## Live macOS smoke test

On an unlocked Mac with Fabric Memory installed and Accessibility granted, run:

```sh
python3 tools/check_computer_use.py
python3 tools/check_computer_use.py --agent --model qwen3-vl:2b
python3 tools/check_computer_use.py --chrome
```

The first command exercises the installed controller against a temporary AppKit window. The fixture writes the submitted text to its own receipt file, so a successful tool response alone cannot pass the test. The second command also checks whether the selected local model can plan and complete the task through Fabric's backend with isolated data. The screenshot check requires Screen Recording permission. The fixture takes focus while the test runs.

The Chrome check opens a temporary loopback page in a new tab, types a random value, and reads it back from Chrome. It closes the tab only after identifying the test page.
