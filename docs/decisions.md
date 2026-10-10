# Decisions

Each entry gives the decision, why it was made, and the alternatives. Evidence is in
[research.md](research.md).

## 1. Name: Burrow

The name is short, suggests tunnels, and doesn't use "Cloudflare" in the product name or icon. The
icon is an abstract arch/burrow mark, not the Cloudflare cloud. Bundle id: `com.davidsonsns.burrow`.

## 2. Stack: Tauri 2 (Rust core + Svelte 5 / TypeScript UI)

- Tauri scored highest (31/35) in research.md §1. Swift scored 30, Electron 28, Wails v3 27.
- **The owner plans a Windows/Linux release later** (said during research). That rules out native
  Swift, which would mean a rewrite, and moves cross-platform reach from "nice to have" to a real
  weight in the score.
- The hard parts (supervisor, streaming proxy, SQLite) are a plain Rust crate, `burrow-core`, with
  no Tauri dependency. It's testable with `cargo test` and portable.
- UI: **Svelte 5** + Vite + TypeScript. It compiles to a small bundle and has built-in `flip`,
  `fly`, and `fade` transitions for purposeful animation without an animation library.
- macOS-specific polish is done through the `objc2` crates where Tauri doesn't reach:
  `SMAppService`, activation policy, and the template icon.

Rejected: Swift (macOS only), Electron (150–300 MB idle for an always-on utility), Wails v3 (beta;
no Go toolchain here; fewer reference tray apps).

## 3. Process model: tunnels are children of the app

- Tunnels run as child processes of Burrow. Quitting Burrow stops them (gracefully: SIGTERM, then
  SIGKILL after 5 s).
- They come back without a terminal through **launch at login** (`SMAppService.mainApp`) plus
  per-tunnel **start automatically**.
- Why not launchd agents: the inspecting proxy lives in the app, so tunnels that outlive it would
  serve 502s for inspected traffic. Also, live stderr capture, status, and backoff restarts are
  much simpler and more accurate in-process, nothing gets installed in
  `~/Library/LaunchAgents`, and the same model works on Windows/Linux.
- Crash safety: each child's pid goes to `<data>/run/<tunnel-id>.pid`. On launch, leftover pids
  whose command line contains Burrow's data directory are killed before anything starts, so a crash
  never leaves orphaned cloudflared processes. Children are also spawned in their own process group,
  which lets the app kill the whole group.
- Trade-off accepted: tunnels are down while the app isn't running. A user who needs
  24/7 tunnels independent of a GUI should use `cloudflared service install`, and the README says so.

## 4. Body capture: an app-owned inspecting reverse proxy, opt-in per tunnel

- Design is in research.md §3. There's one loopback listener per inspected ingress rule, and the
  derived config lives in `<data>/derived/`. **The user's YAML is never written.** Imported files
  are only read, and they're re-read on every start, so edits to them are picked up.
- **Token tunnels: inspection is not offered.** Their ingress is remote, and rewriting it would
  require the Cloudflare API (out of scope for v1). The mapping is still shown from the metrics
  `/config` endpoint while the tunnel runs. Infeasible as written, so this is the closest
  workable alternative.
- WebSocket and SSE are passed through with metadata only.
- If a proxy listener can't be (re)bound, the tunnel is restarted **fail-open** with its original
  config and a warning, putting traffic before capture.
- Body capture is off by default. When on, each body is capped at 1 MB (configurable) and marked
  truncated past the cap. `Authorization`, `Cookie`, `Set-Cookie`, and `Proxy-Authorization`, plus
  user-configured headers, are redacted **before** storage. Redaction happens in the proxy, so
  secrets never reach SQLite.

## 5. Health and status

- `--metrics 127.0.0.1:<port>`: the app picks a free port per tunnel run.
- `/ready` is polled every 2 s. Status meanings:
  - **starting**: the process is alive and `/ready` hasn't returned 200 yet.
  - **connected**: `/ready` returns 200.
  - **degraded**: the tunnel was connected, but `/ready` now returns non-200 or the process stopped
    answering.
  - **crashed**: the process exited when the user didn't ask it to.
  - **stopped**: the user stopped it.
- Quick tunnels get their public URL from `/quicktunnel`. The log banner is a fallback.
- Crash restart uses exponential backoff: 1 s, 2 s, 4 s … capped at 60 s, reset after 2 minutes of
  stable running. After 10 consecutive failures the tunnel stays **crashed** until the user acts.
  This keeps a bad config from hot-looping.

## 6. Logs

- `--output json --loglevel <per-tunnel, default info>` is read from stderr. Stdout is captured
  too.
- Non-JSON lines are stored as `info` with the raw text.
- When a debug line carries a `headers` object, the redaction list is applied to it before
  storage.

## 7. Storage

- SQLite at `~/Library/Application Support/com.davidsonsns.burrow/burrow.db` (Tauri's
  `app_data_dir`), using `rusqlite` with the `bundled` feature and WAL mode.
- Tables: `settings`, `projects`, `tunnels`, `logs`, `requests`.
- A background sweep every 10 minutes (and once at startup) deletes `logs` and `requests` older
  than the TTL (default 7 days). It then enforces the size cap (default 256 MB) by deleting the
  oldest rows in batches and running `PRAGMA incremental_vacuum`.

## 8. Validation messages

`cloudflared tunnel --config <file> ingress validate` runs before every start of a config-based
tunnel, and also on import. Known messages map to plain language, for example:

- "The last ingress rule must match all URLs" → "Add a final catch-all rule without a hostname,
  for example `- service: http_status:404`."

Unknown messages are shown verbatim.

## 9. Menu bar

- A monochrome **template** icon, so macOS tints it for light, dark, and highlighted states.
- The active-tunnel count shows as the status item's title next to the icon. **It's hidden at 0**,
  to keep the menu bar clean (the same approach as macOS's own counters). Internally the count is
  always tracked, and the e2e test asserts the title is empty or 0, then 1, then empty again.
- Left click toggles a **popover** (a borderless, always-on-top webview positioned under the icon
  with `tauri-plugin-positioner` `TrayCenter`; it hides on blur). Right click shows a native menu
  with Open Dashboard, Start All, Stop All, and Quit, so there's always a non-webview path.
- Burrow runs as an accessory app with no Dock icon. The dashboard window is a normal window, and
  closing it hides it.

## 10. Launch at login

- macOS: `SMAppService.mainApp` register/unregister through `objc2-service-management`, which is
  the native mechanism Apple recommends on macOS 13+. Its status, including "requires approval",
  is shown in Settings.
- Other platforms (future): `tauri-plugin-autostart`.
- Verification without logging out: register, confirm with `sfltool dumpbtm`/System Settings, quit
  the app, then launch it the way the login session does (`open -g`, no arguments) and check that
  auto-start tunnels come up. The steps are recorded in docs/screenshots. Logging out would end the
  session running the build agent.

## 11. cloudflared discovery

Discovery checks, in order:

1. The user setting.
2. `PATH` from a login shell (`$SHELL -lc 'command -v cloudflared'`). GUI apps launched by launchd
   get a minimal `PATH`.
3. `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, `/usr/bin`.

The version comes from `cloudflared --version`. If none is found, the UI shows
`brew install cloudflared` with a copy button and a link to Cloudflare's downloads page.

## 12. Build and test

- `make build` is the one command. It runs `npm ci` and `npx tauri build --bundles app`, and the
  output lands in `target/release/bundle/macos/Burrow.app`.
- `make dev` runs the dev loop, `make test` runs the Rust and frontend tests, and `make e2e` runs
  the end-to-end check.
- Releases are ad-hoc signed (`signingIdentity: "-"`). Notarization needs a Developer ID that isn't
  available here. It's documented, but not part of v1.

## 13. E2E automation hook

The e2e test drives the **real built app**. When launched with `BURROW_E2E_PORT=<port>`, Burrow
starts a loopback-only control server. The endpoints are:

- add a quick tunnel
- start or stop it
- read the tray title
- run the same query the log viewer uses
- ask the dashboard webview for its rendered text

The control server is compiled in only with the `e2e` cargo feature, so release builds made with
`make build` don't contain it.
