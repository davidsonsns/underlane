# Verification log

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
