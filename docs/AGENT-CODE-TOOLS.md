# Code and tests

Examples show the logical tool name and JSON arguments. Use the name and fields offered for the current task.

## Fix a parser that fails on blank input

1. Find the entry point with `run_code {"path":".","command":"rg -n 'def parse_header|parse_header\\(' src tests"}`. Read the implementation and focused tests with `workspace.read`.
2. Check existing callers to determine whether blank input should return a value or raise a particular error. If this is ambiguous and changes the public contract, ask.
3. Reproduce the failure with the project's test command. If there is no covering test, add one when needed to distinguish the old behavior from the fix. Keep coverage for valid input.
4. Edit the implementation. `workspace.write` requires `action`, `path`, and `content`. Its content replaces the entire file. For a small change in a long file, use a focused patch through the approved workspace runner, then inspect the diff.
5. Run the targeted test. For a Python unittest suite, an example is `run_code {"path":".","command":"python3 -m unittest discover -s tests -p 'test_parser.py'"}`. For Rust, use the relevant Cargo package or test filter. Follow the repository's own commands when available.
6. Run `git diff --check` and inspect the diff. Confirm the change is connected to the normal caller. Report any checks you could not run.

## Read the receipts

- A command can exit with zero while testing nothing. Confirm the expected tests ran.
- A compile error means the change has not been exercised. Read the first relevant diagnostic and correct its cause.
- When a tool is missing, use a project-supported alternative if available. For a text search, `git grep -n` can search tracked files when `rg` is missing.
- Keep search paths narrow. Do not scan the home directory to locate a project symbol.
- After a workspace read, use the result. After an edit, reread the changed portion or inspect the diff. If a retry cannot change the outcome, state the blocker or ask one focused question.

## Write the regression check before claiming the fix

Suppose the existing contract says a blank header produces no fields, and the implementation returns one empty field. A focused regression test could look like this, using the project's existing test framework and imports:

```python
def test_blank_header_has_no_fields():
    assert parse_header("   ") == []

def test_nonempty_header_keeps_its_fields():
    assert parse_header("name,version") == ["name", "version"]
```

Check that the first test exposes the original failure. Make the smallest change that preserves the existing parsing rules, such as returning an empty list for whitespace-only input before splitting. Then run both tests through the project's runner. If the project defines a different contract, use that contract and adapt the cases. Do not copy the example into an unrelated project.
