import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { argumentsFor, verifyBackup } from "../backup.mjs";
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
test("operator arguments require explicit targets and reject ambiguous options", () => {
  const valid = argumentsFor([
    "restore",
    "--database-container",
    "brioche-postgres-1",
    "--database",
    "brioche_restore",
    "--media-volume",
    "brioche_restore_media",
    "--input",
    "backups/example",
  ]);
  assert.equal(valid.options.database, "brioche_restore");
  assert.equal(valid.options.user, "brioche");
  for (const args of [
    [],
    ["restore", "--input", "path"],
    ["backup", "--output", "path", "--output", "again"],
    ["verify", "--input"],
    ["verify", "--input", "path", "--unknown", "value"],
    [
      "restore",
      "--database-container",
      "db",
      "--database",
      "postgres",
      "--media-volume",
      "media",
      "--input",
      "path",
    ],
    [
      "restore",
      "--database-container",
      "db",
      "--database",
      "bad'name",
      "--media-volume",
      "media",
      "--input",
      "path",
    ],
    [
      "backup",
      "--database-container",
      "--privileged",
      "--media-volume",
      "media",
      "--output",
      "path",
    ],
  ])
    assert.throws(() => argumentsFor(args));
});
test("backup validation checks complete dump/media bytes and rejects unsafe manifests", async () => {
  const root = await mkdtemp(join(tmpdir(), "brioche-backup-unit-"));
  try {
    await mkdir(join(root, "media"));
    const dumpBytes = Buffer.from(
      "Protocol unit fixture, not a PostgreSQL archive",
    );
    const mediaBytes = Buffer.from("Original synthetic media bytes");
    const dump = {
      name: "database.dump",
      bytes: dumpBytes.length,
      sha256: hash(dumpBytes),
    };
    const media = {
      name: `${hash(mediaBytes)}.wav`,
      bytes: mediaBytes.length,
      sha256: hash(mediaBytes),
    };
    const original = {
      format: "brioche-backup-v1",
      postgresMajor: 18,
      dump,
      media: [media],
    };
    await writeFile(join(root, "database.dump"), dumpBytes);
    await writeFile(join(root, "media", media.name), mediaBytes);
    const manifest = async (value) =>
      writeFile(join(root, "manifest.json"), JSON.stringify(value));
    await manifest(original);
    assert.deepEqual(await verifyBackup(root), original);
    for (const value of [
      { ...original, format: "unknown" },
      { ...original, postgresMajor: 17 },
      { ...original, dump: { ...dump, name: "../outside" } },
      { ...original, media: [{ ...media, name: "../outside.wav" }] },
      { ...original, media: [media, media] },
      { ...original, dump: { ...dump, bytes: dump.bytes + 1 } },
      { ...original, media: [{ ...media, bytes: 33 * 1024 ** 2 }] },
    ]) {
      await manifest(value);
      await assert.rejects(verifyBackup(root));
    }
    await manifest(original);
    await writeFile(
      join(root, "database.dump"),
      Buffer.alloc(dumpBytes.length),
    );
    await assert.rejects(verifyBackup(root), /checksum/);
    await writeFile(join(root, "database.dump"), dumpBytes);
    await writeFile(
      join(root, "media", media.name),
      Buffer.alloc(mediaBytes.length),
    );
    await assert.rejects(verifyBackup(root), /checksum/);
  } finally {
    await rm(root, { recursive: true });
  }
});
