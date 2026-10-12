# Underlane

A macOS menu bar app that runs your `cloudflared` tunnels for you. Set them up once, then stop
thinking about them: no terminal windows, no juggling `config.yml` files.

![Underlane dashboard with the Lane Cut logo](docs/screenshots/lane-cut-dashboard-dark.png)

- **Import what you already have.** Underlane scans `~/.cloudflared` for config files, and you can
  add files or project folders. It also supports token (dashboard-managed) tunnels and
  `trycloudflare.com` quick tunnels.
- **One click per tunnel**, or all at once, with live status: starting, connected, degraded,
  stopped or crashed. Crashed tunnels restart with backoff.
- **A count in the menu bar** shows how many tunnels are up. The popover can copy a tunnel's public
  URL or open it in the browser.
- **The dashboard** has a log viewer with level filters and search. For tunnels with inspection
  on, it also has a request inspector with headers, pretty-printed JSON, a hex view and
  "copy as curl".
- **Launch at login** (native `SMAppService`) and per-tunnel **start automatically**.
- **Your config files are never modified.** Request inspection uses a derived copy in Underlane's data
  folder.

Underlane drives the `cloudflared` binary; it does not reimplement the tunnel protocol. It isn't
affiliated with Cloudflare.

## Why "Underlane"

A tunnel is a lane that runs under everything else. That's what `cloudflared` gives you: your
Mac dials out to Cloudflare's edge, and traffic for your public hostname comes back along that
connection to `localhost`. No port is opened on your router and no firewall rule is added. The
traffic goes underneath them. Underlane is the app that keeps those lanes open, quietly, from
the menu bar, so you don't have to think about them.

The name is also easy to say and to spell: two everyday words, no doubled letters, nothing to
guess. It deliberately leaves out "Cloudflare" and "cloudflared", because the app isn't an
official Cloudflare product and shouldn't look like one.

It was called Burrow at first. That name was already taken by other open-source projects,
including another Mac app, so it was renamed before the first release.

## Setup

Requirements: macOS 13.3 or later (built and tested on macOS 27, Apple Silicon), plus:

```sh
brew install cloudflared   # Underlane finds it on PATH or in Homebrew's folders
```

To build from source you also need Xcode command-line tools, Node 20+ and Rust (via
[rustup](https://rustup.rs)).

```sh
make build      # npm ci + tauri build -> target/release/bundle/macos/Underlane.app
make install    # same, then copies it to ~/Applications (needed for launch at login)
```

Open `Underlane.app`. On first run it opens the dashboard. Choose **Add tunnel** to import config
files, paste a tunnel token, or share a local port as a quick tunnel.

If you use a menu bar manager (Hidden Bar, Ice, Bartender), new items may land in its hidden
section. Drag Underlane's circular Lane Cut icon into the visible part.

## Dev loop

```sh
make dev        # Vite + cargo with hot reload (frontend) and rebuild-on-change (Rust)
make test       # cargo test --workspace, vitest, svelte-check
make e2e        # end-to-end check against a real quick tunnel (no Cloudflare account needed)
```

Useful environment variables:

| Variable | Effect |
| --- | --- |
| `UNDERLANE_DATA_DIR` | Use another data folder (tests, experiments). It also disables the single-instance lock. |
| `UNDERLANE_LOG` | Log filter for the app's own stdout logs, e.g. `debug` or `underlane_core=debug`. |
| `UNDERLANE_E2E_PORT` | Start the loopback control server. Only in builds with `--features e2e`; release builds don't contain it. |

`Underlane --login-item enable|disable|status`, run from the app bundle, manages launch at login from
a script.

### Tests

- `crates/underlane-core/tests/` holds the integration tests:
  - config parsing and folder scanning
  - derived configs (only inspected services change, paths become absolute, the original stays
    untouched)
  - the proxy's capture, redaction (headers and query parameters), truncation of
    streaming/chunked bodies, gzip, SSE, WebSocket, and upstream errors
  - TTL and size-cap expiry
  - log parsing and validation hints
  - the supervisor against a fake `cloudflared`: connect, crash with backoff, a derived config run
    from the config directory, the watchdog shim, and orphan reaping
- `src/lib/*.test.ts` (Vitest) cover the UI's pure helpers: `format` (copy as curl, hex dump, JSON
  detection), `listnav` (list keyboard navigation), `latest` (debounced latest-only async jobs),
  `combobox` (the project picker's rows), `focus` (returning focus when a dialog closes) and `utils`
  (class merging with the custom text sizes).
- `e2e/run.mjs` launches the real app with an isolated data folder and a local echo server. It
  exposes the echo server as a quick tunnel with inspection on, POSTs JSON to the
  `trycloudflare.com` URL, and asserts:
  - the request and response bodies are in SQLite and visible in the dashboard's request viewer
  - the menu bar count, read from the real `NSStatusItem`, goes 0 → 1 → 0

  Evidence lands in `e2e/out/`.
- `cargo test --release -p underlane-core --test proxy_capture -- --ignored --nocapture` measures the
  latency the proxy adds (about 0.03 ms at p50 on loopback).

## Architecture

```
┌──────────────────────────── Underlane.app (one process) ─────────────────────────────┐
│  WKWebView: popover.html + index.html (Svelte 5)        tray icon (NSStatusItem)   │
│        │  invoke / events                                    ▲ count              │
│  src-tauri  (commands, tray & windows, SMAppService, App Nap, e2e control server)  │
│        │                                                                          │
│  underlane-core (plain Rust, no Tauri)                                               │
│   ├─ supervisor ── spawns ──▶ Underlane --cloudflared-shim ──▶ cloudflared tunnel …  │
│   │      │  stderr JSON logs, /ready /quicktunnel /config polling, backoff        │
│   ├─ proxy (hyper) ◀── cloudflared (derived config: service → 127.0.0.1:port)     │
│   │      └──▶ real origin (http/https, originRequest TLS options)                 │
│   ├─ config / derived / import    (read-only on your YAML)                        │
│   └─ db (SQLite: settings, projects, tunnels, logs, requests; TTL + size cap)     │
└───────────────────────────────────────────────────────────────────────────────────┘
```

- **`crates/underlane-core`** contains everything platform-independent:
  - `config.rs` parses cloudflared YAML.
  - `derived.rs` decides which rules can be inspected and writes the derived copy.
  - `proxy/` is the inspecting reverse proxy. It streams frames through untouched while copying up
    to the cap, redacts before storage, and passes WebSocket and SSE through.
  - `supervisor.rs` handles process lifecycle, health, restarts, orphan reaping and detecting the
    same tunnel running outside Underlane.
  - `db.rs` is the SQLite layer.
  - `shim.rs` is the watchdog that kills cloudflared if the app dies.
  - `decode.rs` handles display-time gzip/br/deflate decoding.
- **`src-tauri`** is the app shell:
  - `state.rs` wires the supervisor to SQLite (a bounded, batched writer) and to the UI (events).
  - `tray.rs` builds the menu bar item, the popover (vibrancy, positioned under the icon) and the
    dashboard window.
  - `platform.rs` covers `SMAppService`, App Nap and state restoration.
  - `commands.rs` holds the UI commands.
  - `e2e.rs` is the test-only control server.
- **`src/`** is the Svelte 5 UI, styled with Tailwind v4 and shadcn-svelte (Luma style, tuned to
  macOS). `popover/` is the menu bar panel. `dashboard/` has the Tunnels, Requests, Logs and
  Settings views. `lib/` holds the API, store and formatting helpers, and `lib/components/` the
  components: `ui/` are the shadcn-svelte copies (with Underlane's macOS edits), `app/` are Underlane's
  own (StatusDot, Toolbar, Tip, Combobox, …). `styles/app.css` is the only stylesheet: the design
  tokens for light and dark, and the base rules.

Where state lives: `~/Library/Application Support/com.davidsonsns.underlane/`.

| Path | Contents |
| --- | --- |
| `underlane.db` | SQLite: settings, tunnels, logs, captured requests |
| `derived/` | Generated configs |
| `run/` | pidfiles |
| `neutral.yml` | Keeps quick and token tunnels from loading a default config |

Tokens for token tunnels live in the macOS Keychain.

Why it's built this way (stack, process model, capture design, trade-offs):
[`docs/research.md`](docs/research.md) and [`docs/decisions.md`](docs/decisions.md). The manual
verification log with screenshots is in [`docs/verification.md`](docs/verification.md).

## Limitations (v1)

- Tunnels run as children of the app, so quitting Underlane stops them. Use launch at login plus
  "start automatically", or `cloudflared service install` for tunnels that must survive without
  the app.
- Inspection isn't available for token tunnels, for rules with `http2Origin`/`matchSNItoHost`, or
  for `unix:` sockets. Those rules route directly, and the UI says why.
- Quick tunnels get a new hostname on every start and don't support server-sent events
  (Cloudflare's limits).
- Creating or deleting tunnels and DNS routes, multi-user features, auto-update and
  Windows/Linux builds are out of scope for v1. The core is portable; see decisions.md §2.
