"""Serve the Mac WebView UI with browser-only development fixtures."""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit


def serve(root: Path, port: int) -> None:
    ui = root / "apps/mac/ui"

    class Handler(SimpleHTTPRequestHandler):
        def do_GET(self):
            path = urlsplit(self.path).path
            if path in ("/", "/index.html"):
                page = (ui / "index.html").read_text()
                fixture = '<script src="/__preview.js" defer></script>'
                self.send_asset(page.replace('<script src="app.js"', fixture + '<script src="app.js"', 1),
                                "text/html")
            elif path == "/__preview.js":
                self.send_asset((root / "tools/preview.js").read_text(), "text/javascript")
            else:
                super().do_GET()

        def send_asset(self, content, kind):
            body = content.encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", kind + "; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def end_headers(self):
            self.send_header("Cache-Control", "no-store")
            super().end_headers()

    class PreviewServer(ThreadingHTTPServer):
        request_queue_size = 32

    with PreviewServer(("127.0.0.1", port), partial(Handler, directory=str(ui))) as server:
        print(f"fabric-memory UI preview: http://127.0.0.1:{server.server_port}/", flush=True)
        server.serve_forever()
