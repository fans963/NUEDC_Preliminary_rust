#!/usr/bin/env python3
"""
本机前端测试服务器 — 以正确 MIME 类型 serve frontend 目录 + mock API。

用法:
    cd NUEDC_Preliminary_rust
    python3 serve-frontend.py
    然后访问 http://localhost:8000
"""

import http.server
import json
import os
import sys
import time

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
FRONTEND_DIR = os.path.join(os.path.dirname(__file__), "frontend")


class FrontendHandler(http.server.SimpleHTTPRequestHandler):
    """serve 静态文件 + mock API 端点。"""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=FRONTEND_DIR, **kwargs)

    # ── API 路由 ────────────────────────────────────────────────────────

    api_state = {"state": "idle", "is_running": False, "is_loading": False, "animation_count": 0, "pages": 0}

    def _api_status(self):
        self.send_json(self.api_state)

    def _api_start(self):
        self.api_state["state"] = "running"
        self.api_state["is_running"] = True
        self.send_json({"status": "started"})

    def _api_stop(self):
        self.api_state["state"] = "idle"
        self.api_state["is_running"] = False
        self.send_json({"status": "stopped"})

    def _api_upload(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length) if length else b""
        self.api_state["animation_count"] += 1
        self.api_state["state"] = "running"
        self.api_state["is_running"] = True
        self.send_json({"status": "ok", "pages": 1, "bytes": len(body)})

    # ── 工具方法 ────────────────────────────────────────────────────────

    def send_json(self, obj, status=200):
        data = json.dumps(obj).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    # ── 请求分发 ────────────────────────────────────────────────────────

    def do_GET(self):
        path = self.path.split("?")[0].rstrip("/")

        if path == "/api/status":
            return self._api_status()
        if path == "/favicon.ico":
            self.send_response(204)
            self.end_headers()
            return

        return super().do_GET()

    def do_POST(self):
        path = self.path.split("?")[0].rstrip("/")

        if path == "/api/start":
            return self._api_start()
        if path == "/api/stop":
            return self._api_stop()
        if path == "/api/upload":
            return self._api_upload()

        self.send_json({"error": "not found"}, 404)

    def guess_type(self, path):
        if path.endswith(".html"):
            return "text/html; charset=utf-8"
        return super().guess_type(path)


if __name__ == "__main__":
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
    server = http.server.HTTPServer(("0.0.0.0", PORT), FrontendHandler)
    print(f"→ POV 前端测试服务器: http://localhost:{PORT}")
    print(f"  API: /api/status, /api/start, /api/stop, /api/upload")
    print(f"  按 Ctrl+C 停止")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n  已停止")
        server.server_close()
