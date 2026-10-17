import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { updaterManifest } from "../updater-manifest.mjs";

const TAG_ERROR = /Release tag/;
const REPOSITORY_ERROR = /Repository/;
const SIGNATURE_ERROR = /signature/;
const NOTES_ERROR = /release notes/;
const DATE_ERROR = /Invalid time value/;
const input = {
  notes: "### Fixed\n- A release fix.\n",
  pubDate: new Date("2026-10-16T12:34:56Z"),
  repository: "davidsonsns/underlane",
  signature: `${Buffer.from("generated signature").toString("base64")}\n`,
  tag: "v0.1.0",
};

test("both Mac architectures receive the universal release and its signature", () => {
  const manifest = updaterManifest(input);
  assert.equal(manifest.version, "0.1.0");
  assert.equal(manifest.pub_date, "2026-10-16T12:34:56.000Z");
  assert.equal(manifest.notes, input.notes.trim());
  assert.deepEqual(Object.keys(manifest.platforms), [
    "darwin-aarch64",
    "darwin-x86_64",
  ]);
  for (const platform of Object.values(manifest.platforms)) {
    assert.equal(
      platform.url,
      "https://github.com/davidsonsns/underlane/releases/download/v0.1.0/Underlane.app.tar.gz"
    );
    assert.equal(platform.signature, input.signature.trim());
  }
});

test("updater versions require a stable release tag", () => {
  for (const tag of [
    undefined,
    "main",
    "0.1.0",
    "v01.1.0",
    "v0.1.0-beta.1",
    "v0.1.0/asset",
  ]) {
    assert.throws(() => updaterManifest({ ...input, tag }), TAG_ERROR);
  }
});

test("repository input cannot change the release URL path or host", () => {
  for (const repository of [
    undefined,
    "underlane",
    "owner/..",
    "owner/.",
    "owner/repo/asset",
    "owner/repo?query",
    "owner/repo#fragment",
    "https://example.com/repo",
    "owner/repo%2Fasset",
  ]) {
    assert.throws(
      () => updaterManifest({ ...input, repository }),
      REPOSITORY_ERROR
    );
  }
});

test("missing or malformed signatures, notes, and publication dates fail", () => {
  for (const signature of [
    undefined,
    "",
    " \n",
    "not a generated signature",
    "abc",
    "YWJj=",
    "YW\nJj",
  ]) {
    assert.throws(
      () => updaterManifest({ ...input, signature }),
      SIGNATURE_ERROR
    );
  }
  for (const notes of [undefined, "", " \n"]) {
    assert.throws(() => updaterManifest({ ...input, notes }), NOTES_ERROR);
  }
  assert.throws(
    () => updaterManifest({ ...input, pubDate: new Date("invalid") }),
    DATE_ERROR
  );
});

test("the CLI reads the actual signature and emits JSON only", (context) => {
  const directory = mkdtempSync(join(tmpdir(), "underlane-updater-"));
  context.after(() => rmSync(directory, { force: true, recursive: true }));
  const notesFile = join(directory, "notes.md");
  const signatureFile = join(directory, "Underlane.app.tar.gz.sig");
  writeFileSync(notesFile, input.notes);
  writeFileSync(signatureFile, input.signature);
  const script = fileURLToPath(
    new URL("../updater-manifest.mjs", import.meta.url)
  );
  const args = [script, input.tag, input.repository, notesFile, signatureFile];
  const result = spawnSync(process.execPath, args, { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stderr, "");
  const manifest = JSON.parse(result.stdout);
  assert.equal(
    manifest.platforms["darwin-aarch64"].signature,
    input.signature.trim()
  );
  assert.ok(Number.isFinite(Date.parse(manifest.pub_date)));

  rmSync(signatureFile);
  const missing = spawnSync(process.execPath, args, { encoding: "utf8" });
  assert.equal(missing.status, 1);
  assert.equal(missing.stdout, "");
  assert.ok(missing.stderr.includes("ENOENT"));
});
