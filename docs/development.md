# Development

Run commands from the repository root. Use macOS 13.3+, Xcode command-line tools,
Node.js 22.22+, pnpm 10.27.0, and Rust via [rustup](https://rustup.rs).
See the [README setup](../README.md#setup) for installation.

## Workspace

```text
apps/desktop/          Svelte frontend and Tauri shell
apps/web/              Astro landing page and screenshot fixtures
crates/underlane-core/ Shared Rust core
e2e/                   Desktop end-to-end checks
scripts/               Native build and verification helpers
```

The pnpm workspace owns `apps/*`; root desktop scripts forward to the `underlane`
package. The Cargo workspace shares its lockfile and root `target/` directory.
Frontend output is `apps/desktop/dist`; native bundles land in `target/`.

## Desktop commands

```sh
make dev               # Vite hot reload and Rust rebuilds
make build             # Frozen dependency install and release app bundle
make install           # Build, then copy to ~/Applications
make test              # Rust, Vitest, script-helper tests, and Svelte/TypeScript checks
make e2e               # Build with the e2e feature and test a real quick tunnel
pnpm run lint          # Biome formatting and lint checks through Ultracite
pnpm run fix           # Apply safe fixes and formatting
pnpm run analyze       # Fallow: unused code, duplication, and complexity
pnpm run test:scripts  # Harness and browser-mock tests without a native app
make security          # Gitleaks across all local branches and tags
```

Install `gitleaks` (`brew install gitleaks` on macOS), then run `make hooks` once per
checkout. The hooks scan staged content before commits and complete local history before
pushes, with detected values redacted from output. CI repeats the full-history scan using
a pinned, checksum-verified Gitleaks release. The only scanner exception is the exact
public WebSocket nonce from RFC 6455 in the handshake test fixture.

Keep credentials and captured databases outside Git. Local environment files, private
key/certificate files, `.cloudflared/`, and SQLite files are ignored; explicitly synthetic
`.env.example` files may be committed. Product screenshots must use synthetic data from
`apps/web/scripts/capture-app.mjs`. Keep native verification evidence in ignored `e2e/out/`.

`make install` replaces `~/Applications/Underlane.app`. Use that installed copy for
launch at login. Build output is `target/release/bundle/macos/Underlane.app`.
The release build uses ad-hoc signing; rebuilding can trigger another Keychain access prompt.
For universal DMGs, GitHub Releases, and changelog maintenance, see [Releases](releases.md).

Keep `pnpm run check` alongside linting because Biome's Svelte support is experimental.
Biome skips generated components in `apps/desktop/src/lib/components/ui`; Fallow follows
their imports. `.fallowrc.json` lists standalone runners, intentional UI exports, and
bounded complexity exceptions with reasons. Source suppressions explain analyzer limits.

## Landing page

The published site is [underlane.app](https://underlane.app). Its source is `apps/web`.

```sh
pnpm dev:web                       # http://127.0.0.1:4321
pnpm check:web                     # Astro and TypeScript diagnostics
pnpm build:web                     # Static output in apps/web/dist
pnpm --filter @underlane/web test   # Motion, slideshow, reduced-motion, and no-JS checks
pnpm --filter @underlane/web screenshots
```

The screenshot command launches the real Svelte UI with synthetic Tauri data and captures
light/dark projects, requests, logs, and the menu bar panel. It uses a temporary Vite server,
never connects to Cloudflare, and does not use app data. Browser captures omit native macOS
window controls. Install Chromium first if needed: `pnpm exec playwright install chromium`.

## Verification

- `crates/underlane-core/tests/` covers config parsing, imports, derived configs, tunnel setup,
  hostname edits, DNS, proxy capture/redaction, retention, CLI cleanup, and supervision against
  a fake connector. SQLite migration tests live alongside the database implementation.
- Frontend Vitest tests cover formatting, filtering, grouping, async query guards, navigation,
  and list state. With Vite running, `node e2e/onboarding-browser.mjs` checks onboarding with
  synthetic IPC. Browser mocks accept `E2E_BASE_URL` for a frozen preview.
- `make e2e` launches the native app with isolated data and a local echo server, exposes it
  through a real quick tunnel, and checks captured JSON in SQLite and the dashboard plus
  the native menu bar count. It needs network access, but no Cloudflare account.
  Evidence lands in `e2e/out/`.

Optional targeted checks:

```sh
# Install an official connector into disposable test data.
cargo test -p underlane --lib installer::tests::live_installs_verified_official_release -- --ignored --nocapture

# Offline contracts against a real cloudflared binary.
cargo test -p underlane-core --test cli_contract -- --ignored --nocapture

# Measure inspection proxy latency on loopback.
cargo test --release -p underlane-core --test proxy_capture -- --ignored --nocapture

# Config-change UI against a fake connector, after an e2e feature build.
node e2e/config-badge.mjs target/debug/bundle/macos/Underlane.app
```

Set `CLOUDFLARED_CONTRACT_BIN` to an absolute binary path if contract tests cannot discover
cloudflared. The [verification log](verification.md) records prior manual checks.

## Runtime controls

| Variable | Effect |
| --- | --- |
| `UNDERLANE_DATA_DIR` | Override app data and disable the single-instance lock for isolated runs |
| `UNDERLANE_LOG` | Filter app stdout logs, e.g. `debug` or `underlane_core=debug` |
| `UNDERLANE_E2E_PORT` | Enable the loopback control server in builds with `--features e2e`; production builds omit it |

Run `Underlane --login-item enable|disable|status` from the app bundle to manage
launch at login from a script. If a menu bar manager hides the icon, move Underlane
into its visible section.

See [architecture](../ARCHITECTURE.md) for runtime flow, config ownership, and data storage.
