import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const RELEASE_TAG = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const REPOSITORY =
  /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?\/(?!\.{1,2}$)[A-Za-z0-9_.-]{1,100}$/;
const BASE64 =
  /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

function releasePlatform(tag, repository, signature) {
  if (!REPOSITORY.test(repository)) {
    throw new Error("Repository must be a GitHub OWNER/REPO name.");
  }
  const signed = signature?.trim();
  if (!(signed && BASE64.test(signed))) {
    throw new Error(
      "Updater signature must contain the generated .sig base64 text."
    );
  }
  return {
    signature: signed,
    url: `https://github.com/${repository}/releases/download/${tag}/Underlane.app.tar.gz`,
  };
}

export function updaterManifest({
  tag,
  repository,
  notes,
  signature,
  pubDate = new Date(),
}) {
  if (!RELEASE_TAG.test(tag)) {
    throw new Error("Release tag must be vMAJOR.MINOR.PATCH, such as v0.1.0.");
  }
  if (!notes?.trim()) {
    throw new Error("Updater release notes must not be empty.");
  }
  const platform = releasePlatform(tag, repository, signature);
  return {
    notes: notes.trim(),
    platforms: {
      "darwin-aarch64": platform,
      "darwin-x86_64": platform,
    },
    pub_date: pubDate.toISOString(),
    version: tag.slice(1),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const [tag, repository, notesFile, signatureFile] = process.argv.slice(2);
    if (process.argv.length !== 6) {
      throw new Error(
        "Usage: node scripts/updater-manifest.mjs TAG OWNER/REPO NOTES_FILE SIGNATURE_FILE"
      );
    }
    process.stdout.write(
      `${JSON.stringify(
        updaterManifest({
          notes: readFileSync(notesFile, "utf8"),
          repository,
          signature: readFileSync(signatureFile, "utf8"),
          tag,
        }),
        null,
        2
      )}\n`
    );
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
