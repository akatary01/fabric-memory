"""Live test of the installed Fabric Memory controller on an unlocked Mac.

Run: python3 tools/check_computer_use.py
Uses a disposable AppKit window and writes only inside its temporary directory.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import plistlib
import re
import signal
import subprocess
import tempfile
from threading import Thread
import time
import uuid

FIXTURE = Path(__file__).parent / "tests/fixtures/ComputerUseFixture.swift"
TITLE = "Fabric Computer Use Fixture"


def until(callback, description, seconds=8):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = callback()
        if value:
            return value
        time.sleep(0.15)
    raise AssertionError(description)


def controller(helper, root, **args):
    result = subprocess.run(
        [helper], input=json.dumps(args), text=True, capture_output=True, timeout=15,
        env={**os.environ, "FABRIC_DATA_DIR": str(root)},
    )
    payload = json.loads(result.stdout)
    if result.returncode or "error" in payload:
        raise RuntimeError(payload.get("error") or result.stderr)
    return payload["result"]


def build_fixture(root):
    identifier = f"local.memoryfabric.test.{uuid.uuid4().hex}"
    app = root / f"{TITLE}.app"
    executable = app / "Contents/MacOS/Fixture"
    executable.parent.mkdir(parents=True)
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps({
        "CFBundleExecutable": "Fixture", "CFBundleIdentifier": identifier,
        "CFBundleName": TITLE, "CFBundlePackageType": "APPL",
        "CFBundleVersion": "1",
    }))
    subprocess.run(["xcrun", "swiftc", str(FIXTURE), "-o", executable], check=True)
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", app], check=True, capture_output=True)
    return app, identifier


def run(app_bundle, agent, model):
    helper = app_bundle / "Contents/MacOS/fabric-computer-use"
    with tempfile.TemporaryDirectory(prefix="fabric-computer-check-") as directory:
        root = Path(directory)
        permissions = controller(helper, root, action="permissions")
        if not permissions.get("accessibility"):
            raise RuntimeError("Enable Fabric Memory in macOS Accessibility before running the live test.")
        app, identifier = build_fixture(root)
        pid = None
        try:
            opened = controller(helper, root, action="open_app", app=str(app))
            assert opened["opened"] == identifier
            pid = int(opened["pid"])
            print("PASS open_app: returned the running fixture's bundle ID and PID.", flush=True)

            def observe():
                return controller(helper, root, action="observe", app=identifier)

            def visible_fixture():
                state = observe()
                return state if TITLE in state.get("ui", "") else None

            observed = until(visible_fixture, "Fixture window never appeared in the accessibility tree.")
            button = re.search(r"^([0-9.]+) (AXButton[^\n]*Submit fixture[^\n]*)$", observed["ui"], re.MULTILINE)
            assert button, "Submit button missing from the accessibility tree."
            print("PASS observe: found the actual window and Submit button.", flush=True)

            receipt = root / "submitted.txt"
            args = dict(action="click", app=identifier, element=button[1], observed_pid=pid)
            try:
                controller(helper, root, **args, expected="Deliberately wrong label", expected_exact=True)
            except RuntimeError as error:
                assert "target changed" in str(error).lower(), str(error)
            else:
                raise AssertionError("Controller accepted a stale label.")
            assert not receipt.exists(), "A rejected click changed the fixture."
            print("PASS stale click: rejected the wrong label and produced no side effect.", flush=True)

            nonce = f"Fabric check {uuid.uuid4().hex[:10]}"
            controller(helper, root, action="type", app=identifier, text=nonce)
            until(lambda: nonce in observe()["ui"], "Typed text never appeared in the fixture.")
            print("PASS type: the fixture's accessibility value matched the typed text.", flush=True)
            current = observe()
            button = re.search(r"^([0-9.]+) (AXButton[^\n]*Submit fixture[^\n]*)$", current["ui"], re.MULTILINE)
            assert button, "Submit button disappeared."
            controller(helper, root, **{**args, "element": button[1]}, expected=button[2], expected_exact=True)
            until(receipt.is_file, "The fixture never received the click.")
            assert receipt.read_text() == nonce, "The fixture submitted the wrong text."
            print("PASS click: the target app independently saved the exact typed text.", flush=True)

            if permissions.get("screen_recording"):
                capture = Path(controller(helper, root, action="screenshot")["image_path"])
                png = capture.read_bytes()
                assert png.startswith(b"\x89PNG\r\n\x1a\n") and png[12:16] == b"IHDR"
                width = int.from_bytes(png[16:20], "big")
                height = int.from_bytes(png[20:24], "big")
                assert width > 0 and height > 0
                print(f"PASS screenshot: saved a PNG measuring {width} × {height}.", flush=True)
            else:
                print("SKIP screenshot: macOS Screen Recording permission is off.", flush=True)

            if agent:
                run_agent(app_bundle, root, identifier, receipt, model)
        finally:
            if pid:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass


def run_agent(app_bundle, root, identifier, receipt, model):
    nonce = f"Fabric agent {uuid.uuid4().hex[:10]}"
    receipt.unlink()
    info = plistlib.loads((app_bundle / "Contents/Info.plist").read_bytes())
    data = root / "agent-data"
    data.mkdir()
    (data / "computer-use.json").write_text(json.dumps({"enabled": True, "screen_sharing": False}))
    (data / "agent-settings.json").write_text(json.dumps({"auto_approve": True}))
    if model:
        (data / "model-config.json").write_text(json.dumps({"model": model}))
    environment = {**os.environ, **info.get("LSEnvironment", {}), "FABRIC_DATA_DIR": str(data),
                   "FABRIC_MODE": "local", "FABRIC_WEB": "off"}
    environment.pop("FABRIC_ENV_FILE", None)
    prompt = (f'Use computer control on app {identifier} (the {TITLE}). '
              f'Type "{nonce}" into its field, then click the Submit fixture button.')
    try:
        outcome = subprocess.run([app_bundle / "Contents/MacOS/fabric-mac", "--prompt-stdin"],
                                 input=prompt, text=True, env=environment, capture_output=True, timeout=240)
    finally:
        activity = data / "activity.jsonl"
        if activity.is_file():
            for line in activity.read_text().splitlines():
                step = json.loads(line)
                if step.get("tool") == "computer":
                    print(f"AGENT {step.get('kind')} {step.get('action') or ''} {step.get('summary') or ''}", flush=True)
    if outcome.returncode:
        raise AssertionError(f"Agent exited with {outcome.returncode}: {outcome.stderr[-600:]}")
    reply = json.loads(outcome.stdout.strip().splitlines()[-1])
    if not receipt.is_file() or receipt.read_text() != nonce:
        raise AssertionError(f"Agent did not submit the expected text. Reply: {reply.get('text', '')[:600]}")
    if reply.get("failure"):
        raise AssertionError(f"Agent performed the UI actions but reported failure: {reply['failure']}")
    print(f"PASS agent ({model or 'default model'}): fixture saved the exact string {nonce!r}.", flush=True)


def run_chrome(app_bundle):
    """Verify real browser input using a temporary loopback page."""
    title = f"Fabric Chrome Probe {uuid.uuid4().hex[:8]}"
    expected = f"fabric-chrome-{uuid.uuid4().hex[:8]}"

    class ProbePage(BaseHTTPRequestHandler):
        def do_GET(self):
            page = f'<title>{title}</title><label>Fabric test input <input autofocus></label>'
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.end_headers()
            self.wfile.write(page.encode("utf-8"))

        def log_message(self, *_):
            pass

    helper = app_bundle / "Contents/MacOS/fabric-computer-use"
    with tempfile.TemporaryDirectory(prefix="fabric-chrome-check-") as directory, \
            ThreadingHTTPServer(("127.0.0.1", 0), ProbePage) as server:
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        root = Path(directory)
        url = f"http://127.0.0.1:{server.server_port}/probe"

        def chrome(action, **args):
            return controller(helper, root, action=action, app="chrome", **args)

        def observed():
            state = chrome("observe")
            return state if title in state.get("window_title", "") else None

        try:
            chrome("open_url", url=url, new_tab=True)
            state = until(observed, "Chrome did not display the temporary test page.")
            print("PASS Chrome navigation: observed the temporary page in a new tab.", flush=True)
            field = re.search(r"^([0-9.]+) AXTextField[^\n]*Fabric test input", state["ui"], re.MULTILINE)
            assert field, "Chrome did not expose the test input."
            chrome("type", element=field[1], text=expected)
            until(lambda: expected in (observed() or {}).get("ui", ""), "Chrome did not reflect the typed text.")
            print(f"PASS Chrome input: read back the exact text {expected!r}.", flush=True)
        finally:
            try:
                if observed():
                    chrome("key", key="Cmd+W")
            finally:
                server.shutdown()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, default=Path("/Applications/Fabric Memory.app"))
    parser.add_argument("--agent", action="store_true", help="Also test Fabric's local planner with isolated app data.")
    parser.add_argument("--model", help="Use this installed local model for the agent test.")
    parser.add_argument("--chrome", action="store_true", help="Also test input in a temporary Chrome tab.")
    options = parser.parse_args()
    run(options.app, options.agent, options.model)
    if options.chrome:
        run_chrome(options.app)
