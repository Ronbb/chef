import {
  createCipheriv,
  createDecipheriv,
  createHash,
  randomBytes,
  randomUUID,
} from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import {
  lstat,
  mkdir,
  open,
  readFile,
  rename,
  writeFile,
} from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";
import { pipeline } from "node:stream/promises";
import { Transform, Writable } from "node:stream";
import { pathToFileURL } from "node:url";
import { validateBackupManifest, verifyBackup } from "./backup.mjs";

const format = "brioche-sealed-backup-v1";
const magic = Buffer.from("BRISEAL1");
const manifestLimit = 16 * 1024 ** 2;
function check(value, message) {
  if (!value) throw new Error(message);
}
function inside(parent, child) {
  const path = relative(parent, child);
  return (
    !path ||
    (!isAbsolute(path) &&
      path !== ".." &&
      !path.startsWith("..\\") &&
      !path.startsWith("../"))
  );
}
async function ordinary(path, maximum, directory = false) {
  const info = await lstat(path);
  check(
    !info.isSymbolicLink() &&
      (directory
        ? info.isDirectory()
        : info.isFile() && info.size > 0 && info.size <= maximum),
    "Invalid backup path or size",
  );
  return info;
}
export async function generateKey(path) {
  await writeFile(path, randomBytes(32), { flag: "wx", mode: 0o600 });
}
async function keyAt(path) {
  await ordinary(path, 32);
  const key = await readFile(path);
  check(key.length === 32, "Expected a 32-byte binary key file");
  return key;
}
function records(manifest) {
  return [
    { ...manifest.dump, source: "database.dump" },
    ...manifest.media.map((record) => ({
      ...record,
      source: join("media", record.name),
    })),
  ];
}
function checker(record) {
  const hash = createHash("sha256");
  let bytes = 0;
  return new Transform({
    transform(chunk, _, done) {
      bytes += chunk.length;
      if (bytes > record.bytes)
        return done(new Error("Backup object exceeds expected size"));
      hash.update(chunk);
      done(null, chunk);
    },
    flush(done) {
      done(
        bytes === record.bytes && hash.digest("hex") === record.sha256
          ? null
          : new Error("Backup object checksum or size mismatch"),
      );
    },
  });
}
const aad = (id, slot) => Buffer.from(`${format}\n${id}\n${slot}`);
async function encryptFile(source, target, key, id, slot, record) {
  await ordinary(source, record.bytes);
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, nonce, {
    authTagLength: 16,
  });
  cipher.setAAD(aad(id, slot));
  await writeFile(target, Buffer.concat([magic, nonce]), {
    flag: "wx",
    mode: 0o600,
  });
  await pipeline(
    createReadStream(source),
    checker(record),
    cipher,
    createWriteStream(target, { flags: "a" }),
  );
  const file = await open(target, "a");
  try {
    await file.write(cipher.getAuthTag());
  } finally {
    await file.close();
  }
}
async function decryptFile(source, key, id, slot, maximum, sink, record) {
  const info = await ordinary(source, maximum + 36);
  check(info.size > 36, "Truncated encrypted object");
  const file = await open(source, "r");
  const header = Buffer.alloc(20),
    tag = Buffer.alloc(16);
  try {
    check(
      (await file.read(header, 0, 20, 0)).bytesRead === 20 &&
        header.subarray(0, 8).equals(magic),
      "Unsupported encrypted object",
    );
    check(
      (await file.read(tag, 0, 16, info.size - 16)).bytesRead === 16,
      "Truncated authentication tag",
    );
  } finally {
    await file.close();
  }
  const cipher = createDecipheriv("aes-256-gcm", key, header.subarray(8), {
    authTagLength: 16,
  });
  cipher.setAAD(aad(id, slot));
  cipher.setAuthTag(tag);
  await pipeline(
    createReadStream(source, { start: 20, end: info.size - 17 }),
    cipher,
    ...(record ? [checker(record)] : []),
    sink,
  );
}
async function envelope(input, key) {
  await ordinary(input, 0, true);
  await ordinary(join(input, "envelope.json"), 1024);
  const meta = JSON.parse(await readFile(join(input, "envelope.json"), "utf8"));
  check(
    meta.format === format &&
      /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/.test(meta.id ?? ""),
    "Unsupported encrypted backup",
  );
  const chunks = [];
  await decryptFile(
    join(input, "manifest.enc"),
    key,
    meta.id,
    "manifest",
    manifestLimit,
    new Writable({
      write(chunk, _, done) {
        chunks.push(chunk);
        done();
      },
    }),
  );
  return {
    id: meta.id,
    manifest: validateBackupManifest(
      JSON.parse(Buffer.concat(chunks).toString("utf8")),
    ),
  };
}
export async function sealBackup({ input, output, keyFile }) {
  input = resolve(input);
  output = resolve(output);
  keyFile = resolve(keyFile);
  check(
    !inside(input, output) &&
      !inside(output, input) &&
      !inside(input, keyFile) &&
      !inside(output, keyFile),
    "Backup, output and key must be separate paths",
  );
  const key = await keyAt(keyFile);
  try {
    const manifest = await verifyBackup(input);
    await mkdir(output, { mode: 0o700 }); // Refuse existing destinations; never overwrite.
    const id = randomUUID();
    for (const [index, record] of records(manifest).entries()) {
      await encryptFile(
        join(input, record.source),
        join(output, `${index}.enc`),
        key,
        id,
        String(index),
        record,
      );
    }
    const bytes = await readFile(join(input, "manifest.json"));
    // Re-read validation prevents publishing a manifest changed during encryption.
    check(
      JSON.stringify(JSON.parse(bytes)) === JSON.stringify(manifest),
      "Source manifest changed during encryption",
    );
    await encryptFile(
      join(input, "manifest.json"),
      join(output, "manifest.enc"),
      key,
      id,
      "manifest",
      {
        bytes: bytes.length,
        sha256: createHash("sha256").update(bytes).digest("hex"),
      },
    );
    await writeFile(
      join(output, "envelope.json"),
      JSON.stringify({ format, id }) + "\n",
      { flag: "wx", mode: 0o600 },
    );
  } finally {
    key.fill(0);
  }
}
export async function openBackup({ input, output, keyFile }) {
  input = resolve(input);
  keyFile = resolve(keyFile);
  if (output) {
    output = resolve(output);
    check(
      !inside(input, output) &&
        !inside(output, input) &&
        !inside(output, keyFile),
      "Backup, output and key must be separate paths",
    );
  }
  check(!inside(input, keyFile), "Keep the key outside the encrypted backup");
  const key = await keyAt(keyFile);
  try {
    const { id, manifest } = await envelope(input, key);
    if (output) {
      await mkdir(output, { mode: 0o700 });
      await mkdir(join(output, "media"), { mode: 0o700 });
    }
    for (const [index, record] of records(manifest).entries()) {
      const sink = output
        ? createWriteStream(join(output, record.source), {
            flags: "wx",
            mode: 0o600,
          })
        : new Writable({
            write(_, __, done) {
              done();
            },
          });
      await decryptFile(
        join(input, `${index}.enc`),
        key,
        id,
        String(index),
        record.bytes,
        sink,
        record,
      );
    }
    if (output) {
      // No complete manifest until every authentication tag and original digest passes.
      await writeFile(
        join(output, "manifest.json.partial"),
        JSON.stringify(manifest) + "\n",
        { flag: "wx", mode: 0o600 },
      );
      await rename(
        join(output, "manifest.json.partial"),
        join(output, "manifest.json"),
      );
      await verifyBackup(output);
    }
    return { mediaObjects: manifest.media.length };
  } finally {
    key.fill(0);
  }
}
export function argumentsFor(args) {
  const [operation, ...rest] = args;
  check(
    ["keygen", "seal", "verify", "open"].includes(operation),
    "Expected keygen, seal, verify or open",
  );
  const allowed =
    operation === "keygen"
      ? ["key-file"]
      : operation === "verify"
        ? ["key-file", "input"]
        : ["key-file", "input", "output"];
  const options = {};
  for (let i = 0; i < rest.length; i += 2) {
    const key = rest[i]?.slice(2);
    check(
      rest[i]?.startsWith("--") &&
        allowed.includes(key) &&
        !Object.hasOwn(options, key) &&
        rest[i + 1] &&
        !rest[i + 1].startsWith("--"),
      "Invalid, repeated or missing option",
    );
    options[key] = rest[i + 1];
  }
  check(
    allowed.every((key) => options[key]),
    "Missing required option",
  );
  return {
    operation,
    options: {
      input: options.input,
      output: options.output,
      keyFile: options["key-file"],
    },
  };
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  try {
    const { operation, options } = argumentsFor(process.argv.slice(2));
    if (operation === "keygen") await generateKey(options.keyFile);
    else if (operation === "seal") await sealBackup(options);
    else await openBackup(options);
    console.log("Backup encryption operation completed.");
  } catch {
    console.error(
      "Backup encryption operation failed. Check private paths, key, completeness and integrity. Existing destinations are never overwritten. Inspect incomplete outputs before retrying; unsealed partial files may contain sensitive data.",
    );
    process.exitCode = 1;
  }
}
