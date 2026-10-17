# Releases

Release directly from `main`: update the versions and changelog, run checks, commit,
and push a version tag. No release PR or manual draft-publishing step is needed.

Pushing a tag such as `v0.4.0` starts the [release workflow](../.github/workflows/release.yml).
It builds a universal DMG and signed updater archive for Apple Silicon and Intel,
verifies the app's signature and architectures, and publishes the latest GitHub Release
using that version's changelog entries. The version tag identifies the released commit.

Builds use ad-hoc signing. No Apple Developer ID, certificate, account, or notarization
credentials are needed. Updater signing uses a separate Tauri key, described below.
The workflow uses GitHub's built-in token to upload the release assets. Underlane's
updater needs public, unauthenticated access to the published manifest and archive;
a private repository's assets cannot serve this updater.

## Set up updater signing once

1. Generate a Tauri updater key pair on a trusted machine, outside the repository:

   ```sh
   mkdir -p "$HOME/.tauri"
   pnpm tauri signer generate -w "$HOME/.tauri/underlane.key"
   ```

   Keep the private key and its password in secure backups. Losing the key prevents
   existing installations from trusting new updates; never commit it or share it.
2. Set `plugins.updater.pubkey` in `apps/desktop/src-tauri/tauri.conf.json` to the
   generated public key file's contents. Set its endpoint to
   `https://github.com/davidsonsns/underlane/releases/latest/download/latest.json`.
   The public key is safe to commit and must match the private signing key.
3. In the repository's **Settings → Secrets and variables → Actions**, create
   `TAURI_SIGNING_PRIVATE_KEY` with the private key file's contents. For an encrypted
   key, also create `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` with its password; leave
   that secret unset for an unencrypted key.
   The workflow fails before the build if the private key secret is absent.

Tauri updater signatures verify update archives independently of Apple's ad-hoc
app signing. Keep the same updater key for subsequent releases.

## Maintain the changelog

Add a short, user-visible entry under `Unreleased` in [CHANGELOG.md](../CHANGELOG.md)
when behavior changes. Use `Added`, `Changed`, `Fixed`, `Removed`, or `Security`
as needed; omit empty categories. Describe the result for users instead of copying
commit messages or listing internal refactors.

## Release

1. Choose the next version: patch for fixes (`0.3.1`), minor for features (`0.4.0`).
   Use tags in the form `vMAJOR.MINOR.PATCH`; prerelease suffixes are not supported
   by this workflow.
2. Set the same version in `Cargo.toml` (`workspace.package.version`),
   `apps/desktop/package.json`, and `apps/desktop/src-tauri/tauri.conf.json`.
   Run `cargo check --workspace` to update the workspace packages in `Cargo.lock`.
   The independent landing-page package does not need a matching version.
3. Move `Unreleased` entries into `## [VERSION] - YYYY-MM-DD`, using the intended
   publication date. Leave a fresh `Unreleased` heading and update the comparison
   links at the bottom of the changelog.
4. Check the exact release notes, then run the normal checks:

   ```sh
   node scripts/release-notes.mjs v0.4.0
   make test
   pnpm run lint
   pnpm run analyze
   ```

5. Commit the version and changelog changes directly on `main`, push the commit,
   then create and push the version tag:

   ```sh
   git push origin main
   git tag -a v0.4.0 -m "Underlane 0.4.0"
   git push origin v0.4.0
   ```

Replace `0.4.0` in these commands with the version being released. The workflow
rejects mismatched package versions or missing/empty dated changelog sections before
building. No release is created if validation or the build fails.

## Verify the published release

Wait for [Release in GitHub Actions](https://github.com/davidsonsns/underlane/actions/workflows/release.yml)
to finish. The release is published automatically under
[GitHub Releases](https://github.com/davidsonsns/underlane/releases).
Check its release notes and four assets:

- One universal `.dmg` for manual installation.
- `Underlane.app.tar.gz`, the universal updater archive.
- `Underlane.app.tar.gz.sig`, the archive's Tauri signature.
- `latest.json`, with the release version, notes, publication date, and matching
  archive URL/signature for both `darwin-aarch64` and `darwin-x86_64`.

Download the DMG and test first launch on a Mac. Test checking for and installing
the update inside an older Underlane build that contains the same updater public key.
The updater endpoint follows the latest release automatically. The manifest date is
the workflow generation time in ISO 8601 UTC.

The notes explain that builds are ad-hoc signed and not notarized. After the first
blocked launch, users can approve Underlane in **System Settings → Privacy & Security →
Open Anyway**. The [first-launch troubleshooting guide](../TROUBLESHOOTING.md#macos-blocks-underlane-on-first-launch)
walks through the warning and approval steps.
Users can install future releases inside Underlane or replace the app from the DMG.

For a failed build, rerun the failed job from Actions. If publication failed, check
GitHub Releases before rerunning; `gh release create` will not overwrite an existing
release. Never move a published version tag or replace its assets; make a new patch
release for corrections.

## Build a release DMG locally

From the repository root, with the normal build prerequisites installed:

```sh
pnpm install --frozen-lockfile
rustup target add aarch64-apple-darwin x86_64-apple-darwin
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/underlane.key")"
# Set TAURI_SIGNING_PRIVATE_KEY_PASSWORD securely if the key is encrypted.
pnpm tauri build --target universal-apple-darwin --bundles app,dmg --config '{"bundle":{"createUpdaterArtifacts":true}}' --ci -- --locked
```

The DMG lands in `target/universal-apple-darwin/release/bundle/dmg/`; the updater
archive and signature land in `target/universal-apple-darwin/release/bundle/macos/`.
`make build` produces a native `.app` for the local development/install loop without
generating updater artifacts or requiring a signing key.
