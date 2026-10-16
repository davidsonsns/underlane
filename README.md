<p align="center">
  <img src="apps/desktop/src-tauri/icons/app-icon.png" alt="Underlane's Lane Cut logo" width="128" height="128">
</p>

# Underlane

A macOS menu bar app that runs your `cloudflared` tunnels for you. Set them up once, then stop
thinking about them: no terminal windows, no juggling `config.yml` files.

![Underlane dashboard with the Lane Cut logo](docs/screenshots/lane-cut-dashboard-dark.png)

- **Create a project or load what you already have.** Choose **New → Project**, enter a name
  and optional group, then load an existing cloudflared config or create a draft in
  `~/.cloudflared/underlane` (or a folder you choose). Loaded files stay in place. Creation
  opens the project, where **Set up tunnel** uses the cloudflared CLI to log in when needed,
  create a tunnel, and link its ID and credentials automatically. **New → Quick tunnel**
  creates a temporary tunnel for a local endpoint. Existing token tunnels remain supported.
- **One click per tunnel**, or all at once, with live status: starting, connected, degraded,
  stopped or crashed. Crashed tunnels restart with backoff.
- **A count in the menu bar** shows how many tunnels are up. The popover can copy a tunnel's public
  URL or open it in the browser.
- **The dashboard** has a log viewer with level filters and search. For tunnels with inspection
  on, it also has a request inspector with headers, pretty-printed JSON, a hex view and
  "copy as curl".
- **Launch at login** (native `SMAppService`) and per-tunnel **start automatically**.
- **Manage hostnames** in a compact editor for locally managed config tunnels. Add, edit, or remove
  rules, then save to the original config with a backup. Saving may change YAML comments and
  formatting; advanced rule settings are retained. Running tunnels restart after validation.
  Importing, starting, and request inspection still leave the original file untouched.
- **Check and create DNS records** from each project hostname using your existing cloudflared
  login. Underlane verifies the account, domain, proxy setting, and existing records before
  creating a route with cloudflared. Conflicting records are preserved. An API token with
  **Zone Read** and **DNS Read** is an optional fallback, stored in the system keychain.
  Without authenticated access, public checks show “DNS target unverified” for hidden targets;
  local service availability is checked separately. Automatic DNS checks run at most once
  every 24 hours per project hostname, with cached status retained across app restarts.
  Use **Check DNS** in the hostname menu to refresh immediately.
- **Know when a running config is stale.** Imported tunnels show “Config changed on disk · Restart
  to apply” after an external edit, checked when you revisit the detail or return to the dashboard.
  Restart is explicit; Open in Editor uses the file's registered app, and Show in Finder reveals it.
- **Understand tunnel failures.** Known failures show a short cause and next action, with an
  expandable, copyable redacted diagnostic. Retry/backoff feedback stays visible; unknown errors
  keep their original safe message without guessing a diagnosis.

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

Requirements: macOS 13.3 or later (built and tested on macOS 27, Apple Silicon).
Guided welcome automatically checks for a working `cloudflared`. If it is missing, choose
**Install for Underlane** to download a verified official release for Apple Silicon or Intel
into the app's data folder. This requires no administrator password or system service.
You can also choose an existing executable, use the linked installation guide, or install it manually:

```sh
brew install cloudflared   # Underlane finds it on PATH or in Homebrew's folders
```

To build from source you also need Xcode command-line tools, Node 20+, pnpm 10.27.0 and Rust (via
[rustup](https://rustup.rs)).

Enable the pinned pnpm version with Corepack (install Corepack first if your Node
distribution omits it):

```sh
corepack enable
corepack install
```

```sh
make build      # pnpm install --frozen-lockfile + tauri build -> target/release/bundle/macos/Underlane.app
make install    # same, then copies it to ~/Applications (needed for launch at login)
```

Open `Underlane.app`. First run opens **Guided welcome → Connector → First tunnel**.
**Use temporary link** starts a quick tunnel without a Cloudflare account and shows its public URL
when connected. Anyone with the link can access the local service; the URL changes each time the
tunnel starts and stops working when the tunnel stops or Underlane quits. Quick tunnels are for
testing and development.

**Connect Cloudflare** is optional, for users who want their own hostname. It authorizes
cloudflared in the browser (or verifies existing authorization), then offers project creation.
Authorization grants tunnel-management access and saves the default account certificate; it does
not configure a hostname or visitor access rules. Installation errors and cancelled authorization
can be retried without restarting onboarding. Completed onboarding is remembered; existing users
with saved tunnels continue directly to the dashboard.

From the dashboard, choose **New** for a project or quick tunnel. New project configs are drafts:
choose **Set up tunnel**, complete Cloudflare login in
your browser if prompted, then configure hostnames and start from the project. Loading a config shows its
hostnames and endpoints before creation and uses that original file without copying it.

Tunnel setup validates the project config before creating a Cloudflare resource and saves a
backup when linking its identity. Credentials stay in `~/.cloudflared/underlane/<project-id>.json`.
Failed linking can be retried using the saved credentials; setup remembers the original tunnel
name across project renames. Setup supports the default `~/.cloudflared/cert.pem` account.

If you use a menu bar manager (Hidden Bar, Ice, Bartender), new items may land in its hidden
section. Drag Underlane's circular Lane Cut icon into the visible part.

## Repository layout

```text
apps/desktop/          Svelte frontend, app configuration, and src-tauri shell
apps/web/              Astro landing page and product screenshot fixtures
crates/underlane-core/ Shared Rust core
scripts/               Native build and verification helpers
e2e/                   Desktop end-to-end checks
```

Run the documented commands from the repository root. The pnpm workspace owns
`apps/*`; root scripts forward desktop commands to the `underlane` package.
The Cargo workspace shares its lockfile and root `target/` directory across
`crates/underlane-core` and `apps/desktop/src-tauri`. Frontend output lives in
`apps/desktop/dist`; app bundles still land in `target/`.

### Landing page

The Astro site in `apps/web` uses Node.js 22.22 or later. From the repository root:

```sh
pnpm dev:web       # Local landing page at http://127.0.0.1:4321
pnpm check:web     # Astro and TypeScript diagnostics
pnpm build:web     # Static output in apps/web/dist
```

`pnpm --filter @underlane/web screenshots` refreshes the light and dark product
images by launching the real Svelte UI with synthetic Tauri data. It uses the
workspace's Playwright Chromium installation (`pnpm exec playwright install chromium`
if needed), starts a temporary Vite server, and captures four sample projects,
requests, logs, and the menu bar panel. It does not connect to Cloudflare or use
your app data. Browser captures omit native macOS window controls.

## Dev loop

```sh
make dev        # Vite + cargo with hot reload (frontend) and rebuild-on-change (Rust)
make test       # Rust tests, Vitest, script-helper tests, and svelte-check
make e2e        # end-to-end check against a real quick tunnel (no Cloudflare account needed)
pnpm run lint   # Biome lint + formatting checks with Ultracite's core and Svelte presets
pnpm run fix    # Apply Biome's safe fixes and formatting
pnpm run analyze # Fallow: unused code, duplication, and complexity; fails on findings
pnpm run test:scripts # Local harness/browser-mock tests; no native app or tunnel needed
```

With Vite running, `node e2e/onboarding-browser.mjs` checks the guided flow using synthetic IPC.
The optional live installer check downloads only into disposable test data:
`cargo test -p underlane --lib installer::tests::live_installs_verified_official_release -- --ignored --nocapture`.

`pnpm run check` still runs Svelte/TypeScript diagnostics. Biome's Svelte support is
experimental, so keep this check alongside linting. Biome skips generated shadcn
components in `apps/desktop/src/lib/components/ui`; Fallow still follows their imports. Fallow
includes both window entry points, the Svelte config, and standalone verification
scripts, and excludes Rust build output. PascalCase and kebab-case Svelte component
filenames are supported.
Fallow lists standalone runners explicitly so imported harness helpers are checked
for unused exports. Its named `ignoreExports` entries preserve intentional UI
namespace contracts (`Root`, `Content`, `Trigger`, etc.) and widget `focus` methods:
identical names in separate modules are not duplicate implementations. Source-level
suppressions explain intentional sequencing, composite-widget semantics, and other
specific analyzer limitations. No baseline hides existing findings. These commands
run locally; no CI or Git hooks are installed.

Named `health.thresholdOverrides` retain bounded UI transactions whose CRAP score
uses estimated zero coverage for mounted component handlers. Each entry explains
its ordering/ownership contract and caps the current score; cyclomatic and cognitive
limits stay at their defaults. Pure list, query, grouping and navigation decisions
have unit tests. Browser mocks can target a frozen preview with `E2E_BASE_URL`.

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
  - one-shot command classification, config selection, timeout/cancellation/drop cleanup, and
    secret redaction; SQLite migration tests also cover preservation, rollback and newer schemas
- `apps/desktop/src/lib/*.test.ts` (Vitest) cover the UI's pure helpers: `format` (copy as curl, hex dump, JSON
  detection), `listnav` (list keyboard navigation), `latest` (debounced latest-only async jobs),
  `combobox` (the group picker's rows), `focus` (returning focus when a dialog closes) and `utils`
  (class merging with the custom text sizes).
- `e2e/run.mjs` launches the real app with an isolated data folder and a local echo server. It
  exposes the echo server as a quick tunnel with inspection on, POSTs JSON to the
  `trycloudflare.com` URL, and asserts:
  - the request and response bodies are in SQLite and visible in the dashboard's request viewer
  - the menu bar count, read from the real `NSStatusItem`, goes 0 → 1 → 0

  Evidence lands in `e2e/out/`.
- `cargo test --release -p underlane-core --test proxy_capture -- --ignored --nocapture` measures the
  latency the proxy adds (about 0.03 ms at p50 on loopback).
- `cargo test -p underlane-core --test cli_contract -- --ignored --nocapture` explicitly runs
  offline cloudflared contracts with isolated HOME and synthetic configs. Set
  `CLOUDFLARED_CONTRACT_BIN` to an absolute binary path if discovery cannot find it.
- `node e2e/config-badge.mjs target/debug/bundle/macos/Underlane.app` tests the config-change UI
  locally against the shared fake connector after an `e2e` feature build. It covers edits,
  restoration, restart, missing files and failure recovery without a Cloudflare account.

## Architecture

```
┌──────────────────────────── Underlane.app (one process) ─────────────────────────────┐
│  WKWebView: popover.html + index.html (Svelte 5)        tray icon (NSStatusItem)   │
│        │  invoke / events                                    ▲ count              │
│  Tauri shell (commands, tray & windows, SMAppService, App Nap, e2e control server) │
│        │                                                                          │
│  underlane-core (plain Rust, no Tauri)                                               │
│   ├─ supervisor ── spawns ──▶ Underlane --cloudflared-shim ──▶ cloudflared tunnel …  │
│   │      │  stderr JSON logs, /ready /quicktunnel /config polling, backoff        │
│   ├─ proxy (hyper) ◀── cloudflared (derived config: service → 127.0.0.1:port)     │
│   │      └──▶ real origin (http/https, originRequest TLS options)                 │
│   ├─ config / derived / import    (read-only on your YAML)                        │
│   └─ db (SQLite: settings, groups, tunnels, logs, requests; TTL + size cap)     │
└───────────────────────────────────────────────────────────────────────────────────┘
```

- **`crates/underlane-core`** contains everything platform-independent:
  - `config.rs` parses cloudflared YAML.
  - `derived.rs` decides which rules can be inspected and writes the derived copy.
  - `proxy/` is the inspecting reverse proxy. It streams frames through untouched while copying up
    to the cap, redacts before storage, and passes WebSocket and SSE through.
  - `supervisor.rs` handles process lifecycle, health, restarts, orphan reaping and detecting the
    same tunnel running outside Underlane.
  - `cli.rs` runs bounded one-shot commands with explicit target/neutral configs, cancellation,
    process cleanup, secret stdout and output-based errors; it is separate from the supervisor.
  - `platform.rs` owns Unix process listing, groups, termination and home lookup.
  - `secret.rs` wraps and zeroizes backend tokens and secret command output and redacts diagnostics.
  - `db.rs` is the SQLite layer, with transactional versioned migrations (current schema v1).
  - `shim.rs` is the watchdog that kills cloudflared if the app dies.
  - `decode.rs` handles display-time gzip/br/deflate decoding.
- **`apps/desktop/src-tauri`** is the app shell:
  - `state.rs` wires the supervisor to SQLite (a bounded, batched writer) and to the UI (events).
  - `tray.rs` builds the menu bar item, the popover (vibrancy, positioned under the icon) and the
    dashboard window.
  - `platform.rs` covers `SMAppService`, App Nap and state restoration.
  - `commands.rs` holds the UI commands.
  - `e2e.rs` is the test-only control server.
- **`apps/desktop/src/`** is the Svelte 5 UI, styled with Tailwind v4 and shadcn-svelte (Luma style, tuned to
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
- Deleting remote tunnels or DNS records, multi-user features, auto-update and
  Windows/Linux builds are out of scope for v1. The core is portable; see decisions.md §2.
