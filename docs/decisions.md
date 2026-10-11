# Decisions

Each entry gives the decision, why it was made, and the alternatives. Evidence is in
[research.md](research.md).

## 1. Name: Burrow

The name is short, suggests tunnels, and doesn't use "Cloudflare" in the product name or icon. The
icon is an abstract arch/burrow mark, not the Cloudflare cloud. Bundle id: `com.davidsonsns.burrow`.

## 2. Stack: Tauri 2 (Rust core + Svelte 5 / TypeScript UI)

- Tauri and Swift tie at 30/35 in research.md §1 after review corrected Tauri's memory score.
  Electron scores 28 and Wails v3 27. **The tie is broken by weighting, not the sum:**
  cross-platform reach outweighs the last bit of macOS polish for this owner.
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
- Crash safety (revised after review):
  - Each cloudflared is launched through a **watchdog shim**, `Burrow --cloudflared-shim`. The shim
    kills cloudflared when the app's pipe closes, so a crashed or force-quit app takes its tunnels
    down within seconds.
  - Pids are also written to `<data>/run/<tunnel-id>.pid`, and leftovers are reaped on the next
    launch.
  - Every launch mode runs from a file in `<data>/` (a derived copy, even with inspection off). The
    command line therefore always contains Burrow's data directory, and that marker is how
    ownership is checked before killing anything.
  - Each child runs in its own process group.
- **Tunnels already running outside Burrow:** before starting a config tunnel, Burrow scans for
  cloudflared processes that reference the same config path or tunnel UUID and aren't its own. If
  it finds one, it refuses to start and shows "already running outside Burrow (pid N)". A second
  connector would split traffic, so captures would miss requests and Stop wouldn't really stop the
  tunnel. (The owner's `example.yml` is running like this through a script right now.)
- Shutdown: Burrow passes `--grace-period 4s`, and on quit it SIGTERMs every tunnel and waits for
  them synchronously before exiting.
- App Nap: while any tunnel runs, Burrow holds an `NSProcessInfo` activity (user-initiated, idle
  sleep allowed), so timers and the proxy aren't throttled.
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
- WebSocket and SSE are passed through with metadata only. They're recorded when they open and
  updated when they close.
- The upstream leg is always HTTP/1.1. Rules with `http2Origin: true` or `matchSNItoHost: true`, and
  `unix:` services, are **not inspected**. They keep their original service, and the UI says why.
  `ws://`/`wss://` services are inspected.
- cloudflared runs with its working directory set to the original config's directory, because
  relative paths resolve against the CWD. Derived configs get absolute paths resolved the same way.
- If a proxy listener can't be (re)bound, the tunnel is restarted **fail-open** with its original
  config and a warning, putting traffic before capture.
- Body capture is off by default. When on, each body is capped at 1 MB (configurable) and marked
  truncated past the cap.
- Redaction happens before storage:
  - Headers: `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`,
    `Cf-Access-Jwt-Assertion`, `Cf-Access-Client-Secret`, `X-Api-Key` and `X-Auth-Token`, plus
    user-configured names.
  - Query parameters: `token`, `access_token`, `api_key`, `apikey`, `key`, `code`, `sig`,
    `signature`, `password` and `secret`, plus user-configured names. This applies to captured
    paths and to cloudflared log messages, since debug lines contain full URLs.
- **What isn't guaranteed:** bodies are stored as sent, so an origin that echoes a credential into
  its response body, or a form that posts a password, ends up in SQLite. The Settings screen says so
  next to the capture toggle. "Copy as curl" uses the redacted values.
- Captures go through a bounded channel to a batched writer and are dropped, with a counter, rather
  than slowing traffic. A 64 MB global in-flight body budget bounds memory.

## 5. Health and status

- `--metrics 127.0.0.1:0`: cloudflared picks a free port and Burrow reads it from the
  `Starting metrics server on 127.0.0.1:N/metrics` log line. This avoids a pick-then-bind race.
- `/ready` is polled every 2 s. Status meanings:
  - **starting**: the process is alive and `/ready` hasn't returned 200 yet.
  - **connected**: `/ready` returns 200.
  - **degraded**: the tunnel was connected, but `/ready` has now failed on **two consecutive**
    polls. The debounce keeps a brief reconnect from flapping the status.
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
  oldest rows in batches. It then runs `PRAGMA incremental_vacuum` (the database is created with
  `auto_vacuum = INCREMENTAL` before any table exists) and `PRAGMA wal_checkpoint(TRUNCATE)`, so the
  file really shrinks.

## 8. Validation messages

`cloudflared tunnel --config <file> ingress validate` runs before every start of a config-based
tunnel (bounded by a 20 s timeout, and cancellable by Stop), and on demand from the tunnel's
**Validate** button. Import only parses the file, so importing never needs the binary. Known messages map to plain language, for example:

- "The last ingress rule must match all URLs" → "Add a final catch-all rule without a hostname,
  for example `- service: http_status:404`."

Unknown messages are shown verbatim.

## 8a. Single-origin configs

Single-origin configs (a top-level `url:`, no `ingress:`) fail `ingress validate` even though
cloudflared can run them. For these files Burrow skips the validate step and shows the single
mapping.

## 9. Menu bar

- A monochrome **template** icon, so macOS tints it for light, dark, and highlighted states.
- The active-tunnel count shows as the status item's title next to the icon. **It's hidden at 0**,
  to keep the menu bar clean (the same approach as macOS's own counters). Internally the count is
  always tracked, and the e2e test asserts the title is empty or 0, then 1, then empty again.
- Left click toggles a **popover** (a borderless, always-on-top webview positioned under the icon
  with `tauri-plugin-positioner` `TrayCenter`; it hides on blur). Right click shows a native menu
  with Open Dashboard, Start All, Stop All, and Quit, so there's always a non-webview path.
- Burrow runs as an accessory app with no Dock icon. The dashboard window is a normal window that is
  **destroyed on close**, which frees its WebContent process. The popover is built on first use.
- "Copy public URL" uses the quick tunnel URL, or the first non-wildcard hostname, as `https://`.
  The dashboard lists every hostname with its own copy and open buttons.

## 10. Launch at login

- macOS: `SMAppService.mainApp` register/unregister through `objc2-service-management`, which is
  the native mechanism Apple recommends on macOS 13+. Its status, including "requires approval",
  is shown in Settings.
- Other platforms (future): `tauri-plugin-autostart`.
- Ad-hoc signing risk: Background Task Management identifies login items by code signature, and an
  ad-hoc signature changes on every rebuild. Register only the copy installed at
  `~/Applications/Burrow.app` (`make install`), never a bundle inside `target/`. If registration
  keeps failing for an ad-hoc build, fall back to a LaunchAgent plist (what `tauri-plugin-autostart`
  does) and record that here.
- Verification without logging out: register, confirm with `sfltool dumpbtm`/System Settings, quit
  the app, then launch it the way the login session does (`open -g`, no arguments) and check that
  auto-start tunnels come up. The steps are recorded in docs/screenshots. Logging out would end the
  session running the build agent.

## 10a. Tokens

Tokens for token tunnels are stored in the macOS Keychain (the `keyring` crate, which also covers
Windows Credential Manager and the Secret Service later). They're passed to cloudflared through
the `TUNNEL_TOKEN` environment variable, never on the command line. Trade-off: an ad-hoc-signed
rebuild may make the Keychain ask once for access again.

## 10b. Projects

On import, a tunnel is grouped by its config's folder. A file at `<dir>/.cloudflared/x.yml` or
`<dir>/x.yml` goes to the project named `<dir>`'s folder name, and files directly in
`~/.cloudflared` go to **Personal**. Tunnels can be moved between projects.

## 11. cloudflared discovery

Discovery checks, in order:

1. The user setting.
2. `PATH` from a login shell (`$SHELL -lc 'command -v cloudflared'`). GUI apps launched by launchd
   get a minimal `PATH`.
3. `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, `/usr/bin`.

The login-shell lookup has a 3 s timeout, so a slow shell profile can't hang discovery. The version
comes from `cloudflared --version`. If none is found, the UI shows
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

## 14. Review outcomes (research phase)

The research phase was reviewed by opus-5.5 and gpt-6-astra (Codex). Accepted findings and where
they landed:

| Finding | Source | Resolution |
| --- | --- | --- |
| Children survive an app crash; docs contradicted themselves | both | Watchdog shim plus pidfile reap, and the docs were corrected (§3, research §3.7) |
| Uninspected named tunnels weren't identifiable as Burrow's | both | Always run from a file in the data dir (§3) |
| The same tunnel already running outside Burrow | opus | Detect it and refuse, with an explanation (§3) |
| Compressed bodies unreadable | opus | Raw storage, truncation-tolerant decode at display (research §3.3.1) |
| E2E DNS/NXDOMAIN flakiness, 429s | opus | e2e resolves through 1.1.1.1 and pins that IP for the HTTPS request (the `curl --resolve` technique, done in Node), retries with a budget and a marker (README) |
| h2 upstream breaks WebSocket/Host; `http2Origin`, `matchSNItoHost`, `tlsTimeout` | both | HTTP/1.1 upstream; those rules aren't inspected; `tlsTimeout` is applied (§4) |
| Relative paths resolve against the CWD | both | Spawn with CWD = config dir, absolutize the same way, credentials preflight (§4) |
| `unix:`/`ws:` services | opus | ws/wss inspected; unix not inspected in v1 (§4) |
| Query strings and other secrets in URLs/logs | both | Query-param redaction, a wider header list, honest guarantee wording (§4) |
| Memory/back-pressure | opus | Bounded channel, drop counter, 64 MB in-flight budget (§4) |
| `panic = abort` would void containment | opus | Release profile keeps unwind (research §3.7) |
| Webview memory claim wrong | opus | Score corrected, dashboard destroyed on close (§2, §9) |
| App Nap; 30 s grace vs. 5 s kill | opus | NSProcessInfo activity; `--grace-period 4s` (§3) |
| SMAppService with ad-hoc signing | opus | Register the `~/Applications` copy; LaunchAgent fallback (§10) |
| Metrics port race; degraded flapping | opus | `--metrics 127.0.0.1:0` + log parse; two-strike debounce (§5) |
| `auto_vacuum` / WAL size | both | `auto_vacuum=INCREMENTAL` at creation + `wal_checkpoint(TRUNCATE)` (§7) |
| E2E reads internal state, not the menu bar | opus | e2e reads the real `NSStatusItem` button title through the control server (§13) |

Not adopted: a Unix-socket proxy listener instead of loopback TCP (opus L3). Ports are held for the
life of a run and reused across cloudflared restarts, which closes the rebind window. A socket
listener can come later.

## 15. Review outcomes (pre-ship)

The finished code was reviewed by opus-5.5 and gpt-6-astra (Codex). Accepted findings and fixes:

| Finding | Source | Fix |
| --- | --- | --- |
| Stop followed by a quick Start overlapped two runs; the old run's late `Stopped` and pidfile removal clobbered the new one | both | A stopping run stays registered until its process is gone, and a Start is deferred until then. Every state carries a run number, and stale ones are dropped. A pidfile is only removed if it names this run's pid. Regression test `stop_then_immediate_start_does_not_overlap`. |
| Killing only the shim, or a panic, could orphan cloudflared | both | After the direct child exits, its whole process group is swept (SIGTERM, then SIGKILL), and a drop guard kills the group on unwind. `kill_on_drop` on the shim is gone. At startup Burrow also reaps any cloudflared carrying its data-dir marker. Test `killing_only_the_shim_does_not_orphan_cloudflared`. |
| Sync commands blocked the main thread on SQLite | opus | Database-backed commands are `async` and run in `spawn_blocking`. TTL deletes run in small batches. Base64 is only sent for binary bodies (64 KB max for the hex view). |
| Captured bodies escaped the memory budget once queued | both | The write queue has its own 64 MB byte budget; overflow is dropped and counted. A size-cap sweep runs after every 32 MB written, not only every 10 minutes. |
| Capture could resume after a gap (`AAA`+`CCC`) | Codex | After the first skipped byte, a side only counts bytes and stores nothing more. |
| Query secrets in structured log fields (`dest`, `error`) and in `Location`/`Referer` | both | Query redaction applies to every string in a log line's JSON and to URL-valued headers. `Sec-WebSocket-Protocol` is redacted. User-configurable query names are in Settings. |
| Copy as curl: unquoted method (shell injection), HEAD, decoded body with `Content-Encoding` | both | The method is shell-quoted, HEAD becomes `-I`, encoding headers are dropped, and a truncated body is flagged. Unit tests cover it. |
| The Toggle inside the Quick tunnel form submitted the form (created and started a tunnel) | Codex | `type="button"`, and the toggle is disabled while busy. |
| A crash loop couldn't be stopped from its own controls | opus | `supervised` state: Stop is offered whenever a run exists, including crashed-and-retrying. |
| Same-tunnel detection: name substring false positives; bare `tunnel run` missed | opus | Names match only whole arguments. A default config matches a bare `cloudflared tunnel run` (e.g. the launchd service). Tests cover both. |
| Single-origin legacy flags (`no-tls-verify`, `origin-server-name`, …) ignored; `hello-world` configs failed validation | both | Legacy keys are honoured, and `hello-world` counts as single-origin. |
| `Trailer` stripped as hop-by-hop | Codex | It's end-to-end (RFC 9110) and is now forwarded. |
| Validation could hang and block Stop | Codex | 20 s timeout, `kill_on_drop`, and Stop is handled while preparing. |
| The e2e server could be compiled into a release build with `--features e2e` | Codex | It now requires both the `e2e` feature and `debug_assertions`. |
| No bulk restart | Codex | "Restart all" in the dashboard and the tray menu. |
| Log view duplicates, all levels off showing everything, missed events on load, settings overwriting typing, stale request detail, label-wrapped toggles, listbox roles without keyboard support | opus/Codex | All fixed in the Svelte views. |

| `tlsTimeout` was folded into one combined connect deadline | Codex | Burrow's own origin connector applies `connectTimeout` to TCP connect and then a separate `tlsTimeout` to the TLS handshake. Test `tls_timeout_is_a_separate_handshake_deadline`: an origin that accepts TCP but never handshakes fails after the 300 ms TLS deadline, even with a 30 s `connectTimeout`. |

## 16. Verifying launch at login: simulation instead of a log-out/log-in

The original brief's acceptance criterion for launch at login allowed either path: "log out and
back in, **or simulate it**". A real log-out wasn't possible here. The work ran unattended in
this user's GUI session, and logging out terminates every process in it, including the agent
doing the verification. That made the simulation the closest workable path, and it is the one
the brief allows.

How the simulation was made faithful, rather than just "the app was opened":

1. The installed release build registered itself through `SMAppService.mainApp`, the same call
   the Settings toggle makes. macOS reported `enabled`, and the app appeared in System Settings →
   Login Items → Open at Login (screenshot 15).
2. It was launched the way the system opens login items: through LaunchServices, with no arguments
   and no terminal.
3. Evidence that this is the same mechanism (verification.md §6c): apps started by this Mac's real
   login (Raycast, Warp) run as LaunchServices jobs `application.<bundle-id>.<n>`, parented by
   `/sbin/launchd`. Burrow's simulated launch produced the identical job type
   (`application.com.davidsonsns.burrow.198522606.198522611`, parent `/sbin/launchd`, no TTY).
4. Tunnels marked **Start automatically** came up with no start command (screenshots 18–19). They
   logged `Registered tunnel connection` and served a public `curl`.

5. The launch carried the same Apple event macOS sends when it opens login items
   (`keyAELaunchedAsLogInItem`), and Burrow itself recognised it as a login launch (Settings: "This
   launch: Opened by macOS at login"). A plain `open` was recorded as a normal launch, as the
   control. On the login launch Burrow opened no window, and its "start automatically" tunnel
   connected (verification.md §6d, screenshots 24–26). A real log-out would also have quit every
   other app in the owner's session and left the Mac at the login window unattended, so it was
   deliberately not done.

Item 5's launch-at-login requirement is therefore met through the permitted simulation, and no
further step is required. A user who wants extra assurance can turn the setting on and log out
and back in. That is optional confirmation, not missing verification.

## 17. UI stack: Tailwind v4 + shadcn-svelte (Luma)

**Decision (owner).** The UI moves from per-component scoped CSS to Tailwind v4 and shadcn-svelte
with the **Luma** style (preset `b1VlIttQ`: lucide icons, neutral base, `menuAccent: bold`),
tuned to AppKit metrics: system font at 13px, 22px controls and menu rows, sheets that drop from
under the toolbar, keyboard-only focus rings. The port runs one screen per phase, and the app
ships after each phase. The full plan lives outside the repo; this entry records what was decided.

**Layers during the migration.** `src/styles/app.css` is the only global stylesheet and declares
`@layer theme, base, legacy, components, utilities`. The old `base.css` is imported into
`legacy`: it beats Tailwind's preflight, so unported screens keep their look, and loses to
utilities, so new code wins. Svelte `<style>` blocks are unlayered and beat everything, so a
component moves to utilities in one go. Tailwind only scans `src/` (`source(none)` +
`@source "../"`), so the HTML files carry no utility classes. Two preflight compat rules
(placeholder colour, checkbox margin) sit at the bottom of base.css and go with it.

**Why the tokens moved first (Phase 0).** Any token left in base.css's `:root` would override
Tailwind's and app.css's tokens from inside the `legacy` layer (`text-sm` came out 12px,
`rounded-lg` 12px). And base.css's `html { font-size: 13px }` shrank every rem-based utility by
19% (`h-9` = 29.25px). So app.css owns every token from Phase 0, the root font size stays 16px
(13px is set on `body`), and five same-pixel renames free the names Tailwind uses:
`--text-sm→--text-callout`, `--text→--text-sm`, `--accent→--tint`, `--radius-lg→--radius-xl`,
`--font/--mono→--font-sans/--font-mono`. Burrow's names are the source of truth and shadcn's
names alias them. The only intended visual changes were `--text-3` (contrast fix) and `--hover`
(+0.5–1%). Measured in WebKit after the change: html 16px, body 13px, `text-sm` 13px, `h-9`
36px, `rounded-lg` 8px, a `bg-primary` button white on the selected-content blue with its
transition intact, and a legacy `.btn.primary` still 24px on `AccentColor`.

**Accent.** `--tint` is the raw `AccentColor` (rings, switches, icons). Filled surfaces use
`-apple-system-selected-content-background` with white text (AA), behind `@supports` with hex
fallbacks. `AccentColorText` is never used: WKWebView resolves it to black.

**Dark mode.** One `@custom-variant dark` that matches `html[data-theme=dark]`, or the system
setting when the theme is "system" (no `data-theme`). The Rust `set_theme` and vibrancy sync is
unchanged. No `mode-watcher`: the toast component gets the theme setting as a prop.

**No theme cross-fade.** Theme switches snap. The native vibrancy material flips instantly on
`set_theme`, so a CSS fade disagreed with it for 240ms, and the fade's `!important` would have
beaten every `!` utility once inside a cascade layer.

**Selects.** A plain native `<select>` everywhere, with Tailwind's reset undone by
`select { all: revert }`, so WebKit draws the real AppKit pop-up and opens a real NSMenu that
matches the running macOS. Rejected: Bits UI `Select` (a custom trigger can't match each macOS
version's pop-up) and shadcn's `native-select` (it sets `appearance: none` and draws its own
chevron).

**Tooltips at 250ms.** AppKit waits about a second; the owner chose fast tooltips (`c3a3775`).

**Long lists: virtua.** Requests (500 rows) and Logs (3,000 variable-height lines) render through
`virtua`'s `VList`: measured heights, prepend anchoring and stick-to-bottom are where hand-written
windowing goes wrong. Rejected: `@tanstack/svelte-virtual` (store-based adapter in a runes
codebase).

**virtua 0.53.3, measured (Phase 1, ui-lab in Playwright WebKit).** The `VList` API the port
relies on exists and behaves: props `data`, `getKey`, `itemSize`, `shift`, `onscroll`,
`keepMounted`; methods `scrollToIndex`, `getScrollOffset`, `getScrollSize`, `getViewportSize`
(plus `scrollTo`, `scrollBy`, `findItemIndex`, `getItemOffset`, `getItemSize`). With 500 fixed
28px rows in a 280px viewport: `getScrollSize()` 14000, `getViewportSize()` 280, 18 rows in the
DOM at rest and 25 after a 600px wheel scroll; `onscroll` fired with offset 600. 3,000
variable-height lines: 8 rows in the DOM, `scrollToIndex(last, { align: "end" })` lands at the end
(offset + viewport = scroll size) with the last line mounted.
- **`shift` needs a length change** (confirms 03-review R-21). Prepending 1 or 3 rows with
  `shift: true` while scrolled down keeps the first visible row in place (offset 2810 → 2838 →
  2922). At the 500-row cap (prepend + tail trim, length unchanged) `shift` does nothing: the offset
  stays 2810 and the content moves by the number of prepended rows (first visible id 398 → 399,
  399 → 402). Same at the 3,000-line cap for Logs (append 5 + head trim: 1500 → 1505, with or
  without `shift`). Virtualizer.svelte only calls its length-change update when `data.length`
  changes, which is where `shift` is read.
- **What works at the cap.** Requests (fixed height): anchoring by hand keeps the view exactly
  (3/3 rounds: record the first visible row's id and `offset − getItemOffset(index)`, update,
  then `scrollTo(getItemOffset(newIndex) + delta)`). Logs (variable height): index-based anchoring
  fails, because virtua's measured-size cache is per index and a head trim shifts every index.
  What keeps the view in both lists is **deferring the trim while the user is scrolled away**: an
  append without trim keeps the view (4/4 rounds), and a prepend without trim plus `shift: true`
  keeps it (3/3). Trim once the user is back at the live edge (Logs following at the bottom,
  Requests at the top), where the trimmed rows are off screen. Recommended for 2e/2f: the cap
  becomes "at most N while following", with a hard ceiling for safety.
- **`scrollToIndex` does not mount synchronously** (confirms R-22). After `scrollToIndex(400)` the
  offset is 11200 immediately, but row 400 is in the DOM neither synchronously nor after a
  microtask (so not after `await tick()` alone); it is there after one `requestAnimationFrame`.
  Set `aria-activedescendant` only once `#req-<id>` exists (wait a frame, then check).
- **`keepMounted` works.** Row 5 is unmounted after `scrollToIndex(450)` without it and stays
  mounted with `keepMounted={[5]}` (27 rows in the DOM). An index that was never rendered
  (`keepMounted={[480]}` while at the top) is mounted too. So the active row can't be recycled
  under `aria-activedescendant` by pointer scrolling.

**Requests list (Phase 2e).** No fallback was needed: Requests runs on virtua as planned
(`src/dashboard/RequestList.svelte`). A `role=listbox` wrapper holds focus and points
`aria-activedescendant` at the active row once it exists; rows are never focused; the active
index is passed to `keepMounted`. Selection is by request id, falling back to the row now at the
same index when its request is filtered out, cleared or trimmed by the cap. New requests are
queued and prepended in batches. Scrolled down below the cap, `shift` keeps the view; at the cap
the first visible row is re-anchored by hand, **reading and writing the viewport's `scrollTop`,
not virtua's offset**: virtua's `scrollTo` is async and `getScrollOffset()` only catches up on the
next scroll event, so in a burst of events every batch after the first anchored against a stale
offset and the view slid by a row (found in the real app with 6 requests in flight). Measured
with 500 rows in the real app (outside snapshot mode): 30 `[role=option]` in the DOM at rest and
at most 39 while wheel-scrolling, with no frame over 20 ms. The first visible row stays put
through bursts of 1 to 30 requests at the cap (ui-lab harness on the real component, and the
e2e debug build).

**E2E snapshot mode.** The e2e harness asserts on `document.body.innerText`, which a virtualized
list would truncate. A snapshot flag renders every row while the harness reads the text, so the
assertions stay as they are.

**Browser floor: macOS 13.3.** Tailwind v4's output needs WebKit 16.4 (`@property`, cascade
layers, `color-mix`), which macOS 13.3 ships. `minimumSystemVersion` is 13.3 and Vite's
`build.target` is `safari16.4` (owner decision, 2026-10-11).

**Class merging (Phase 1).** `cn` is `createCn` with Burrow's `text-2xs`/`text-callout`/`text-code`
in the font-size group: unconfigured, it read them as colours and dropped them next to one
(`cn("text-callout text-muted-foreground")` → `text-muted-foreground`). `tv` is
tailwind-variants' lite build, which never merges; `cn(variants(…), className)` merges once.
`src/lib/utils.test.ts` pins this.

**Placeholder contrast (Phase 1).** Placeholders use `--text-placeholder` (light `#6e6e73`, dark
`#aeaeb2`, ≥ 4.5:1 on every field background), not `--text-3`, which stays for disabled text and
decoration. WCAG 1.4.3 covers placeholder text.

**Cost.** Phase 0 (tooling and tokens, no components) grew the shared CSS chunk from 2.4 to
5.5 kB gzip and left JS unchanged. Phase 1 (all components installed, Toaster and Tooltip
provider mounted in both windows, nothing using them yet): popover JS 61.5 kB gzip (3.5 entry +
58.0 shared), dashboard JS 80.8 kB, CSS 16.3 kB (popover) and 19.5 kB (dashboard), the shared
chunk carrying every installed component's utilities and sonner's stylesheet. Under the popover
budget, so sonner stays statically imported (no lazy load). Budgets for the finished port: popover JS ≤ 70 kB gzip,
dashboard JS ≤ 150 kB, CSS ≤ 20 kB per page.

**Tooltip chunking (Phase 2a).** Phase 1's popover figure didn't include the tooltip
surface: nothing rendered `Tooltip.Content` yet, so floating-ui and Bits' popper layers were
tree-shaken. Settings is the first screen with Tips. Because the popover imported the
`ui/tooltip` barrel (for the provider), those modules then landed in the shared chunk and the
popover's JS went to 88.8 kB gzip. The popover now imports `tooltip-provider.svelte` directly:
popover 67.9 kB (3.4 entry + 64.5 shared), dashboard 116.6 kB. Phase 2i, which puts Tips in the
popover, brings floating-ui back to it (about +20 kB), so 2i needs O10 (lazy sonner) or a lazy
tooltip content to stay within 70 kB.

**Status dot hint (Phase 2b).** 02 maps StatusDot to "utilities + `Tip` with `statusHint`", but
StatusDot is also in the popover, and a Tip inside it would put `ui/tooltip` (floating-ui) into
the popover's JS before 2i has made room for it. So StatusDot keeps its `use:tooltip` hint behind a
`hint` prop (default on), and a ported screen wraps the dot in its own Tip with `hint={false}`
(TunnelDetail does). The popover and the Tunnels list switch in 2i and 2g. The dot is not a tab
stop: the status word sits next to it as text. Popover JS after 2b: 68.0 kB (3.4 + 64.6), up
49 B on 2a, 27 B of which comes with main's StatusDot unchanged (the shared chunk exports more
names to the dashboard).

**Identifier fields skip the text system (Phase 2b).** In the real app, typing "zzz" into the
rename field raised a spelling bubble whose dismissal swallowed the first Escape (the keydown
never reached the page), and "alpha3" was saved as "Alpha3". The rename, Local address and token
inputs set `autocorrect="off" autocapitalize="off" spellcheck="false" writingsuggestions="false"`,
and so does the Combobox's search field (Bits already sets the first and third).
Settings' path and redact fields and the Add sheet's fields are candidates for the same (their
phases).

**Combobox matching (Phase 2b).** The project picker filters by a case-insensitive substring of
the trimmed search, not Bits' fuzzy scorer, which hid "Work" when the search was " Work" and
let Enter pick "website" for "web". An exact match is listed and highlighted first, so Enter picks
it; any other non-empty search lists `Create "<name>"` first and highlighted, with the matches
after it. It opens with the current project highlighted, so Return alone changes nothing. The
filtering and order are done in `comboRows` (`src/lib/combobox.ts`, unit-tested), and Bits runs
with `shouldFilter={false}` rather than a custom `filter`: with a filter Bits reorders the rows by
moving their DOM nodes, which is fragile under Svelte's keyed blocks and would leave the order
to its score sort.

**Alternatives rejected.** Bits UI alone with hand-written styles (an earlier spike: every
component's look written from scratch, and its dark variant missed "System" in dark mode).
Keeping scoped CSS (1,440 lines of duplicated per-component styles, no shared primitives, and the
accessibility fixes the port brings would have to be hand-built).
