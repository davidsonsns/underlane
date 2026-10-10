# Goal: a menu bar app for managing cloudflared tunnels

## Why

Today, running Cloudflare Tunnels means starting `cloudflared` by hand, keeping a terminal open per tunnel, and juggling several `config.yml` files across projects. Build a desktop app that runs in the menu bar and makes tunnels something you set up once and stop thinking about.

## Context and assumptions

- Working directory: this folder (empty, not yet a git repo). Run `git init` first and commit after each milestone.
- The primary target is **macOS** (Apple Silicon). Cross-platform support (Windows/Linux) is nice to have. Don't let it hold back the macOS experience, but don't rule it out without a reason either.
- `cloudflared` is already installed (`/opt/homebrew/bin/cloudflared`, 2026.9.1). The app should detect it on `PATH` and at common Homebrew locations, show the version, and explain clearly what to do if it's missing. Don't bundle it in v1.
- The app manages the `cloudflared` binary. It does not reimplement the tunnel protocol.
- Working name: pick something short. Don't use "Cloudflare" in the product name or icon (trademark).

## Phase 1: Research, then decide (write it down before you write any code)

Write `docs/research.md` and `docs/decisions.md`. Verify every claim about `cloudflared` against the current docs or `cloudflared --help`, not memory.

1. **Stack.** Compare what people actually use for polished menu bar / tray utilities in 2026. At minimum consider Tauri 2 (Rust + web UI), native Swift/SwiftUI (+ AppKit `NSStatusItem`), Electron, and Wails (Go). Look at real apps in this space (e.g. https://www.ports-app.com/ and other tray utilities) and what they're built with. Score each option on:
   - quality of the menu bar/tray integration (template icons, a dynamic count badge, popover vs. menu)
   - how easily it can run and supervise child processes and capture their stdout/stderr
   - login-item / auto-start support
   - animation and theming capability
   - memory footprint at idle
   - signing/notarization effort
   - cross-platform reach
   Pick one and justify it. Rust is not a requirement; choose on merit.
2. **cloudflared integration.** Document how to:
   - run named tunnels from a config file, token-based (remotely managed) tunnels, and quick tunnels (`trycloudflare.com`)
   - detect when a tunnel is healthy (the `--metrics` server, `/ready`, the Prometheus metrics, and the quick-tunnel hostname endpoint, if each still exists)
   - get structured logs (log level and format flags, `--logfile`)
   - see what `cloudflared tail` / the management API can and can't offer
3. **Request/response body capture.** `cloudflared` does not expose request or response bodies. Confirm that, then design the capture path. The expected approach is a **local inspecting reverse proxy owned by the app**: for tunnels with inspection turned on, the app generates a *derived* config whose ingress `service:` points at the proxy, and the proxy forwards to the real origin. Address:
   - streaming and chunked bodies
   - WebSockets and SSE (pass them through; record metadata only)
   - HTTPS origins and `originRequest` options
   - the added latency
   - what happens to traffic if the proxy crashes
   Never modify the user's original YAML.
4. **Process model.** Decide whether tunnels run as children of the app (they stop when the app quits) or as launchd agents (they survive app restarts). Record the trade-off and the choice.

## Phase 2: Build v1

### Tunnel/project management
- Import existing `config.yml` files (scan `~/.cloudflared/` and let the user add files or folders), plus token-based tunnels and quick tunnels.
- Group tunnels by project. Show each tunnel's hostname → local service mapping, as parsed from its ingress rules.
- Start, stop, and restart each tunnel individually, or all at once. Show live status (starting / connected / degraded / stopped / crashed) and restart crashed tunnels with backoff.
- Validate configs before starting them (`cloudflared tunnel ingress validate`), and show errors in plain language.

### Menu bar
- A clean monochrome template icon, plus a count of active tunnels (in the spirit of ports-app.com).
- The menu/popover lists projects and tunnels with status dots and one-click toggles. It also has quick actions: copy public URL, open in browser, open the dashboard, quit.

### Dashboard window
- Settings: cloudflared path, launch at login, per-tunnel "start automatically", log retention (TTL), body capture limits, theme.
- Log viewer: a live tail per tunnel with level filters and search. For tunnels with inspection on, show a request list (method, path, status, duration, size) with a detail pane showing headers and the request/response body. Pretty-print JSON, show a hex/placeholder view for binary, and add "copy as curl".

### Auto-start
- Launch at login via the platform's native mechanism (on macOS, `SMAppService` or the equivalent your stack supports).
- Tunnels marked "start automatically" come up when the app launches.

### Storage
- Use SQLite in the app's data directory for app state, cloudflared logs, and captured requests.
- Expire rows past the TTL (default 7 days) with a background sweep, and enforce a size cap.
- Turn off body capture by default and enable it per tunnel. Cap the stored size per body (default 1 MB, then truncate and mark as truncated). Redact `Authorization`, `Cookie`, `Set-Cookie`, and any headers the user configures.

### UI/UX
- Clean and minimal, with native-feeling typography and spacing.
- Light, dark, and system themes, with a toggle.
- Smooth, purposeful animations (status changes, list insertions, panel transitions) that respect "reduce motion".
- Every list has an empty state, a loading state, and an error state.

## Done when (all of these must hold)

1. `docs/research.md` and `docs/decisions.md` exist and justify the stack, the process model, and the capture design.
2. The app builds from a clean checkout with one documented command, and the README covers setup, the dev loop, and the architecture.
3. Automated tests cover config parsing, generating the derived config, the proxy's capture/redaction/truncation, and TTL expiry.
4. **An end-to-end check passes without a Cloudflare account**:
   - start a local echo server
   - expose it through the app as a quick tunnel with inspection on
   - `curl` the `trycloudflare.com` URL with a JSON body
   - assert the request and response bodies appear in SQLite and in the log viewer, and that the menu bar count goes 0 → 1 → 0 across start and stop
5. Manually verified on this Mac, with screenshots in `docs/screenshots/`:
   - importing an existing `config.yml`
   - running 2+ tunnels at once
   - launch at login (log out and back in, or simulate it)
   - auto-starting tunnels
   - theme switching
   - the light and dark dashboards
6. No terminal needs to stay open for any of the above.

## Working rules

- Commit at each milestone (research, skeleton, tunnel management, menu bar, dashboard, capture, auto-start, polish).
- After the research phase and again before calling it done, get a review per my global CLAUDE.md routing: opus plus a gpt-6-astra second opinion. Fix the real findings.
- If a requirement turns out to be infeasible as written, don't stop. Pick the closest workable alternative, record why in `docs/decisions.md`, and keep going.
- Out of scope for v1: creating or deleting tunnels and DNS routes via the Cloudflare API, team/multi-user features, auto-updates, and a Windows/Linux release build.
