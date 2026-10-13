#!/usr/bin/env python3
"""Offline setup contract: only invented accounts, certificates and remote identities."""
import base64, json, os, sys, time
from pathlib import Path

here = Path(__file__).resolve().parent
mode = (here / "mode").read_text().strip()
args = sys.argv[1:]
with (here / "calls.jsonl").open("a") as calls:
    calls.write(json.dumps(args) + "\n")
assert args[:2] == ["tunnel", "--config"]
assert Path(args[2]).name == "neutral.yml"
assert args[3] == "--origincert"
cert = Path(args[4])
command = args[5]
remote_path = here / "remote.json"
remote = json.loads(remote_path.read_text()) if remote_path.exists() else []
ident = "00000000-0000-4000-8000-000000000001"

if command == "login":
    print("https://example.invalid/login?callback=invented-login-secret")
    if mode == "login-zero-no-cert":
        sys.exit(0)
    cert.parent.mkdir(parents=True, exist_ok=True)
    if mode == "login-zero-invalid-cert":
        cert.write_text("invented-invalid-certificate-token")
    else:
        body = base64.b64encode(json.dumps({"accountID": "synthetic-account", "zoneID": "synthetic-zone", "apiToken": "synthetic-api-token"}).encode()).decode()
        with cert.open("x") as output:
            output.write("-----BEGIN ARGO TUNNEL TOKEN-----\n" + body + "\n-----END ARGO TUNNEL TOKEN-----\n")
        cert.chmod(0o600)
    sys.exit(0)

if command == "list":
    assert args[6:] == ["--output", "json"]
    if mode == "auth-fail":
        print("authentication failed: status 403", file=sys.stderr)
        sys.exit(1)
    if mode == "malformed-list":
        print('invented-list-secret malformed json')
        sys.exit(0)
    if mode == "existing-name" and not remote:
        remote = [{"id": ident, "name": "Test project", "deleted_at": "0001-01-01T00:00:00Z"}]
    print(json.dumps(remote))
    sys.exit(0)

assert command == "create"
assert args[6] == "--credentials-file"
assert args[8:10] == ["--output", "json"]
assert args[10] == "--"
assert len(args) == 12
credentials = Path(args[7])
if any(item["name"] == args[11] for item in remote):
    print("tunnel with name already exists", file=sys.stderr)
    sys.exit(1)
remote.append({"id": ident, "name": args[11], "deleted_at": "0001-01-01T00:00:00Z"})
remote_path.write_text(json.dumps(remote))
if mode not in ("create-hang-no-file", "create-fail-no-file"):
    payload = {"AccountTag": "other-account" if mode == "wrong-account" else "synthetic-account", "TunnelSecret": base64.b64encode(bytes(32)).decode(), "TunnelID": ident}
    with credentials.open("x") as output:
        output.write('invented-malformed-secret {' if mode == "malformed-credentials" else json.dumps(payload))
    credentials.chmod(0o400)
(here / "created").write_text("ready")
if mode in ("create-hang", "create-hang-no-file"):
    while True:
        time.sleep(0.05)
if mode in ("create-fail-file", "create-fail-no-file"):
    print("invented-create-secret", file=sys.stdout)
    sys.exit(1)
print(json.dumps(remote[-1]))
