# Research

Working name: **Underlane**, a menu bar app that supervises `cloudflared` tunnels.

Everything here about `cloudflared` was checked against **cloudflared 2026.9.1** on this Mac
(`/opt/homebrew/bin/cloudflared`), either with `--help` or by running it. Where a claim comes from
Cloudflare's docs instead, the source is linked. Raw help output was captured on 2026-10-09.

---

## 1. Stack

### What polished tray utilities are built with (2026)

| App | What it is | Stack |
| --- | --- | --- |
| [Ports](https://www.ports-app.com/) | Lists localhost dev servers from the menu bar, macOS 14+ | Not published. The UI is macOS-native in look (Activity Monitor-style energy badge). Most likely Swift/AppKit, but unconfirmed. |
| [Port Pilot](https://flaviocopes.com/software/port-pilot/) | Menu bar control panel for local services | SwiftUI menu bar app, plus a Node/Ink terminal dashboard |
| LocalhostBar, OpenPorts, PortBar, Berth | Small port and dev-server menu bar tools | Mostly Swift/SwiftUI (macOS-only) |
| [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev), Clash Nyanpasu | Tray GUIs that supervise a proxy core binary (mihomo) | **Tauri** (Rust + web UI). They run the core as a [sidecar process](https://deepwiki.com/libnyanpasu/clash-nyanpasu/3.4-sidecar-process-management) on Windows/macOS/Linux. |
| Docker Desktop, 1Password (older), Slack | Tray plus heavy windows | Electron |

Takeaway: macOS-only menu bar utilities are almost always Swift. Cross-platform tray apps that
**supervise an external binary** (the same shape as this app) tend to use Tauri.

### Options scored (1 = poor, 5 = excellent)

| Criterion | Tauri 2 (Rust + WKWebView) | Swift/SwiftUI + AppKit | Electron | Wails v3 (Go) |
| --- | --- | --- | --- | --- |
| Tray integration: template icon, count text, popover vs. menu | **4.** `TrayIconBuilder::icon_as_template`, `set_title` shows text beside the icon on macOS (not shown on Windows; [v2 tray API](https://v2.tauri.app/reference/javascript/api/namespacetray/)). A popover is a borderless window placed with `tauri-plugin-positioner` `TrayCenter`. Native menus are also available. | **5.** `NSStatusItem` + `NSPopover`, fully native. `MenuBarExtra(.window)` is simpler but [lacks access to the status item and window](https://swiftpackageregistry.com/orchetect/MenuBarExtraAccess), so real apps [drop down to NSStatusItem](https://multi.app/blog/pushing-the-limits-nsstatusitem). | **4.** `Tray` with template images and `setTitle` on macOS. The popover is a frameless window you position yourself. | **3.** v3 has a system tray with popup-window support, but v3 is still beta ([v3.0.0-beta.2, 2026-08](https://docs-preview.wails.io/changelog/)), with recent tray crash fixes on Windows/Linux. |
| Run and supervise child processes, capture stdout/stderr | **5.** `tokio::process` gives async pipes, kill-on-drop, exit status, process groups. The inspecting proxy (hyper) can live in the same Rust core. | **4.** `Process` + `Pipe` work. A streaming HTTP proxy needs SwiftNIO (heavier, but solid). | **4.** Node `child_process` is easy, and Node's `http` can proxy, but everything competes with the UI event loop. | **5.** Go `os/exec` and `net/http/httputil.ReverseProxy` are excellent here. |
| Login item / auto-start | **4.** `tauri-plugin-autostart` uses a LaunchAgent plist or AppleScript on macOS ([plugin docs](https://v2.tauri.app/plugin/autostart)) and the registry or XDG on Windows/Linux. The modern `SMAppService.mainApp` is reachable through [`objc2-service-management`](https://docs.rs/crate/objc2-service-management/latest) (0.3.x) or [`smappservice-rs`](https://docs.rs/smappservice-rs). | **5.** `SMAppService.mainApp.register()`, [macOS 13+](https://developer.apple.com/documentation/servicemanagement/smappservice/mainapp.md). | **5.** `app.setLoginItemSettings` defaults to `type: 'mainAppService'` on macOS ([docs](https://www.electronjs.org/docs/latest/api/app)). | **3.** No first-party API, so ServiceManagement needs cgo bindings. |
| Animation and theming | **5.** CSS transitions, Svelte `animate:flip`/`transition:`, `prefers-color-scheme`, `prefers-reduced-motion`. | **5.** SwiftUI animations, `accessibilityReduceMotion`. | **5.** Same as Tauri (Chromium). | **5.** Same as Tauri (webview). |
| Idle memory | **3.** Reported ~30–50 MB for small apps ([comparison](https://devtoolpicks.com/blog/tauri-vs-electron-indie-hackers-2026)), but every WKWebView window gets its own WebContent, Networking and GPU helper processes, so a live popover plus a dashboard costs more. [Measurement caveats](https://github.com/tauri-apps/tauri/issues/5889) apply. Mitigation: destroy the dashboard window on close and build the popover lazily. | **5.** Typically 15–40 MB. Lightest. | **2.** 150–300 MB reported, because it bundles Chromium + Node (same sources; numbers depend on whether shared memory is counted). | **4.** Similar to Tauri (system webview). |
| Signing / notarization effort | **4.** `tauri build` signs and notarizes given `APPLE_SIGNING_IDENTITY` and notary credentials. One binary, no helpers. | **5.** Xcode does it end to end. | **3.** Works via electron-builder, but the Electron helper apps and entitlements make it fiddlier. | **3.** Manual `codesign`/`notarytool` scripting. |
| Cross-platform reach | **5.** macOS, Windows, Linux from one codebase. | **1.** macOS only. A Windows/Linux port is a rewrite. | **5.** All three. | **4.** All three (beta). |
| **Total (of 35)** | **30** | **30** | **28** | **27** |

### Recommendation

**Tauri 2.** Tauri and Swift tie on the raw sum (30–30), so the sum doesn't decide it. The weighting
does. The deciding factors:

1. The owner plans to ship Windows and Linux later. A Swift app would have to be rewritten. With
   Tauri the core (supervision, config parsing, inspecting proxy, SQLite) is a plain Rust crate
   that already builds everywhere, and the web UI ports as-is.
2. The heaviest part of the app is not the UI. It is the streaming HTTP/WebSocket reverse proxy
   and the process supervisor. Rust (tokio + hyper) is a first-class fit for both and runs
   in-process with no extra runtime.
3. The macOS experience stays close to native. Template icon, a count beside the icon via
   `set_title`, a popover anchored to the icon, `SMAppService` through objc2 bindings, and no Dock
   icon (`ActivationPolicy::Accessory`).

On macOS polish alone, Swift would win. A hybrid (a SwiftUI shell over a shared Rust core via
UniFFI) was also considered and rejected for v1: it means two UI codebases as soon as Windows/Linux
ship. Future platforms need two adjustments. Windows has no tray title, and Linux gets no tray
click events (menu only), so the count will be drawn into the icon there, and the native tray menu
is kept complete. Electron loses on idle memory
for an always-on utility. Wails v3 is still in beta.

---

## 2. cloudflared integration

### 2.1 Running the three tunnel kinds

| Kind | Command (verified with `--help`) | Notes |
| --- | --- | --- |
| **Named, from a config file** | `cloudflared tunnel --config <file> run [TUNNEL]` | The tunnel name or UUID comes from the `tunnel:` key or the positional argument. Needs the credentials JSON (`credentials-file:`) but not `cert.pem` when identified by UUID ("does not need access to cert.pem … if you identify the tunnel by UUID", from `tunnel run --help`). |
| **Token-based (remotely managed)** | cloudflared supports `--token <TOKEN>`, `--token-file <path>` and `TUNNEL_TOKEN` env. Underlane stores the token in Keychain and runs `cloudflared tunnel --config <app-owned neutral.yml> run` with `TUNNEL_TOKEN` env, keeping the token out of argv and token files. | Ingress is configured in the dashboard and pushed to the connector. Local ingress cannot be rewritten (see §3). |
| **Quick tunnel** | `cloudflared tunnel --url http://127.0.0.1:PORT` | Prints `https://<random>.trycloudflare.com`. Verified live: the URL served HTTP 200. [Documented limits](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/): 200 in-flight requests (then 429), **no SSE**, no uptime guarantee, a new hostname each run. |

Gotchas found in testing:

- A quick tunnel still searches for a default `config.yml` in `~/.cloudflared` and friends (log line:
  "Cannot determine default configuration path. No file [config.yml config.yaml] in
  [~/.cloudflared …]"). A user's default config could change what a quick tunnel does. **Fix: pass
  `--config <app-owned minimal yml>`.** An *empty* file logs an `error` ("Configuration file … was
  empty") but works. A file containing just `no-autoupdate: true` is clean. Verified both.
- The app always passes `--no-autoupdate` because Homebrew manages the binary. cloudflared prints
  "will not automatically update if installed by a package manager" anyway.
- `SIGTERM` starts a graceful shutdown ("Initiating graceful shutdown due to signal terminated"),
  bounded by `--grace-period` (default 30s). A second signal forces exit. The app sends SIGTERM,
  waits a few seconds, then SIGKILL.
- `cloudflared` writes logs to **stderr**.

### 2.2 Health detection

`--metrics <addr>` defaults to the first free port in `localhost:20241-20245`. The app passes an
explicit `127.0.0.1:<free port>` per tunnel so it always knows where to look. Endpoints, verified
live on 2026.9.1:

| Endpoint | Exists? | Response (observed) |
| --- | --- | --- |
| `/ready` | yes | `200 {"status":200,"readyConnections":1,"connectorId":"…"}`. Returns 503 when no edge connection is registered. `cloudflared tunnel ready` wraps it. |
| `/healthcheck` | yes | `200 OK` (process liveness only) |
| `/quicktunnel` | yes | `200 {"hostname":"synthetic-quick-3.trycloudflare.com"}`. This is how the app learns the public URL without scraping the log banner. |
| `/metrics` | yes | Prometheus text: `cloudflared_tunnel_ha_connections`, `cloudflared_tunnel_total_requests`, `cloudflared_tunnel_request_errors`, `cloudflared_tunnel_concurrent_requests_per_tunnel`, `cloudflared_tunnel_server_locations{edge_location=…}`, `build_info{version=…}`. These match the [metrics docs](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/monitor-tunnels/metrics/). |
| `/config` | yes | `{"version":-1,"config":{"ingress":[…]}}`, the **effective** ingress, including remotely managed rules. The app uses it to show hostname → service mappings for token tunnels. |
| `/diag/tunnel` | yes | `{"tunnelID":…,"connectorID":…,"connections":[{"isConnected":true,"edgeAddress":…}]}` |

Status model built on these (decided in decisions.md §5): process spawned, `/ready` not yet 200 →
**starting**. `/ready` 200 with the expected connection count → **connected**. Alive but `/ready` 503,
or fewer HA connections than expected after having been connected → **degraded**. Unexpected
exit → **crashed**. Exit requested by the user → **stopped**.

### 2.3 Structured logs

- `--loglevel {debug, info, warn, error, fatal}` (default `info`).
- `--output json` (or `TUNNEL_LOG_OUTPUT=json`) emits one JSON object per line on stderr, always with
  `level`, `message`, `time`, plus context fields (`connIndex`, `event`, `ip`, `location`,
  `error`, …). Verified.
- `--logfile <file>` / `--log-directory <dir>` additionally write to disk. The app does **not** use
  these. It reads the stderr pipe and owns retention in SQLite, so files don't pile up.
- **At `debug` level, every proxied request is logged with method, URL, `content-length`,
  `ingressRule`, and all request headers** (verified: `"headers":{"Accept":["*/*"],"Cf-Connecting-Ip":[…],…}`),
  which matches the warning in `--help`. **Bodies are never logged.** The app defaults to `info`. If a
  user picks `debug`, the redaction list is applied to the `headers` field before the line is stored.

### 2.4 `cloudflared tail` and the management API

`cloudflared tail [TUNNEL]` streams logs from a *remote* connector through Cloudflare's management
service ([docs](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/monitor-tunnels/logs/)).

What it can do:
- Filter by `--event` (`cloudflared`, `http`, `tcp`, `udp`), `--level`, `--sample`, and
  `--connector-id`, with JSON output.
- Watch a connector running on another machine.

What it can't do:
- It needs an account and auth (`cloudflared tunnel login` / `--token` / `cert.pem`) and "edit
  permissions for Cloudflare Tunnel". **Quick tunnels have no account, so tail is unusable there.**
- Sessions are held open for at most one hour. Under load Cloudflare "prioritizes service
  stability over log delivery", so events are dropped.
- **No request or response bodies.** HTTP events carry the same metadata as local debug logs.

Conclusion: for processes the app runs itself, reading the local stderr JSON stream is strictly
better (complete, no account, no drops). `tail` adds nothing for v1.

### 2.5 Validation

`cloudflared tunnel --config <file> ingress validate` exits 0 and prints `OK`, or exits 1 and
prints `Validation failed: <reason>`. Reasons observed:

- "The last ingress rule must match all URLs (i.e. it should not have a hostname or path filter)"
- "localhost:4000 is an invalid address, please make sure it has a scheme and a hostname"
- "Rule #1 is matching the hostname '', but this will match every hostname, meaning the rules which follow it will never be triggered."

`ingress rule <URL>` reports which rule matches (`Matched rule #1 …`). The app maps the validation
messages to plain-language hints (decisions.md §8).

---

## 3. Request/response body capture

### 3.1 Confirming cloudflared doesn't expose bodies

- `--help` for `tunnel`, `tunnel run`, and `tail` has no body or payload option.
- Debug logs carry headers and `content-length` only (verified above).
- `tail`'s HTTP events are the same metadata.
- The metrics server exposes counters, not payloads.

So capture has to happen on the path between cloudflared and the origin.

### 3.2 Design: an inspecting reverse proxy owned by the app

```
edge ─▶ cloudflared ──(derived config: service = http://127.0.0.1:P_i)──▶ Underlane proxy :P_i ──▶ real origin (http/https)
                                                                              │
                                                                              └─▶ capture → SQLite (requests table)
```

- **One listener per inspected ingress rule.** Each rule can point at a different origin, so each
  rule with an `http://` or `https://` service gets its own `127.0.0.1:<ephemeral>` listener that
  knows exactly one upstream. No routing table, no Host-header guessing.
- **Derived config.** The original YAML is parsed and re-emitted to
  `<app data>/derived/<tunnel-id>.yml`. Only `ingress[i].service` changes, and only for
  inspectable rules. Non-HTTP services (`ssh://`, `tcp://`, `unix:`, `http_status:`,
  `hello_world`, `bastion`) are left as they were. The **user's file is opened read-only and never
  written**. `credentials-file` and other relative paths are rewritten to absolute ones,
  resolved against the original file's directory, because the derived file lives elsewhere.
- **Quick tunnels:** `--url http://127.0.0.1:<proxy port>` instead of the real origin.
- **Token tunnels:** ingress comes from the dashboard and is pushed to the connector at runtime,
  so there's no local file to derive. Inspection is unavailable for token tunnels, and the UI
  says so. This is a recorded limitation (decisions.md §4).

### 3.3 Streaming and chunked bodies

- The proxy (hyper 1.x) **streams frames straight through**. Each body frame is forwarded as it
  arrives and, in parallel, appended to a capture buffer until it reaches the per-body cap (default
  1 MB). Past the cap, frames still flow and the record is marked `truncated = true`. Nothing is
  buffered before forwarding, so time to first byte and memory per request stay flat.
- Chunked transfer encoding is handled by hyper on both legs. `Content-Length` is preserved when
  the origin sends it.
- The record is written when the response body finishes. If either side aborts, the record is
  written with an `error` note and whatever was captured.

### 3.3.1 Compressed bodies

The edge adds `Accept-Encoding: gzip` to origin requests even when the client didn't send it
(observed). Real origins therefore often answer with gzip or br. The proxy stores the **raw** bytes
plus `Content-Encoding`. The UI decodes gzip, deflate and br at display time with a streaming
decoder that tolerates truncation: a 1 MB-truncated gzip stream still shows its decodable prefix.
The UI shows a "decoded" or "truncated" badge, and a test covers a gzip origin.

### 3.3.2 Memory and back-pressure

Captured records go to a **bounded** channel feeding one batched SQLite writer. When the writer
falls behind, records are dropped (traffic never waits on storage) and the drop count is shown in
the UI. A global in-flight budget (64 MB of body bytes across all open exchanges) stops storing
bytes past the budget, and those records are marked truncated. This bounds memory even at the
quick-tunnel limit of 200 concurrent requests.

### 3.4 WebSockets and SSE

- **WebSocket:** an `Upgrade: websocket` request is forwarded with its upgrade headers. On `101`
  the proxy joins both upgraded connections with `tokio::io::copy_bidirectional`. Recorded:
  method, path, status 101, request headers, duration until close, bytes each way. No frames.
- **SSE:** a response with `Content-Type: text/event-stream` is streamed through without capture.
  Only metadata is recorded. Note that quick tunnels don't support SSE at all (Cloudflare's docs).
- Other long-lived streaming responses are still bounded by the 1 MB cap, so memory stays bounded.

### 3.5 HTTPS origins and `originRequest`

With inspection on, cloudflared connects over plain HTTP to the loopback proxy, and the proxy makes
the upstream connection. TLS-related `originRequest` options therefore move from cloudflared to the
proxy:

| `originRequest` key | Applied by |
| --- | --- |
| `noTLSVerify` | proxy (rustls with a no-op verifier) |
| `originServerName` | proxy (SNI and certificate name) |
| `caPool` | proxy (extra root certificates) |
| `httpHostHeader` | cloudflared still sets it on the request to the proxy. The proxy forwards Host unchanged. |
| `connectTimeout` | proxy (TCP connect deadline to the origin); also kept for cloudflared → proxy |
| `disableChunkedEncoding`, `keepAlive*`, `tcpKeepAlive`, `noHappyEyeballs`, `proxyType`, `access` | stay in the derived config for cloudflared → proxy |
| `tlsTimeout` | proxy (its own deadline on the TLS handshake, started after the TCP connection is up) |
| `http2Origin: true` | **rule is not inspected**: it keeps its original service, and the UI says why. gRPC and h2-only origins need h2 and trailers end to end. Translating them isn't worth the risk in v1. |
| `matchSNItoHost: true` | **rule is not inspected**: per-request SNI from Host can't be expressed with a pooled upstream connector without changing behavior. |

The proxy always speaks **HTTP/1.1 upstream**, exactly like cloudflared does when `http2Origin` is
false, so the WebSocket `Upgrade`/`101` path and Host-based routing behave the same. Top-level
`originRequest` values are merged with rule-level ones (the rule wins) before being applied.
`keepAlive*`, `tcpKeepAlive` and `noHappyEyeballs` now only govern the loopback hop. The proxy's
upstream client uses its own defaults: a 90 s idle pool and happy-eyeballs fallback. This is
documented as a known difference.

Services: `http://`, `https://`, `ws://` and `wss://` are inspectable (`ws`/`wss` map to
`http`/`https`). `unix:` / `unix+tls:` sockets are not inspected in v1 and keep their original
service. Non-inspected rules keep their `originRequest` exactly as written.

**Relative paths.** cloudflared resolves a relative `credentials-file` (and other paths) against
the **process working directory**, not the YAML file's directory (verified by the reviewers against
cloudflared source and behavior). Underlane always spawns cloudflared, for validation and every run,
with its working directory set to the **original config's directory**. It also writes absolute
paths, resolved against that same directory, into the derived file. The original and derived
configs therefore resolve to identical files. A preflight check shows a clear error if the
credentials file doesn't exist, or if its `TunnelID` doesn't match a UUID in `tunnel:`.
`ingress validate` checks neither.

### 3.6 Added latency

One loopback hop with no buffering: an extra TCP connect on the first request (connections to the
origin are pooled), plus copying each frame. Expected cost: well under 1 ms per request on small
bodies, which is negligible next to the edge round trip of ~20–80 ms. The measured number is in
§3.6.1 once the proxy exists. Because inspection is per tunnel, tunnels that don't need it pay
nothing.

#### 3.6.1 Measured

Measured with `cargo test --release -p underlane-core --test proxy_capture -- --ignored --nocapture`
(`bench_added_latency`): 1,000 sequential POSTs with a small JSON body after a 200-request warm-up,
on this Mac (Apple Silicon, macOS 27), loopback origin, capture on.

| | p50 | p99 |
| --- | --- | --- |
| direct to origin | 0.032 ms | 0.127 ms |
| through the inspecting proxy | 0.060 ms | 0.095 ms |
| **added by the proxy** | **0.027 ms** | — |

That's three orders of magnitude below the edge round trip.

### 3.7 If the proxy crashes

The proxy runs inside the app process as tokio tasks:

- **A panic while handling one request** stays in that connection's task, because the release
  profile keeps `panic = "unwind"` (never `abort`). The connection drops, cloudflared answers that
  one request with its own 502, and the listener keeps serving.
- **A listener that dies** (e.g. accept error) is restarted by the tunnel supervisor on the same
  port. cloudflared retries the origin, so clients see 502s for the gap.
- **If the app process dies** (crash, force-quit, SIGKILL), the proxy is gone. On macOS, child
  processes do *not* die with their parent, so each cloudflared is launched through a tiny
  **watchdog shim**: Underlane's own binary re-executed as `underlane --cloudflared-shim`. The shim
  spawns cloudflared, holds the read end of a pipe from the app on its stdin, and on EOF (the app
  died) sends SIGTERM to cloudflared, then SIGKILL after 5 s. Tunnels therefore go down within
  seconds of an app crash rather than serving 502s indefinitely. As a second line of defence, the
  next launch kills leftovers found by pidfile. Every launch mode, including uninspected named
  tunnels, runs from a config file inside Underlane's data dir, so the command line always carries a
  Underlane marker that identifies the process.
- **Fail-open option:** if a listener can't be rebound, the supervisor restarts that tunnel with
  the original config (inspection off) and shows a warning. It favors traffic over capture.

---

## 4. Process model

| | Children of the app | launchd agents (`cloudflared service install`, or one plist per tunnel) |
| --- | --- | --- |
| App quit | Tunnels stop | Tunnels keep running |
| App crash | Children must be reaped. On macOS a child does **not** die with its parent. Handled with pidfiles plus cleanup on the next launch. | Unaffected |
| Log capture | Direct stderr pipe, real time | launchd writes to files, which must be tailed |
| Inspection proxy | Same lifetime as the tunnels it fronts | The proxy lives in the app, so inspected traffic 502s whenever the app isn't running. A separate proxy daemon would double the components. |
| Restart with backoff | In-app supervisor, with full control and status | `KeepAlive` in launchd, with coarser control and status only from outside |
| Footprint on the system | Nothing installed outside the app's data dir | Plists in `~/Library/LaunchAgents`, plus cleanup when tunnels are deleted |
| Cross-platform | Same code on Windows/Linux | launchd vs. Windows services vs. systemd: three backends |

**Choice: children of the app**, with "launch at login" and per-tunnel "start automatically" so
tunnels come back without a terminal or a manual step. Rationale is in decisions.md §3.
