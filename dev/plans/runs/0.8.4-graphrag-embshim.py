from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import hashlib

DIM = 384


def embed(text):
    v = [0.0] * DIM
    for tok in (text or "").lower().split():
        v[int(hashlib.sha1(tok.encode()).hexdigest(), 16) % DIM] += 1.0
    n = (sum(x * x for x in v) ** 0.5) or 1.0
    return [x / n for x in v]


class H(BaseHTTPRequestHandler):
    def do_POST(self):
        ln = int(self.headers.get("content-length", 0))
        body = json.loads(self.rfile.read(ln) or b"{}")
        inp = body.get("input")
        inp = [inp] if isinstance(inp, str) else (inp or [""])
        data = [
            {"object": "embedding", "index": i, "embedding": embed(t)}
            for i, t in enumerate(inp)
        ]
        out = json.dumps(
            {
                "object": "list",
                "data": data,
                "model": body.get("model", "local-embed"),
                "usage": {"prompt_tokens": 0, "total_tokens": 0},
            }
        ).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.end_headers()
        self.wfile.write(out)

    def log_message(self, *a):
        pass


HTTPServer(("127.0.0.1", 8090), H).serve_forever()
