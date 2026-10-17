import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const RELEASE_TAG = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const RELEASE_HEADING = /^\[(.+)\] - \d{4}-\d{2}-\d{2}$/;
const CHANGELOG_SECTION = /^##? /m;
const LINE_BREAK = /\r?\n/;
const CHANGE_ENTRY = /^- \S/m;

function changelogEntry(version, changelog) {
  const sections = changelog
    .split(CHANGELOG_SECTION)
    .filter(
      (section) =>
        section.split(LINE_BREAK, 1)[0].match(RELEASE_HEADING)?.[1] === version
    );
  if (sections.length !== 1) {
    throw new Error(
      `CHANGELOG.md needs exactly one dated [${version}] section.`
    );
  }
  const notes = sections[0]
    .split(LINE_BREAK)
    .slice(1)
    .filter((line) => !line.startsWith("["))
    .join("\n")
    .trim();
  if (!CHANGE_ENTRY.test(notes)) {
    throw new Error(`CHANGELOG.md [${version}] needs at least one change.`);
  }
  return notes;
}

export function releaseNotes(tag, versions, changelog) {
  if (!RELEASE_TAG.test(tag)) {
    throw new Error("Release tag must be vMAJOR.MINOR.PATCH, such as v0.1.0.");
  }
  const version = tag.slice(1);
  for (const [name, actual] of Object.entries(versions)) {
    if (actual !== version) {
      throw new Error(`${name} is ${actual}; release tag requires ${version}.`);
    }
  }

  return `${changelogEntry(version, changelog)}

### Installation

Requires macOS 13.3 or later. The DMG supports Apple Silicon and Intel Macs.
Open the DMG and drag Underlane into Applications.

This build is **ad-hoc signed and not notarized**. After trying to open it,
go to **System Settings → Privacy & Security → Open Anyway**, then confirm.
See [Apple's instructions](https://support.apple.com/en-us/102445).
For the warning dialog and a missing Open Anyway button, see
[first-launch troubleshooting](https://github.com/davidsonsns/underlane/blob/main/TROUBLESHOOTING.md#macos-blocks-underlane-on-first-launch).
`;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = new URL("../", import.meta.url);
  const json = (path) => JSON.parse(readFileSync(new URL(path, root), "utf8"));
  const metadata = JSON.parse(
    execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--no-deps", "--locked"],
      { cwd: root, encoding: "utf8" }
    )
  );
  const versions = Object.fromEntries(
    metadata.packages
      .filter((pkg) => metadata.workspace_members.includes(pkg.id))
      .map((pkg) => [pkg.name, pkg.version])
  );
  versions["Tauri app"] = json(
    "apps/desktop/src-tauri/tauri.conf.json"
  ).version;
  versions["Desktop package"] = json("apps/desktop/package.json").version;
  process.stdout.write(
    releaseNotes(
      process.argv[2],
      versions,
      readFileSync(new URL("CHANGELOG.md", root), "utf8")
    )
  );
}
