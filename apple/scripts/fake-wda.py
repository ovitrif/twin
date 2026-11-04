#!/usr/bin/env python3
"""A stand-in for WebDriverAgent that logs each request as one JSON line, for
testing the mirror's input without a phone. Usage: fake-wda.py PORT LOG"""
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

port, log = int(sys.argv[1]), sys.argv[2]

class Handler(BaseHTTPRequestHandler):
    def reply(self, value):
        body = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def handle_any(self):
        length = int(self.headers.get("Content-Length") or 0)
        body = json.loads(self.rfile.read(length) or b"null")
        with open(log, "a") as f:
            f.write(json.dumps({"method": self.command, "path": self.path, "body": body}) + "\n")
        if self.path == "/session":
            self.reply({"sessionId": "fake", "value": {"sessionId": "fake"}})
        elif self.path.endswith("/window/size"):
            self.reply({"value": {"width": 420, "height": 912}})  # iPhone Air in points
        else:
            self.reply({"value": None})

    do_GET = do_POST = handle_any
    def log_message(self, *args): pass

HTTPServer(("127.0.0.1", port), Handler).serve_forever()
