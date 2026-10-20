# Changelog

Notable changes to Underlane, with the newest version first.

## [Unreleased]

## [0.5.0] - 2026-10-19

### Added

- Individual hostname Start/Stop controls on hover and keyboard focus, with saved selections and uninterrupted sibling hostname traffic for supported local HTTP/HTTPS configs.
- Inline hostname diagnostics distinguish DNS mismatches, stopped connectors, unavailable local services, and public HTTPS failures across the dashboard and menu bar.
- A reviewed Repair DNS flow updates only the selected tunnel CNAME, with configuration/account checks and verified readback.
- Background public reachability checks and route-specific test paths distinguish successful responses from authentication gates, cached responses, and unverified routes.

### Fixed

- Starting the first hostname in a project skips unnecessary TLS certificate loading for plain HTTP origins.
- Starting one hostname no longer displays automatic DNS status on its stopped siblings.
- Unverified DNS status stays in the hostname actions menu instead of adding an inline badge.
- Stopping the last enabled hostname shuts down its connector and clears project activity.
- Token-authenticated configs keep whole-project controls so remote routing cannot bypass an individual hostname's stopped state.
- Removed Stopped badges from hostname rows.

## [0.4.0] - 2026-10-18

### Added

- A saved background opacity control from 50% to 100%, with live preview while keeping text and controls solid.

### Changed

- Projects open their settings directly, without nested Overview, Requests, and Logs tabs.
- Requests and Logs are shared sidebar views with optional project filters.
- Project creation and bulk start, stop, and restart actions are available from the workspace sidebar, which now shows the app version.

## [0.3.0] - 2026-10-17

### Added

- A grouped workspace sidebar with project navigation and project-specific Overview, Requests, and Logs views.
- Group creation, renaming, and empty-group deletion, with drag-and-drop and menu actions to move projects between groups.

### Security

- Redact credentials in captured redirect URL fragments and URL username/password fields.
- Remove deleted captures and logs from active database files when clearing history or resetting the app.

## [0.2.0] - 2026-10-16

### Added

- An “Update available” sidebar button to download verified app updates and restart when ready, restoring which projects were running.

### Changed

- Clearer first-launch and local-network permission troubleshooting for macOS.

## [0.1.0] - 2026-10-16

### Added

- macOS menu bar controls for Cloudflare Tunnels, with automatic crash recovery and launch at login.
- A request inspector with headers, bodies, timing, JSON and hex views, and copy as curl.
- Projects and groups, existing config imports, and temporary quick tunnels.
- Guided cloudflared installation and tunnel setup, hostname editing with backups, and DNS checks.
- Searchable tunnel logs, failure explanations, and warnings when a config needs a restart.

[Unreleased]: https://github.com/davidsonsns/underlane/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/davidsonsns/underlane/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/davidsonsns/underlane/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/davidsonsns/underlane/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/davidsonsns/underlane/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/davidsonsns/underlane/releases/tag/v0.1.0
