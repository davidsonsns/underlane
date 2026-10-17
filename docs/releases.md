# Releases

Pushing a tag such as `v0.1.0` starts the [release workflow](../.github/workflows/release.yml).
It builds one universal DMG for Apple Silicon and Intel, verifies the app's signature
and architectures, and creates a **draft** GitHub Release using that version's changelog
entries. Publishing the draft is a separate manual step.

Builds use ad-hoc signing. No Apple Developer ID, certificate, account, or notarization
credentials are needed. The workflow uses GitHub's built-in token to upload the DMG.
Downloads from a private repository require repository read access.

## Maintain the changelog

Add a short, user-visible entry under `Unreleased` in [CHANGELOG.md](../CHANGELOG.md)
when behavior changes. Use `Added`, `Changed`, `Fixed`, `Removed`, or `Security`
as needed; omit empty categories. Describe the result for users instead of copying
commit messages or listing internal refactors.

For the first release, the `0.1.0` section is already prepared. Update its date if
shipping after the date shown.

## Prepare a release

1. Choose the next version: patch for fixes (`0.1.1`), minor for features (`0.2.0`).
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
   node scripts/release-notes.mjs v0.1.0
   make test
   pnpm run lint
   pnpm run analyze
   ```

5. Commit and push the release preparation. From that commit, create and push the
   version tag:

   ```sh
   git tag -a v0.1.0 -m "Underlane 0.1.0"
   git push origin v0.1.0
   ```

Replace `0.1.0` in these commands with the version being released. The workflow
rejects mismatched package versions or missing/empty dated changelog sections before
building. No release is created if validation or the build fails.

## Review and publish

Wait for [Release in GitHub Actions](https://github.com/davidsonsns/underlane/actions/workflows/release.yml)
to finish, then open the draft under [GitHub Releases](https://github.com/davidsonsns/underlane/releases).
Download its DMG and test first launch on a Mac. Check the release notes and attached
DMG, then publish the draft when ready.

The notes explain that builds are ad-hoc signed and not notarized. After the first
blocked launch, users can approve Underlane in **System Settings → Privacy & Security →
Open Anyway**. See [Apple's instructions](https://support.apple.com/en-us/102445).
Users install future releases manually by replacing the app; Underlane has no auto-updater.

For a failed build, rerun the failed job from Actions. If a draft was already created
but its upload failed, finish uploading the DMG to that draft; `gh release create`
will not overwrite an existing release. Never move a published version tag or replace
its assets; make a new patch release for corrections.

## Build a release DMG locally

From the repository root, with the normal build prerequisites installed:

```sh
pnpm install --frozen-lockfile
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm tauri build --target universal-apple-darwin --bundles app,dmg --ci -- --locked
```

The DMG lands in `target/universal-apple-darwin/release/bundle/dmg/`.
`make build` still produces a native `.app` for the local development/install loop.
