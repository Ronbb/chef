import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { lstat, mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { pipeline } from "node:stream/promises";
import { Transform } from "node:stream";
import { pathToFileURL } from "node:url";

const MAX_DUMP = 10 * 1024 ** 3;
const MAX_MEDIA = 32 * 1024 ** 2;
const name = /^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,127}$/;
const databaseName = /^[a-zA-Z_][a-zA-Z0-9_]{0,62}$/;
const sha = /^[a-f0-9]{64}$/;
const extensions = new Map([
  ["image/svg+xml", "svg"],
  ["image/png", "png"],
  ["image/jpeg", "jpg"],
  ["image/webp", "webp"],
  ["audio/mpeg", "mp3"],
  ["audio/wav", "wav"],
]);
function requireCondition(condition, message) {
  if (!condition) throw new Error(message);
}
export function argumentsFor(args: string[]) {
  const [operation, ...rest] = args;
  requireCondition(
    ["backup", "restore", "verify"].includes(operation),
    "Expected backup, restore or verify",
  );
  const allowed =
    operation === "verify"
      ? ["input"]
      : operation === "backup"
        ? ["database-container", "database", "user", "media-volume", "output"]
        : ["database-container", "database", "user", "media-volume", "input"];
  const options: Record<string, string> = {};
  for (let i = 0; i < rest.length; i += 2) {
    const key = rest[i]?.slice(2);
    requireCondition(
      rest[i]?.startsWith("--") &&
        allowed.includes(key) &&
        !Object.hasOwn(options, key) &&
        rest[i + 1] &&
        !rest[i + 1].startsWith("--"),
      "Invalid, repeated or missing option",
    );
    options[key] = rest[i + 1];
  }
  const directory = options[operation === "backup" ? "output" : "input"];
  requireCondition(directory, "Backup directory is required");
  options.directory = resolve(directory);
  if (operation !== "verify") {
    requireCondition(
      name.test(options["database-container"] ?? "") &&
        name.test(options["media-volume"] ?? ""),
      "Explicit database container and media volume names are required",
    );
    options.user ??= "brioche";
    if (operation === "backup") options.database ??= "brioche";
    requireCondition(
      databaseName.test(options.database ?? "") &&
        databaseName.test(options.user),
      "Invalid database or user name",
    );
    if (operation === "restore")
      requireCondition(
        !["postgres", "template0", "template1"].includes(options.database),
        "Restore requires a new application database",
      );
  }
  return { operation, options };
}
function child(args, input = false) {
  const process = spawn("docker", args, {
    shell: false,
    stdio: [input ? "pipe" : "ignore", "pipe", "pipe"],
  });
  // Do not echo SQL, connection credentials, or dump contents from tool stderr.
  process.stderr.resume();
  const completed = new Promise<void>((resolveExit, reject) => {
    process.once("error", () =>
      reject(new Error("Docker command could not start")),
    );
    process.once("exit", (code) =>
      code === 0
        ? resolveExit()
        : reject(
            new Error(`Docker operation failed (exit ${code ?? "signal"})`),
          ),
    );
  });
  completed.catch(() => {});
  return { process, completed };
}
async function output(args, limit = 16 * 1024 ** 2) {
  const { process, completed } = child(args);
  const buffers = [];
  let size = 0;
  try {
    for await (const buffer of process.stdout) {
      size += buffer.length;
      requireCondition(size <= limit, "Docker output exceeds limit");
      buffers.push(buffer);
    }
    await completed;
    return Buffer.concat(buffers).toString("utf8").trim();
  } catch (error) {
    process.kill();
    throw error;
  }
}
function digestStream(limit) {
  const hash = createHash("sha256");
  let bytes = 0;
  const stream = new Transform({
    transform(buffer, _, done) {
      bytes += buffer.length;
      if (bytes > limit)
        return done(new Error("Backup object exceeds size limit"));
      hash.update(buffer);
      done(null, buffer);
    },
  });
  return { stream, result: () => ({ bytes, sha256: hash.digest("hex") }) };
}
async function capture(args, file, limit) {
  const { process, completed } = child(args);
  const digest = digestStream(limit);
  try {
    await pipeline(
      process.stdout,
      digest.stream,
      createWriteStream(file, { flags: "wx", mode: 0o600, flush: true }),
    );
    await completed;
    return digest.result();
  } catch (error) {
    process.kill();
    throw error;
  }
}
async function send(args, file, expected = null, allowEarlyClose = false) {
  const { process, completed } = child(args, true);
  process.stdout.resume();
  try {
    if (expected) {
      const digest = digestStream(expected.bytes);
      await pipeline(createReadStream(file), digest.stream, process.stdin);
      const actual = digest.result();
      requireCondition(
        actual.bytes === expected.bytes && actual.sha256 === expected.sha256,
        "Backup file changed during restore",
      );
    } else {
      await pipeline(createReadStream(file), process.stdin);
    }
    await completed;
  } catch (error) {
    // Listing a custom archive may exit after the TOC without consuming its data.
    // Accept that only when pg_restore itself succeeded; the whole file was
    // already verified, and the subsequent restore hashes all streamed bytes.
    if (
      allowEarlyClose &&
      ["EPIPE", "EOF", "ERR_STREAM_PREMATURE_CLOSE"].includes(error.code)
    ) {
      await completed;
      return;
    }
    process.kill();
    throw error;
  }
}
async function fileDigest(file, limit) {
  const info = await lstat(file);
  requireCondition(
    info.isFile() &&
      !info.isSymbolicLink() &&
      info.size > 0 &&
      info.size <= limit,
    "Invalid backup file or size",
  );
  const digest = digestStream(limit);
  // Drain hashed bytes without keeping a dump or recording in memory.
  digest.stream.resume();
  await pipeline(createReadStream(file), digest.stream);
  return digest.result();
}
function fileRecord(value, dump = false) {
  requireCondition(
    value &&
      typeof value === "object" &&
      sha.test(value.sha256 ?? "") &&
      Number.isSafeInteger(value.bytes) &&
      value.bytes > 0 &&
      value.bytes <= (dump ? MAX_DUMP : MAX_MEDIA),
    "Invalid backup object metadata",
  );
  requireCondition(
    dump
      ? value.name === "database.dump"
      : typeof value.name === "string" &&
          new RegExp(`^${value.sha256}\\.(svg|png|jpg|webp|mp3|wav)$`).test(
            value.name,
          ),
    "Invalid backup object name",
  );
}
export function validateBackupManifest(manifest) {
  requireCondition(
    manifest?.format === "brioche-backup-v1" &&
      Number.isInteger(manifest.postgresMajor) &&
      manifest.postgresMajor >= 18 &&
      Array.isArray(manifest.media) &&
      manifest.media.length <= 100000,
    "Unsupported backup manifest",
  );
  fileRecord(manifest.dump, true);
  const seen = new Set();
  for (const record of manifest.media) {
    fileRecord(record);
    requireCondition(!seen.has(record.name), "Duplicate media object");
    seen.add(record.name);
  }
  return manifest;
}
export async function verifyBackup(directory) {
  const rootInfo = await lstat(directory);
  const mediaInfo = await lstat(join(directory, "media"));
  requireCondition(
    rootInfo.isDirectory() &&
      !rootInfo.isSymbolicLink() &&
      mediaInfo.isDirectory() &&
      !mediaInfo.isSymbolicLink(),
    "Backup directories must be ordinary directories",
  );
  const file = join(directory, "manifest.json");
  const info = await lstat(file);
  requireCondition(
    info.isFile() && !info.isSymbolicLink() && info.size <= 16 * 1024 ** 2,
    "Invalid backup manifest",
  );
  const manifest = JSON.parse(await readFile(file, "utf8"));
  validateBackupManifest(manifest);
  for (const [record, subdirectory, limit] of [
    [manifest.dump, "", MAX_DUMP],
    ...manifest.media.map((record) => [record, "media", MAX_MEDIA]),
  ]) {
    const actual = await fileDigest(
      join(directory, subdirectory, record.name),
      limit,
    );
    requireCondition(
      actual.bytes === record.bytes && actual.sha256 === record.sha256,
      "Backup checksum or size mismatch",
    );
  }
  return manifest;
}
const sqlArgs = (options, database, sql) => [
  "exec",
  options["database-container"],
  "psql",
  "-X",
  "-At",
  "-v",
  "ON_ERROR_STOP=1",
  "-U",
  options.user,
  "-d",
  database,
  "-c",
  sql,
];
async function postgresMajor(options) {
  const version = await output(
    sqlArgs(options, "postgres", "SHOW server_version_num"),
  );
  requireCondition(
    /^\d{6}$/.test(version),
    "Cannot determine PostgreSQL version",
  );
  return Math.floor(Number(version) / 10000);
}
async function withVolume(options, volume, readonly, operation) {
  const image = await output([
    "inspect",
    "--type",
    "container",
    "--format",
    "{{.Image}}",
    options["database-container"],
  ]);
  requireCondition(
    /^sha256:[a-f0-9]{64}$/.test(image),
    "Cannot determine existing database image",
  );
  const helper = `brioche-backup-${randomUUID()}`;
  await output([
    "run",
    "--detach",
    "--name",
    helper,
    "--network",
    "none",
    "--mount",
    `type=volume,source=${volume},target=/backup-media${readonly ? ",readonly" : ""}`,
    "--entrypoint",
    "sleep",
    image,
    "infinity",
  ]);
  try {
    return await operation(helper);
  } finally {
    try {
      await output(["stop", "--time", "1", helper]);
      await output(["rm", "--volumes", helper]);
    } catch {
      console.error(
        "Temporary backup helper could not be removed; check Docker containers.",
      );
    }
  }
}
async function mediaInventory(options) {
  const auditions =
    (await output(
      sqlArgs(
        options,
        options.database,
        "SELECT to_regclass('voice_audition_events') IS NOT NULL",
      ),
    )) === "t";
  const auditionObjects = auditions
    ? " UNION SELECT result->>'sha256' AS sha,'audio/wav' AS mime FROM voice_audition_events WHERE status='ready' UNION SELECT result->>'providerSha256' AS sha,'audio/wav' AS mime FROM voice_audition_events WHERE status='ready'"
    : "";
  const clips =
    (await output(
      sqlArgs(
        options,
        options.database,
        "SELECT to_regclass('course_speech_clip_events') IS NOT NULL",
      ),
    )) === "t";
  // Preserve every ready attempt, including rejected or superseded private clips.
  const clipObjects = clips
    ? " UNION SELECT result->>'sha256' AS sha,'audio/wav' AS mime FROM course_speech_clip_events WHERE status='ready' UNION SELECT result->>'providerSha256' AS sha,'audio/wav' AS mime FROM course_speech_clip_events WHERE status='ready'"
    : "";
  const inventory = JSON.parse(
    await output(
      sqlArgs(
        options,
        options.database,
        `SELECT COALESCE(json_agg(objects), '[]'::json) FROM (SELECT descriptor->>'sha256' AS sha, descriptor->>'mimeType' AS mime FROM media_assets UNION SELECT descriptor->>'sha256' AS sha, descriptor->>'mimeType' AS mime FROM audio_assets${auditionObjects}${clipObjects}) objects`,
      ),
    ),
  );
  requireCondition(
    Array.isArray(inventory) && inventory.length <= 100000,
    "Media inventory exceeds limit",
  );
  return inventory;
}
export async function backup(options) {
  await output(["volume", "inspect", options["media-volume"]]);
  const major = await postgresMajor(options);
  requireCondition(major >= 18, "PostgreSQL 18 or later is required");
  await mkdir(options.directory, { mode: 0o700 }); // Existing directories are never overwritten.
  await mkdir(join(options.directory, "media"), { mode: 0o700 });
  const startedAt = new Date().toISOString();
  console.log("Creating database snapshot…");
  const dump = await capture(
    [
      "exec",
      options["database-container"],
      "pg_dump",
      "-U",
      options.user,
      "-d",
      options.database,
      "--format=custom",
    ],
    join(options.directory, "database.dump.partial"),
    MAX_DUMP,
  );
  await rename(
    join(options.directory, "database.dump.partial"),
    join(options.directory, "database.dump"),
  );
  // Registries and stored objects are append-only. Reading after pg_dump covers
  // every registration in its earlier MVCC snapshot; additional objects are harmless.
  const inventory = await mediaInventory(options);
  const media = [];
  const seen = new Set();
  await withVolume(options, options["media-volume"], true, async (helper) => {
    for (const record of inventory) {
      requireCondition(
        sha.test(record.sha ?? "") && extensions.has(record.mime),
        "Invalid registered media descriptor",
      );
      const filename = `${record.sha}.${extensions.get(record.mime)}`;
      if (seen.has(filename)) continue;
      seen.add(filename);
      const digest = await capture(
        ["exec", helper, "cat", `/backup-media/${filename}`],
        join(options.directory, "media", filename),
        MAX_MEDIA,
      );
      requireCondition(
        digest.bytes > 0 && digest.sha256 === record.sha,
        "Registered media file checksum mismatch",
      );
      media.push({ name: filename, ...digest });
    }
  });
  // This marker is only written after the dump and every media object are complete.
  const manifest = {
    format: "brioche-backup-v1",
    postgresMajor: major,
    startedAt,
    completedAt: new Date().toISOString(),
    dump: { name: "database.dump", ...dump },
    media,
  };
  await writeFile(
    join(options.directory, "manifest.json.partial"),
    JSON.stringify(manifest, null, 2) + "\n",
    { flag: "wx", mode: 0o600 },
  );
  await rename(
    join(options.directory, "manifest.json.partial"),
    join(options.directory, "manifest.json"),
  );
  await verifyBackup(options.directory);
  console.log(
    `Backup verified (${media.length} media objects). Keep this private and copy it off the live disk.`,
  );
}
export async function restore(options) {
  const manifest = await verifyBackup(options.directory); // Validate before creating anything.
  requireCondition(
    (await postgresMajor(options)) === manifest.postgresMajor,
    "Restore requires the same PostgreSQL major version",
  );
  await send(
    ["exec", "-i", options["database-container"], "pg_restore", "--list"],
    join(options.directory, "database.dump"),
    null,
    true,
  );
  const exists = await output(
    sqlArgs(
      options,
      "postgres",
      `SELECT 1 FROM pg_database WHERE datname='${options.database}'`,
    ),
  );
  requireCondition(
    exists === "",
    "Target database already exists; restore never overwrites it",
  );
  let volumeExists = true;
  try {
    await output(["volume", "inspect", options["media-volume"]]);
  } catch {
    volumeExists = false;
  }
  requireCondition(
    !volumeExists,
    "Target media volume already exists; restore never overwrites it",
  );
  const operation = randomUUID();
  await output([
    "volume",
    "create",
    "--label",
    `brioche.restore-operation=${operation}`,
    options["media-volume"],
  ]);
  const label = await output([
    "volume",
    "inspect",
    "--format",
    '{{index .Labels "brioche.restore-operation"}}',
    options["media-volume"],
  ]);
  requireCondition(
    label === operation,
    "Target media volume was concurrently created; refusing to write",
  );
  await output([
    "exec",
    options["database-container"],
    "createdb",
    "-U",
    options.user,
    "--",
    options.database,
  ]);
  console.log("Restoring into new database and media volume…");
  await send(
    [
      "exec",
      "-i",
      options["database-container"],
      "pg_restore",
      "-U",
      options.user,
      "-d",
      options.database,
      "--single-transaction",
      "--exit-on-error",
      "--no-owner",
      "--no-acl",
    ],
    join(options.directory, "database.dump"),
    manifest.dump,
  );
  // Verify domain references too: checksums alone cannot detect omitted objects.
  const inventory = await mediaInventory(options);
  const available = new Set(manifest.media.map((record) => record.name));
  for (const record of inventory) {
    requireCondition(
      sha.test(record.sha ?? "") && extensions.has(record.mime),
      "Invalid restored media descriptor",
    );
    requireCondition(
      available.has(record.sha + "." + extensions.get(record.mime)),
      "Backup omits media referenced by restored database; targets remain incomplete",
    );
  }
  await withVolume(options, options["media-volume"], false, async (helper) => {
    await output(["exec", helper, "chown", "10001:10001", "/backup-media"]);
    await output(["exec", helper, "chmod", "700", "/backup-media"]);
    for (const record of manifest.media) {
      await send(
        [
          "exec",
          "-i",
          "--user",
          "10001:10001",
          helper,
          "tee",
          `/backup-media/${record.name}`,
        ],
        join(options.directory, "media", record.name),
        record,
      );
      const hash = await output([
        "exec",
        helper,
        "sha256sum",
        `/backup-media/${record.name}`,
      ]);
      requireCondition(
        hash.split(/\s/)[0] === record.sha256,
        "Restored media checksum mismatch",
      );
    }
  });
  console.log(
    "Restore finished. Targets remain offline; reapply schema/function boundaries and dedicated runtime grants, then verify migrations, release, login and learning before switching the application.",
  );
}
async function main() {
  const { operation, options } = argumentsFor(process.argv.slice(2));
  if (operation === "backup") await backup(options);
  else if (operation === "restore") await restore(options);
  else {
    await verifyBackup(options.directory);
    console.log("Backup checksums verified.");
  }
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  main().catch((error) => {
    console.error(error.message);
    console.error(
      "No automatic cleanup of backup files or restore targets. An incomplete operation needs inspection before retrying.",
    );
    process.exitCode = 1;
  });
}
