#!/usr/bin/env python3
import json, os, re, signal, subprocess, sys, threading, time
from urllib.parse import urlsplit
from http.server import BaseHTTPRequestHandler, HTTPServer
here = os.path.dirname(os.path.abspath(__file__))
mode = os.environ.get("FAKE_CLOUDFLARED_MODE") or open(os.path.join(here, "mode")).read().strip()
args = sys.argv[1:]
with open(os.path.join(here, "argv.json"), "w") as f:
    json.dump({"argv": args, "cwd": os.getcwd()}, f)
def log(level, msg):
    sys.stderr.write(json.dumps({"level": level, "message": msg, "time": "2026-10-10T06:00:00Z"}) + "\n")
    sys.stderr.flush()

# Overrides are per invocation, so parallel tests never mutate process-wide environment.
if any(k in os.environ for k in ("FAKE_CLOUDFLARED_STDOUT", "FAKE_CLOUDFLARED_STDERR", "FAKE_CLOUDFLARED_EXIT_CODE")):
    sys.stdout.write(os.environ.get("FAKE_CLOUDFLARED_STDOUT", ""))
    sys.stderr.write(os.environ.get("FAKE_CLOUDFLARED_STDERR", ""))
    sys.exit(int(os.environ.get("FAKE_CLOUDFLARED_EXIT_CODE", "0")))
if mode in ("hang", "ignore-term", "descendant"):
    if mode in ("ignore-term", "descendant"):
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
    descendant = None
    if mode == "descendant":
        descendant = subprocess.Popen([sys.executable, "-c", "import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(300)"])
    with open(os.path.join(here, "pids.json"), "w") as f:
        json.dump({"parent": os.getpid(), "descendant": descendant.pid if descendant else None}, f)
    while True:
        time.sleep(0.2)
errors = {
    "unknown-flag": (0, "Incorrect Usage: flag provided but not defined: -fake-unknown"),
    "login-refused": (0, "You have an existing certificate at /synthetic/cert.pem which login would overwrite. If this is intentional, please move or delete that file then run this command again."),
    "missing-config": (1, "Cannot read config file /synthetic/missing.yml: no such file or directory"),
    "missing-credentials": (1, "Tunnel credentials file '/synthetic/missing.json' doesn't exist or is not a file"),
    "missing-cert": (1, "Cannot determine default origin certificate path. No file cert.pem in isolated HOME. You need to specify the origin certificate path with the origincert option."),
    "invalid-config": (1, "Validation failed: The last ingress rule must match all URLs (i.e. it must not have a hostname or path filter)"),
    "incompatible-url": (1, "Multiple-origin routing is incompatible with the --url flag"),
    "metrics-bind": (1, "Error opening metrics server listener: bind: address already in use"),
    "tunnel-not-found": (1, "Expected to find a single tunnel with uuid 00000000-0000-4000-8000-000000000001 but found 0 tunnels"),
    "nonzero": (7, "synthetic command failed"),
}
if mode in errors:
    code, message = errors[mode]
    print(message, file=sys.stderr)
    sys.exit(code)
if mode == "malformed-json":
    print('{"synthetic":'); sys.exit(0)
if mode == "outdated":
    log("warn", "Your version 2026.9.1 is outdated. We recommend upgrading it to 2026.10.0")
if args == ["--version"]:
    print("cloudflared version 2026.9.1 (built synthetic)"); sys.exit(0)
ident = "00000000-0000-4000-8000-000000000001"
rest = args[1:] if args[:1] == ["tunnel"] else args[:]
while rest and rest[0].startswith("--"):
    rest = rest[1:] if "=" in rest[0] or rest[0] in ("--no-autoupdate", "--hello-world") else rest[2:]
command = rest[0] if rest else None
if command == "list":
    print(json.dumps([{"id": ident, "name": "synthetic", "created_at": "2026-10-10T06:00:00Z", "deleted_at": "0001-01-01T00:00:00Z", "connections": []}]))
    sys.exit(0)
if command == "info":
    print(json.dumps({"id": ident, "name": "synthetic", "createdAt": "2026-10-10T06:00:00Z", "connectors": []}))
    sys.exit(0)
if command == "create":
    print(json.dumps({"AccountTag": "synthetic-account", "TunnelSecret": "synthetic-create-secret", "TunnelID": ident}))
    sys.exit(0)
if command == "route" and rest[1:2] == ["dns"]:
    log("info", "synthetic.example.com is already configured to route to your tunnel" if mode == "already-routed" else "Added CNAME synthetic.example.com which will route to this tunnel")
    sys.exit(0)
if command == "login":
    print("Please open the following URL and log in with your Cloudflare account:\nhttps://example.invalid/synthetic-login")
    sys.exit(0)
if command == "ingress" and rest[1:2] == ["rule"]:
    if mode == "ingress-slow": time.sleep(0.8)
    if mode == "ingress-aba":
        open(os.path.join(here, "ready"), "w").close()
        deadline = time.time() + 3
        while not os.path.exists(os.path.join(here, "proceed")) and time.time() < deadline: time.sleep(0.01)
    if mode == "ingress-malformed": print("Matched rule #bogus"); sys.exit(0)
    if mode == "ingress-out-of-range": print("Matched rule #999"); sys.exit(0)
    if mode == "ingress-invalid": print("Validation failed: synthetic invalid ingress", file=sys.stderr); sys.exit(1)
    target = args[args.index("--config") + 1]
    # Restricted fixture YAML, not a replacement cloudflared parser.
    rules = []
    for line in open(target):
        field = line.strip()
        if field.startswith("- "):
            rules.append({}); field = field[2:]
        if rules and ":" in field:
            key, value = field.split(":", 1)
            if key in ("hostname", "path", "service"):
                rules[-1][key] = value.strip().strip("\"'")
    if not rules or rules[-1].get("hostname") or rules[-1].get("path"):
        print("Validation failed: last ingress rule must match all URLs", file=sys.stderr); sys.exit(1)
    if mode == "ingress-echo": print(open(target).read())
    if mode == "ingress-aba":
        open(os.path.join(here, "read"), "w").close()
        deadline = time.time() + 3
        while not os.path.exists(os.path.join(here, "restored")) and time.time() < deadline: time.sleep(0.01)
    url = urlsplit(rest[2])
    for index, rule in enumerate(rules):
        host = rule.get("hostname", "")
        match_host = not host or host == url.hostname or (host.startswith("*.") and url.hostname.endswith(host[1:]))
        if match_host and (not rule.get("path") or re.search(rule["path"], url.path or "/")):
            print("Using rules from " + target)
            print("Matched rule #%d" % index)
            for key, value in rule.items(): print("\t" + key + ": " + value)
            sys.exit(0)
    print("No match", file=sys.stderr); sys.exit(1)
if "ingress" in args:
    print("OK"); sys.exit(0)
if mode == "crash":
    log("error", "failed to connect to the edge"); sys.exit(1)
if mode == "token-echo":
    log("info", "synthetic child echoed " + os.environ.get("TUNNEL_TOKEN", ""))
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        if self.path == "/ready":
            body = {"status": 200, "readyConnections": 1}; code = 200
        elif self.path == "/quicktunnel":
            body = {"hostname": "fake-words-here.trycloudflare.com"}; code = 200
        else:
            body = {}; code = 404
        data = json.dumps(body).encode()
        self.send_response(code); self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data)
srv = HTTPServer(("127.0.0.1", 0), H)
threading.Thread(target=srv.serve_forever, daemon=True).start()
def term(*a):
    log("info", "Initiating graceful shutdown due to signal terminated ...")
    os._exit(0)
signal.signal(signal.SIGTERM, term)
log("info", "Starting tunnel")
log("info", "Starting metrics server on 127.0.0.1:%d/metrics" % srv.server_address[1])
while True:
    time.sleep(0.2)
