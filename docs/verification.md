# Verification log

> Recorded before the app was renamed from Burrow to Underlane (2026-10-11). Paths, bundle ids and
> log lines below are as they were at the time: `Burrow.app` is now `Underlane.app`, and
> `com.davidsonsns.burrow` is now `com.davidsonsns.underlane`. These historical screenshots also
> predate the Lane Cut logo selected on 2026-10-12 and show the original arch icon.

Checks were done on this Mac on 2026-10-10: macOS 27.0.1, Apple Silicon, a 2560×1080 display,
cloudflared 2026.9.1. Two builds were used:

- `make e2e`: the debug build with the loopback control server. Used for the automated
  end-to-end check and to drive the screenshot session (`scripts/screenshots.mjs`).
- `make build`: the release build, installed to `~/Applications/Burrow.app`. Used for launch at
  login, auto-start and crash checks.

Screenshots are window-only captures (`scripts/capture-window.swift`) or menu bar strips around
Burrow's status item. The menu bar on this Mac is managed by Hidden Bar, which collapses new items.
The scripts expand it for the capture and collapse it again afterwards.

## 1. End-to-end check (no Cloudflare account)

`make e2e` (`e2e/run.mjs`) launches the real app on a throwaway data folder, starts a local echo
server and exposes it as a quick tunnel with inspection on. Output of the run:

```
[  0.0s] echo server on 127.0.0.1:61155
[  0.9s] app running (pid 10072), data dir /var/folders/…/burrow-e2e-kiBxpJ
[  0.9s] ok: menu bar count is 0 before starting (title hidden)
[  0.9s] ok: quick tunnel created with inspection on
[  7.0s] connected: https://synthetic-quick-11.trycloudflare.com (inspecting=true)
[  7.0s] ok: inspection proxy is active
[  7.0s] ok: menu bar count is 1 while the tunnel runs
[  7.1s] synthetic-quick-11.trycloudflare.com resolves to 104.16.230.132 (via 1.1.1.1)
[  7.6s] POST https://synthetic-quick-11.trycloudflare.com/e2e -> 200 {"echoed":{"hello":"burrow","marker":"burrow-e2e-mv2177nh",…
[  8.2s] ok: SQLite has the POST with status 200
[  8.2s] ok: SQLite has the request body
[  8.2s] ok: SQLite has the response body
[  8.2s] ok: Authorization header was redacted before storage
[ 10.2s] ok: request viewer lists the request
[ 10.2s] ok: request viewer shows the (pretty-printed) response body
[ 10.2s] ok: request detail pane is open
[ 12.1s] ok: log viewer shows live cloudflared logs
[ 12.2s] ok: menu bar count is back to 0 after stopping
[ 12.2s] ok: no cloudflared process left running
[ 12.2s] E2E PASSED
```

After the pre-ship review fixes (decisions.md §15), `make e2e` was run again. It passed, with two
assertions added for the Request tab of the viewer:

```
[ 10.1s] connected: https://synthetic-quick-9.trycloudflare.com (inspecting=true)
[ 34.5s] synthetic-quick-9.trycloudflare.com resolves to 104.16.230.132 (via 1.1.1.1)
[ 35.2s] POST https://synthetic-quick-9.trycloudflare.com/e2e -> 200 {"echoed":{"hello":"burrow","marker":"burrow-e2e-mv22qqcz",…
[ 35.7s] ok: SQLite has the request body / response body / Authorization redacted
[ 39.7s] ok: request viewer shows the (pretty-printed) request body
[ 39.7s] ok: request viewer shows the Authorization header redacted
[ 41.6s] ok: menu bar count is back to 0 after stopping
[ 41.7s] E2E PASSED
```

DNS for the new hostname took 24 s this time. The harness waits on 1.1.1.1 rather than the system
resolver, so a negative cache entry can't break it.

Final run, after the e2e switched to the real `curl` binary (`curl --resolve <host>:443:<ip from
1.1.1.1> -X POST … --data-raw '<json>'`):

```
[  5.4s] connected: https://synthetic-quick-13.trycloudflare.com (inspecting=true)
[  5.4s] ok: menu bar count is 1 while the tunnel runs
[  6.0s] curl -X POST https://synthetic-quick-13.trycloudflare.com/e2e (JSON body) -> 200 {"echoed":{"hello":"burrow","marker":"burrow-e2e-mv24l6g2",…
[  6.5s] ok: SQLite has the POST with status 200 / request body / response body / Authorization redacted
[ 10.5s] ok: request viewer shows the (pretty-printed) request body
[ 12.5s] ok: menu bar count is back to 0 after stopping
[ 12.5s] E2E PASSED
```

Notes on what the checks read:

- The menu bar count comes from the title of the real `NSStatusItem` button, read on the main
  thread through `tray-icon`'s `ns_status_item()`. The count is hidden at zero, so 0 is an empty
  title (decisions.md §9).
- SQLite is read with the `sqlite3` CLI.
- The "request viewer" text is the dashboard's rendered `innerText` after the e2e harness opens
  the Requests view and selects the newest request.

## 2. Importing an existing config.yml

`~/.cloudflared` holds `example.yml` (a named tunnel with four hostnames). It also has a
`config.yml.bak`, which the scan correctly ignores. The Add sheet lists `example.yml` with its
hostnames, and after import the tunnel shows every hostname → service mapping, parsed from its
ingress rules.

| Add sheet | Imported tunnel |
| --- | --- |
| ![](screenshots/01-import-sheet.png) | ![](screenshots/02-imported-config.png) |

`example.yml` was **imported but never started**. That tunnel is already running on this Mac under the
owner's own script (`cloudflared tunnel --config ~/.cloudflared/example.yml … run`, pid 48763). Burrow
detects that case and refuses to start a second connector (decisions.md §3). The file's
modification time and contents were unchanged after import.

## 3. Running two or more tunnels at once

Three quick tunnels ran at once, one with inspection on, plus traffic through the inspected one.
The menu bar count read 3.

| Menu bar | Popover |
| --- | --- |
| ![](screenshots/03-menu-bar-count.png) | ![](screenshots/04-popover-three-tunnels.png) |

![Dashboard, dark](screenshots/05-dashboard-dark.png)
![Requests, dark](screenshots/06-requests-dark.png)
![Logs, dark](screenshots/07-logs-dark.png)

## 4. Theme switching, light and dark dashboards

The theme was switched from Dark to Light through the Appearance setting. The native window
appearance follows the theme too, so the sidebar and popover vibrancy match.

| Dark | Light |
| --- | --- |
| ![](screenshots/08-settings-dark.png) | ![](screenshots/09-settings-light.png) |
| ![](screenshots/05-dashboard-dark.png) | ![](screenshots/10-dashboard-light.png) |
| ![](screenshots/06-requests-dark.png) | ![](screenshots/11-requests-light.png) |
| ![](screenshots/04-popover-three-tunnels.png) | ![](screenshots/12-popover-light.png) |

## 5. Auto-starting tunnels

`web` and `api` were marked **Start automatically**, and Burrow was quit. All cloudflared processes
stopped with it ("after quit: 0 cloudflared processes left"). Burrow was then relaunched with no
further input. `web` and `api` came up on their own, `docs` stayed stopped, and the menu bar read 2.

| Popover after relaunch | Menu bar after relaunch |
| --- | --- |
| ![](screenshots/13-autostart-after-relaunch.png) | ![](screenshots/14-menu-bar-after-autostart.png) |

This check found a real bug: starting tunnels from app setup panicked, because setup runs outside
the Tokio runtime. It's fixed, and a regression test is in
`supervisor::can_start_from_a_thread_outside_the_runtime`.

## 6. Launch at login (simulated)

I didn't log out, because that would end the session running the build. The release build was
used instead, as follows:

1. `make install` copied the release build to `~/Applications/Burrow.app`. In the real data folder
   (`~/Library/Application Support/com.davidsonsns.burrow`), `example.yml` was imported (not
   auto-started), and a `login-demo` quick tunnel to `localhost:8787` was added with **Start
   automatically** on.
2. `~/Applications/Burrow.app/Contents/MacOS/Burrow --login-item enable` registered the app via
   `SMAppService.mainApp`. Its status went from `not_found` to `enabled`. System Settings → General
   → Login Items lists Burrow under **Open at Login** (and under Background App Activity):

   ![](screenshots/15-login-items.png)

3. With Burrow not running, it was started the way a login item is: through LaunchServices,
   with no arguments and detached from any terminal (`open -g ~/Applications/Burrow.app`). The
   process tree afterwards:

   ```
   29700     1 ~/Applications/Burrow.app/Contents/MacOS/Burrow                         ← parent: /sbin/launchd
   29714 29700 ~/Applications/Burrow.app/Contents/MacOS/Burrow --cloudflared-shim /opt/homebrew/bin/cloudflared tunnel --config …/com.davidsonsns.burrow/neutral.yml …
   29715 29714 /opt/homebrew/bin/cloudflared tunnel --config …/com.davidsonsns.burrow/neutral.yml … --url http://localhost:8787
   48763 48752 cloudflared tunnel --config ~/.cloudflared/example.yml … run                  ← the owner's own process, untouched
   ```

   The auto-start tunnel came up by itself. Clicking the real status item showed it connected,
   with a count of 1:

   | Menu bar | Popover (opened by clicking the status item) |
   | --- | --- |
   | ![](screenshots/16-login-launch-menu-bar.png) | ![](screenshots/17-login-launch-popover.png) |

## 6b. Release-build pass (`node scripts/release-check.mjs`)

To make sure the screenshots don't only come from the debug build, the **installed release build**
(`~/Applications/Burrow.app`, which has no control server) was driven with real clicks:

```
[release-check] seeded tunnel: autostart-demo|quick|1|1
[release-check] login item before: {"status":"not_registered","enabled":false,"message":null}
[release-check] login item enable: {"status":"enabled","enabled":true,"message":null}
[release-check] app pid 74864, parent: /sbin/launchd, tty: ??
74864     1 ??  …/Applications/Burrow.app/Contents/MacOS/Burrow
74944 74864 ??  …/Burrow --cloudflared-shim /opt/homebrew/bin/cloudflared tunnel --config …
74945 74944 ??  /opt/homebrew/bin/cloudflared tunnel --config …/com.davidsonsns.burrow/neutral.yml …
[release-check] auto-started tunnel public URL: https://synthetic-quick-4.trycloudflare.com
[release-check] curl https://synthetic-quick-4.trycloudflare.com/release-check -> 404   (the origin's own 404)
[release-check] captured in SQLite by the release app: GET|/release-check?token=[redacted]|404
[release-check] theme after clicking Light: light
[release-check] theme after clicking Dark: dark
[release-check] theme restored: system
[release-check] after quit, Burrow/cloudflared processes left: 0
[release-check] login item disable: {"status":"not_registered","enabled":false,"message":null}
```

The 404 is Python `http.server` (the origin) answering for a file it doesn't have. It shows the
request went through the public URL, cloudflared, Burrow's proxy and the origin, and back.

| Menu bar after login launch | Popover (auto-started tunnel) | Dashboard |
| --- | --- | --- |
| ![](screenshots/18-release-menu-bar.png) | ![](screenshots/19-release-popover.png) | ![](screenshots/20-release-dashboard.png) |

| Settings: Light clicked | Dashboard, light | Settings: Dark clicked |
| --- | --- | --- |
| ![](screenshots/21-release-settings-light.png) | ![](screenshots/22-release-dashboard-light.png) | ![](screenshots/23-release-settings-dark.png) |

## 6c. The simulated login launch uses the same mechanism as a real login

Apps that macOS opened at this Mac's last real login run as LaunchServices launchd jobs,
`application.<bundle-id>.<n>`, with `/sbin/launchd` as their parent. Raycast and Warp are both in
**Open at Login** (screenshot 15). Burrow, registered through `SMAppService` and launched the
login-item way, runs as exactly the same kind of job:

```
1418   application.com.raycast.macos.1911346.184570496          <- started by the real login
1417   application.dev.warp.Warp-Stable.196840554.196840559     <- started by the real login
78770  application.com.davidsonsns.burrow.198522606.198522611   <- Burrow, simulated login launch
Burrow  pid=78770 parent=/sbin/launchd tty=??
Raycast pid=1418  parent=/sbin/launchd tty=??
```

In that launch, a tunnel marked **Start automatically** came up with no start command from anyone.
It logged `Registered tunnel connection` and got the public URL
`https://synthetic-quick-8.trycloudflare.com`, with cloudflared running under Burrow's
shim. Afterwards: 0 cloudflared processes left on quit, the test tunnel was deleted, and the login
item was unregistered again.

## 6d. Login-item launch with macOS's login Apple event (`node scripts/login-item-check.mjs`)

When macOS opens a login item, its launch `oapp` Apple event carries
`keyAEPropData = keyAELaunchedAsLogInItem` ('lgit'). That is the only signal an app gets that it
was opened at login. Burrow reads it in `applicationDidFinishLaunching`, records the launch reason,
shows it in Settings, and opens no window when launched at login.
`scripts/launch-as-login-item.swift` launches the app through LaunchServices with exactly that
event. Run against the installed release build:

```
[login-item-check] register: {"status":"enabled","enabled":true,"message":null}
[login-item-check] plain open -> last_launch = {"at":1791621178855,"reason":"normal"}          <- control
[login-item-check] launched …/Burrow.app as a login item (pid 81954)
[login-item-check] login-item launch -> last_launch = {"at":1791621181718,"reason":"login_item"}
[login-item-check] pid 81954, parent /sbin/launchd, tty ??
[login-item-check] launchd job: 81954  0  application.com.davidsonsns.burrow.198534333.198534338
[login-item-check] windows opened by the login launch: 0 (none expected)
[login-item-check] auto-started "starts-at-login": https://synthetic-quick-6.trycloudflare.com; Registered tunnel connection
[login-item-check] after quit: 0 cloudflared left
[login-item-check] unregister: {"status":"not_registered","enabled":false,"message":null}
```

| Menu bar | Popover: tunnel auto-started at login | Settings: "Opened by macOS at login" |
| --- | --- | --- |
| ![](screenshots/24-login-item-menu-bar.png) | ![](screenshots/25-login-item-popover.png) | ![](screenshots/26-login-item-settings.png) |

## 7. Crash safety and quitting

- **App SIGKILLed:** `kill -9` on the app made the watchdog shim stop cloudflared within 1 s
  ("cloudflared gone 1s after the app was SIGKILLed"). The next launch started cleanly, with no
  "reopen windows?" prompt, because state restoration is disabled. The auto-start tunnel came
  back.
- **Normal quit:** `osascript -e 'quit app "Burrow"'` stopped every tunnel; 0 processes remained.

## 8. No terminal needed

None of the scenarios above depend on a terminal staying open:

- The release app runs under `launchd` (PPID 1).
- The tunnels are children of the app, through the shim.
- Logs come from cloudflared's stderr pipe into SQLite.

Terminals were only used to *trigger* steps (the build, `open`, the `--login-item` registration)
and to inspect process trees.

The screenshot session ran after a night of testing, and trycloudflare.com rate-limited new quick
tunnels (`429`, error code 1015). The `web` tunnel in screenshots 05 and 10 shows "6 restarts".
That is the crash backoff (1 s, 2 s, 4 s … 32 s) retrying until provisioning succeeded, which shows
the restart-with-backoff behaviour working.

## 9. UI migration (Tailwind v4 + shadcn-svelte)

The UI moved from per-component scoped CSS to Tailwind v4 and shadcn-svelte (Luma, tuned to macOS),
one screen at a time (decisions.md §17). Both sets were taken the same way, by
`node scripts/ui-shots.mjs target/debug/bundle/macos/Burrow.app --label <name>` on the e2e debug
build: two quick tunnels to local echo servers (one inspected), three requests, every view in each
theme. Before is `72c52ce` (pre-migration), after is the end of Phase 3 (base.css deleted). The
hostnames, ports and times differ between runs; the `system-*` shots follow the OS appearance
(Dark for both sets).

- Before: [`screenshots/ui-migration/baseline/`](screenshots/ui-migration/baseline/)
- After: [`screenshots/ui-migration/final/`](screenshots/ui-migration/final/)

| Before (light) | After (light) |
| --- | --- |
| ![](screenshots/ui-migration/baseline/light-tunnels.png) | ![](screenshots/ui-migration/final/light-tunnels.png) |
| ![](screenshots/ui-migration/baseline/light-requests.png) | ![](screenshots/ui-migration/final/light-requests.png) |
| ![](screenshots/ui-migration/baseline/light-logs.png) | ![](screenshots/ui-migration/final/light-logs.png) |
| ![](screenshots/ui-migration/baseline/light-settings.png) | ![](screenshots/ui-migration/final/light-settings.png) |
| ![](screenshots/ui-migration/baseline/light-add.png) | ![](screenshots/ui-migration/final/light-add.png) |
| ![](screenshots/ui-migration/baseline/light-popover.png) | ![](screenshots/ui-migration/final/light-popover.png) |

| Before (dark) | After (dark) |
| --- | --- |
| ![](screenshots/ui-migration/baseline/dark-tunnels.png) | ![](screenshots/ui-migration/final/dark-tunnels.png) |
| ![](screenshots/ui-migration/baseline/dark-requests.png) | ![](screenshots/ui-migration/final/dark-requests.png) |
| ![](screenshots/ui-migration/baseline/dark-logs.png) | ![](screenshots/ui-migration/final/dark-logs.png) |
| ![](screenshots/ui-migration/baseline/dark-settings.png) | ![](screenshots/ui-migration/final/dark-settings.png) |
| ![](screenshots/ui-migration/baseline/dark-add.png) | ![](screenshots/ui-migration/final/dark-add.png) |
| ![](screenshots/ui-migration/baseline/dark-popover.png) | ![](screenshots/ui-migration/final/dark-popover.png) |

Rows, top to bottom: Tunnels, Requests (a request selected), Logs, Settings, Add tunnel sheet, Menu bar popover.

Checked on the final build: `npm run check` 0/0, `npm test`, `make test`, `make e2e` (`E2E PASSED`),
and the keyboard paths (Tab through the sidebar and toolbar, the Add sheet's tabs and Escape, the
popover's arrow keys) with a visible focus ring on each.

## Cleanup after verification

- The `login-demo` tunnel was removed from the real data folder, so no public quick tunnel starts
  at login. `example.yml` remains imported, without auto-start.
- **Launch at login was unregistered** afterwards (`Burrow --login-item disable`, status
  `not_registered`), to leave this Mac as it was. Turn it back on in Settings → General → Launch
  at login.
- `~/Applications/Burrow.app` is the latest release build.
- The owner's own `example.yml` cloudflared process (pid 48763) was never touched.

## Phase 0 foundations — 2026-10-12

Implemented the reviewed workspace kickoff on `codex/phase0-foundations`. Phase 0 is complete;
later phases remain separate work. Baseline was clean `main` at `7934847`.

| Fix | Result | Verification |
| --- | --- | --- |
| A1 token-file identity | PASS | Both `--token-file PATH` and `--token-file=PATH` exclude default-config matches; explicit config/name positives remain. |
| A2 accurate matching | PASS | Bare cloudflared and Access commands do not identify a running tunnel; positive `tunnel run` cases remain. |
| A3 external argv secrets | PASS | Token flags, credential JSON with spaces and secret assignments are redacted before external-process hints/Debug. Parsed and persisted logs contain no invented secrets. |
| A4 target-or-neutral config | PASS | Runner constructs explicit target/neutral config before the subcommand, preserves target cwd, and rejects config-free tunnel calls even with leading global flags. Version stays config-free. Conflicting-default real contracts pass. |
| A5 documentation drift | PASS | Decisions §9 adds Restart Running Tunnels; §11 documents process/login-shell PATH search and actual fallback locations. Research §2.1 describes the app's Keychain + TUNNEL_TOKEN transport while retaining the CLI alternatives. |

| Foundation | Result | Verification |
| --- | --- | --- |
| B1 migration ladder | PASS | Production schema remains v1. Six tests cover fresh initialization, populated settings/projects/tunnels/logs/requests, repeat-open, test-only v2 advancement, rollback (including commit-time failure) and future rejection before writes. |
| B2 secrets | PASS | Zeroizing, non-serializable Secret wraps Keychain→RunSpec→child tokens and secret stdout. Debug/Display, hints, callbacks, malformed tails and persisted child logs redact invented values. |
| B3 platform boundary | PASS | Core platform owns home lookup, ps parsing, process groups, signals and cleanup. Existing macOS supervisor/shim/orphan regressions pass. Unsupported process operations return explicit errors outside Unix. |
| B4 one-shot runner | PASS | Timeout, cancellation, dropped caller, queued cancellation/drop, process-group escalation/reaping and permit release pass against fake children, including a SIGTERM-ignoring descendant. Actual inherited secret environment values are tested in an isolated test subprocess. Only existing version/validate calls were ported. |
| B5 classification | PASS | Rc-0 usage/login refusal are failures; outdated warnings and already-routed results succeed. Known error fixtures, unknown/nonzero/malformed output and sanitized hints are covered. Arbitrary JSON names/IDs and validation filenames do not become error anchors. |
| B6 fixtures/contracts | PASS | Shared Python fixture retains supervisor behavior and adds one-shot command/error fixtures. The ignored real suite was explicitly executed, including the safe offline hidden-flag probe. |

Checks executed:

```text
rtk make test                         # baseline: 51 Rust passed, 1 ignored; 42 frontend passed
rtk proxy make test                   # final: 74 Rust passed, 2 ignored; 42 frontend passed
rtk proxy cargo test -p underlane-core --test cli_contract -- --ignored --nocapture
                                      # 1 real contract test passed; 8 direct probes + 2 production-runner target checks
rtk proxy make e2e                    # E2E PASSED; HTTP 200, SQLite/UI bodies, redaction, menu count and no orphan
rtk proxy git diff --check            # passed
```

Type check: 0 errors, 0 warnings. The Vite native-config advisory about existing `__dirname`
usage also appeared at baseline; frontend configuration was not changed.

Real binary: `/opt/homebrew/Cellar/cloudflared/2026.9.1/bin/cloudflared`, version **2026.9.1**,
built **2026-09-10T20:52:23Z**. All probes used cleared environment, isolated HOME/cwd,
synthetic configs and a five-second timeout. No real cert, credential-file or token contents
were read. No account, DNS, binary-update or service-control command was executed.

Exact argv from the final offline run (the binary above precedes each array):

```json
["--version"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpotCUog/valid.yml","ingress","validate"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpotCUog/invalid.yml","ingress","validate"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpotCUog/neutral.yml","ingress","validate","--json","{\"ingress\":[{\"hostname\":\"contract.example.invalid\",\"service\":\"http://localhost:43210\"},{\"service\":\"http_status:404\"}]}"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpotCUog/neutral.yml","ingress","validate","--json","{invalid-json}"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpotCUog/valid.yml","ingress","rule","https://contract.example.invalid/path"]
["access","ssh-config","--hostname","ssh.example.invalid"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpotCUog/invalid.yml","--protocol","http2","--ha-connections","1","ingress","validate"]
```

Exit codes in that order: `0, 0, 1, 0, 1, 0, 0, 1`. The two target-validation argv were also
executed through the production runner and produced the expected success/failure despite the
conflicting default config. The temporary fixtures are removed by the suite; rerun the test to
regenerate them. Hidden flags were accepted by the offline ingress validator; connector behavior
with those flags is outside this phase.

Frontend source/configuration/dependency files are unchanged from baseline. E2E rebuilt a stale
generated dashboard bundle left from `1b555fe`, before the existing sidebar-weight fix. The
original generated bundle was reconstructed in an isolated directory and all **10 files** were
restored to their exact baseline SHA-256 hashes; no frontend change is part of Phase 0.

Independent review cleared the final implementation after config-policy/classifier/hint
regressions were fixed. Deferred: account/login/DNS/service/update features, new schema fields,
later-phase UI and the existing proxy latency benchmark. Non-Unix compilation was not run;
cross-platform support remains outside v1. No Phase 0 check remains UNVERIFIED.

## Phase 1 P1 — config-changed badge — 2026-10-12

Implemented P1 only on `codex/phase1-config-tools`, starting from main `8a01c65`. P2 (rule tester)
and P3 (runtime explainers) remain deferred. Imported source files stay read-only; schema stays v1.

| Gate | Result | Evidence |
| --- | --- | --- |
| Exact loaded source | PASS | Fingerprint is computed from the parsing read used for the derived config. The fake-connector test changes the file during gated validation and proves the baseline/derived copy still describe the loaded bytes. Metadata publishes only after successful spawn. |
| Lifecycle and failures | PASS | Tests cover A→B edits, restored bytes, comments, atomic replacement, deletion and read failure. Comparisons leave the child/derived A unchanged; successful restart loads B. Validation/spawn failures retain the previous baseline; internal respawns advance revision; stop clears comparison. |
| Race and secrecy boundaries | PASS | Backend discards results after run/revision/baseline/stop/path changes, using synchronous supervisor state rather than delayed app events. Frontend covers selection/path/run/revision/unmount races and duplicate checks. Quick/token tunnels have no imported comparison. Synthetic config secrets are absent from serialized comparison state and Debug. |
| Visible behavior and actions | PASS | Native manual checks verified focus-triggered badge, missing-file warning, registered Xcode opening, Finder selection, errors for both missing-file actions and keyboard Restart in light/dark themes. Restart clears the badge and updates the visible service. No automatic restart occurs. |

Checks executed:

```text
rtk proxy make test
  baseline: 74 Rust passed, 2 ignored; 42 frontend passed; type check 0 errors/0 warnings
  final:    83 Rust passed, 2 ignored; 55 frontend passed; type check 0 errors/0 warnings
rtk proxy make e2e
  E2E PASSED: real account-free quick tunnel, HTTP 200, SQLite/UI bodies, redaction,
  menu count and no leftover cloudflared process
rtk proxy node e2e/config-badge.mjs target/debug/bundle/macos/Underlane.app
  CONFIG BADGE E2E PASSED: 20 local assertions against the real app/shared fake connector
rtk proxy cargo test -p underlane-core --test cli_contract -- --ignored --nocapture
  1 real offline contract test passed, 8 raw probes + 2 production runner target checks
rtk proxy git diff --check
  passed
```

An initial full-suite failure exposed parallel test fixtures using the same clock-based file
name. Their names now include an atomic per-process sequence, and the full suite passes. No
dependency was added. The existing Vite `__dirname` advisory remains unchanged from baseline.

Manual fixture: `/tmp/underlane-p1-qa/config.yml`, separate data folder and fake binary; no
account credentials were used. Open in Editor launched `/Applications/Xcode.app` and displayed
that file. Editing `43210`→`43211`, saving and returning focus showed the warning while the
running mapping remained `43210`. Keyboard navigation to the badge's Restart and Return
loaded `43211` and cleared the warning. Repeating with an external comment edit verified the
same keyboard behavior in dark theme. Finder selected the exact disposable YAML. Moving the
source aside produced an unreadable warning while the child remained connected; both source
actions produced visible missing-file errors. The fixture was restored, and the connector was
stopped and the QA app exited after verification. Normal installed app settings were untouched.

| Before P1 | After an external edit (light) |
| --- | --- |
| ![Before badge](screenshots/phase1-config-badge/before-light.png) | ![Changed config badge](screenshots/phase1-config-badge/after-light.jpg) |

| After an external edit (dark) | Source actions (dark) |
| --- | --- |
| ![Dark badge](screenshots/phase1-config-badge/after-dark.jpg) | ![Editor and Finder](screenshots/phase1-config-badge/source-actions-dark.jpg) |

Real offline binary: `/opt/homebrew/Cellar/cloudflared/2026.9.1/bin/cloudflared`, **2026.9.1**,
built **2026-09-10T20:52:23Z**. Cleared environment, isolated HOME/config/cwd, five-second bounds.
Exact argv from this run (binary above precedes each array):

```json
["--version"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpulgGFf/valid.yml","ingress","validate"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpulgGFf/invalid.yml","ingress","validate"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpulgGFf/neutral.yml","ingress","validate","--json","{\"ingress\":[{\"hostname\":\"contract.example.invalid\",\"service\":\"http://localhost:43210\"},{\"service\":\"http_status:404\"}]}"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpulgGFf/neutral.yml","ingress","validate","--json","{invalid-json}"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpulgGFf/valid.yml","ingress","rule","https://contract.example.invalid/path"]
["access","ssh-config","--hostname","ssh.example.invalid"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpulgGFf/invalid.yml","--protocol","http2","--ha-connections","1","ingress","validate"]
```

Exit codes: `0, 0, 1, 0, 1, 0, 0, 1`. The valid/invalid target commands also passed through the
production runner with the expected result. Fixtures are temporary; the suite regenerates them.
These existing contracts are regression checks; P1's file comparisons spawn no cloudflared command.

Independent source review found no actionable defects. No required P1 check remains UNVERIFIED.
Deferred: P2/P3, file watchers, config diffs and the other roadmap features. Non-Unix compilation
was not run; platform support remains unchanged.

## Phase 1 P3 — runtime failure explainers — 2026-10-12

Implemented P3 directly on main from `cbe3a49`, as requested. The config-change badge and
failure explainers are complete; wider Phase 1 features remain separate. No dependency or schema
migration was added.

| Gate | Result | Evidence |
| --- | --- | --- |
| Cause and next action | PASS | Known preparation, spawn, credentials, config, origin and edge failures receive safe cause/action/raw explanations. Structured message/error fields participate; JSON names/IDs, malformed payload fields and path operands do not. Unknown errors retain useful raw evidence without an invented diagnosis. |
| Classification regressions | PASS | Rc-0 usage and refused-login errors remain failures; warning/info/success records do not create explanations. Tests retain successful validation payloads and cover quoted filenames with spaces/contractions, origin status 403/429 attribution and a known error followed by 40 informational lines before exit. |
| Redaction and lifecycle | PASS | URL credentials, sensitive multiline queries, malformed JSON token fields and echoed run tokens stay absent from explanations, hints, Debug and persisted logs. Retry/backoff, Stop and user Restart remain intact. Edge recovery clears obsolete errors without replacing the child. Readiness cannot claim origin recovery, so the observed origin error persists until Stop/Restart. |
| UI and controls | PASS | Native keyboard Return toggles Raw diagnostic; Tab/Return copies it. The AX tree names Raw diagnostic and Copy diagnostic. Pasting into an unsaved disposable input confirmed the exact safe clipboard value. Light/dark screenshots, unknown-error raw evidence, validation replacement and recovery clearing were checked. |

Checks executed:

```text
rtk proxy make test
  106 Rust passed, 2 ignored; 73 frontend passed; Svelte check 0 errors/0 warnings
rtk proxy make e2e
  E2E PASSED: real account-free quick tunnel, HTTP 200, SQLite/UI bodies,
  redaction, menu count 0→1→0 and no leftover cloudflared
rtk proxy npx tauri build --debug --bundles app --features e2e
  final app rebuilt after preserving exit diagnostics through informational bursts
rtk proxy node e2e/failure-explainers.mjs target/debug/bundle/macos/Underlane.app
  FAILURE EXPLAINERS E2E PASSED: 17 assertions against the real app/shared fake
rtk proxy cargo test -p underlane-core --test cli_contract -- --ignored --nocapture
  1 real offline contract passed: 9 raw probes, 2 production ingress matches,
  and 2 production target-validation regressions
rtk proxy git diff --check
  passed
```

Independent review found and verified fixes for multiline URL query redaction, quoted path
operands/contractions, origin errors cleared by edge readiness, and invented preparation exit
context. Final parent review added the informational-burst regression and plain-text exit
status formatting. The first local app run used an overly literal assertion requiring the word
“catch-all”; it now checks the actual existing `http_status:404` fix, and the final run passes.
The existing Vite `__dirname` advisory remains unchanged.

Native fixture: `/tmp/underlane-p3-qa`, with separate data and the shared fake. A held edge error
showed cause/action while retaining Starting, Stop and Restart. Return closed/reopened the
diagnostic; keyboard Copy diagnostic produced a copied toast, and paste confirmed the redacted
value without saving it. Switching the fake's readiness to healthy cleared the explanation
with the same connector PID. A malformed unknown failure in dark theme showed a neutral cause,
next action, exit status 7 and redacted raw output while Try again now/backoff remained present.
A disposable imported config's Validate action showed the catch-all fix; after externally fixing
the file, successful validation replaced it with “The config is valid” and removed the old raw
disclosure. Both fixture tunnels were stopped and the QA app exited. Normal app data was untouched.

| Before P3 | Expanded known diagnostic (light) |
| --- | --- |
| ![Previous raw notice](screenshots/phase1-failure-explainers/before.jpg) | ![Cause, action and keyboard disclosure](screenshots/phase1-failure-explainers/expanded-light.jpg) |

![Unknown redacted diagnostic and retry feedback in dark theme](screenshots/phase1-failure-explainers/unknown-dark.jpg)

Real offline binary: `/opt/homebrew/Cellar/cloudflared/2026.9.1/bin/cloudflared`, **2026.9.1**,
built **2026-09-10T20:52:23Z**. The applicable validation regressions ran with a cleared
environment, isolated HOME/config/cwd, conflicting default config and five-second bounds.
Exact argv from this P3 run (binary above precedes each array):

```json
["--version"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpOsSOBp/valid.yml","ingress","validate"]
["tunnel","--config","/var/folders/k9/rcb7cjf15y19jxyzfgqvdpnr0000gn/T/.tmpOsSOBp/invalid.yml","ingress","validate"]
```

The valid/invalid calls exit 0/1 respectively and also pass through the production runner with
the expected result. The same suite explicitly re-executed the P2 first/catch-all and JSON
validation contracts. Synthetic config/credential fixtures were used; no login, account/DNS
mutation, binary update, service control or external connector restart was performed.

No required P3 gate remains UNVERIFIED. Non-Unix compilation and an actual VoiceOver reading
session were not run; native AX names and keyboard behavior were verified. Origin errors remain
observed-error notices until Stop/Restart because no origin recovery probe was added. The wider
roadmap remains deferred.

## Hostname editor — 2026-10-13

The approved compact modal is implemented for local ingress config tunnels: borderless editable
rows, staged add/remove with Undo, a footer 404 checkbox with explicit alternate fallback, and
Cancel/Save. Save validates before updating source bytes, creates an exact backup, preserves
advanced options and macOS file metadata, rejects source conflicts and newly shadowed routes,
and restarts only a live non-stopping tunnel. New/changed hostname spelling is normalized to
lowercase; existing no-op bytes remain intact.

- `cargo test --workspace`: 123 passed, 2 existing ignored tests. The final serial build passed
  all suites including doctests; an intermediate concurrent build produced a transient rustdoc
  dependency-resolution failure.
- `npm test`: 61 passed; `npm run check`: zero errors/warnings.
- Debug/e2e Tauri bundle built and signed successfully.
- `node e2e/hostnames.mjs target/debug/bundle/macos/Underlane.app`: 29 native checks passed using
  disposable config/database and the synthetic connector. Includes one-restart verification,
  stopped-state refresh, backup bytes, advanced options, conflicts, validation rejection,
  unsupported tunnel boundaries, catch-all retention, and new/edited wildcard-shadow failures.
- A browser-only mocked-IPC harness passed 13 interaction checks with dark/light screenshots,
  keyboard focus/cancellation, error recovery, and footer visibility at 614px height and 19 rows.
  Local evidence is under `e2e/out/hostnames/` (ignored build/test output).
- Changed new Rust modules pass rustfmt; `git diff --check` passes. Repo-wide rustfmt still
  reports pre-existing formatting differences across untouched modules; those were left alone.

No real config, account, DNS record, or externally managed connector was changed. Live Cloudflare
reconnection and an actual VoiceOver session were not exercised; native save/restart commands
used the synthetic connector, and UI interaction checks used Chromium browser mocks.

## New project flow (2026-10-13)

- `cargo test --workspace`: 139 passed, 2 existing ignored tests. The 11 project integration
  tests cover existing source preservation, malformed YAML, revalidation at creation, safe
  filenames, collisions, group/ungrouped persistence, rollback, concurrent registration,
  canonical duplicate detection, symlink-relative credentials, and token-config compatibility.
- `npm test`: 71 passed. `npm run check`: zero errors/warnings. `npm run build` passed.
- Actual dashboard UI checked with mocked Tauri IPC: required name, existing group selection,
  config preview, draft and loaded-config creation, selected-project redirect, invalid selection
  blocking, editor-return setup refresh, refresh retry without a second create, pending-refresh
  navigation to Quick tunnel, and stopped Quick tunnel creation without an automatic start.
  Harness: `.scratch/new-flow-ui.html`; screenshot: `e2e/out/new-flow/project-form.png` (ignored).
- Independent review's editor-refresh, session-isolation, symlink-path, and config-token issues
  were fixed and rechecked. `git diff --check` passed.

No real Cloudflare tunnel, account, DNS record, or existing user config was changed during
validation. Native file-picker/registered-editor integration and live Cloudflare connectivity
were not exercised; backend file/database tests used temporary directories.

## CLI tunnel setup (2026-10-13)

- `cargo test --workspace`: 163 passed, 2 existing ignored tests. Includes 24 fake-CLI setup
  checks for existing/missing login, misleading login exit success, timeout/cancellation,
  credential/account/remote validation, partial-success recovery, rename-safe retry intents,
  secret suppression, hostile argument-like names, supported longer secrets, and YAML linking.
- `npm test`: 71 passed. `npm run check`: zero errors/warnings. `npm run build` passed.
- Mocked-IPC checks on the actual dashboard: Set up tunnel replaces manual identity editing;
  login progress and Cancel setup are shown; completion enables Start and autostart while
  leaving the project stopped; cancellation restores the action and shows retry feedback.
  Screenshot: `e2e/out/tunnel-setup/project-setup.png` (ignored).
- Independent review verified original-name intent persistence after ambiguous creation,
  setup accessibility after draft failures, and source validation/cancellation/publication.

No real Cloudflare account/resource or user certificate was changed. Browser login, remote
creation, and account responses used fixtures; a live OAuth/create round trip was not run.
