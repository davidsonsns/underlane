import assert from "node:assert/strict";
import test from "node:test";
import { releaseNotes } from "../release-notes.mjs";

const TAG_ERROR = /Release tag/;
const VERSION_ERROR = /tauri is 0.2.0; release tag requires 0.1.0/;
const CHANGELOG_ERROR = /CHANGELOG.md/;
const versions = { core: "0.1.0", desktop: "0.1.0", tauri: "0.1.0" };
const changelog = `# Changelog

## [Unreleased]

### Added
- Not shipped yet.

## [0.1.0] - 2026-10-16

### Added
- First release.

## [0.0.1] - 2026-10-01

### Fixed
- An older change.

[0.1.0]: https://example.com/v0.1.0
`;

test("release notes contain only the selected version and installation guidance", () => {
  const notes = releaseNotes(
    "v0.1.0",
    versions,
    changelog.replaceAll("\n", "\r\n")
  );
  assert.ok(notes.startsWith("### Added\n- First release."));
  assert.ok(!notes.includes("Not shipped yet"));
  assert.ok(!notes.includes("An older change"));
  assert.ok(!notes.includes("example.com"));
  assert.ok(notes.includes("ad-hoc signed and not notarized"));
  assert.ok(notes.includes("Apple Silicon and Intel"));
});

test("the final version excludes changelog link definitions", () => {
  const notes = releaseNotes(
    "v0.1.0",
    versions,
    "## [0.1.0] - 2026-10-16\n\n- First release.\n\n[0.1.0]: https://example.com"
  );
  assert.ok(notes.startsWith("- First release."));
  assert.ok(!notes.includes("example.com"));
});

test("release tags require a plain version and every package must match", () => {
  for (const tag of [undefined, "main", "0.1.0", "v01.1.0", "v0.1.0-beta.1"]) {
    assert.throws(() => releaseNotes(tag, versions, changelog), TAG_ERROR);
  }
  assert.throws(
    () => releaseNotes("v0.1.0", { ...versions, tauri: "0.2.0" }, changelog),
    VERSION_ERROR
  );
});

test("missing, undated, duplicate, and empty release sections fail before a build", () => {
  for (const contents of [
    "## [Unreleased]\n- Upcoming change.",
    "## [0.1.0]\n- Missing date.",
    `${changelog}\n## [0.1.0] - 2026-10-17\n- Duplicate.`,
    "## [0.1.0] - 2026-10-16\n\n### Added\n\n## [0.0.1] - 2026-10-01\n- Older.",
    "## [0.1.0] - 2026-10-16\n\nNot a change - just a separator.",
  ]) {
    assert.throws(
      () => releaseNotes("v0.1.0", versions, contents),
      CHANGELOG_ERROR
    );
  }
});
