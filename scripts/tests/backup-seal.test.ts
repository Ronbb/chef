import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  readFile,
  writeFile,
  rm,
  access,
  copyFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  argumentsFor,
  generateKey,
  sealBackup,
  openBackup,
} from "../backup-seal.ts";
import { verifyBackup } from "../backup.ts";

async function fixture(run) {
  const root = await mkdtemp(join(tmpdir(), "brioche-seal-test-"));
  try {
    const input = join(root, "source"),
      keyFile = join(root, "private.key");
    await mkdir(input);
    await mkdir(join(input, "media"));
    const dump = Buffer.from(
        "Synthetic SQL fixture containing account-private data",
      ),
      audio = Buffer.from("Synthetic private voice bytes");
    const record = (bytes, name) => ({
      name,
      bytes: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    });
    const media = record(
      audio,
      createHash("sha256").update(audio).digest("hex") + ".wav",
    );
    await writeFile(join(input, "database.dump"), dump);
    await writeFile(join(input, "media", media.name), audio);
    await writeFile(
      join(input, "manifest.json"),
      JSON.stringify({
        format: "brioche-backup-v1",
        postgresMajor: 18,
        dump: record(dump, "database.dump"),
        media: [media],
      }),
    );
    await generateKey(keyFile);
    await run({ root, input, keyFile, output: join(root, "sealed"), dump });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}
test("sealed backup verifies and restores original bytes with randomized ciphertext", () =>
  fixture(async (options) => {
    await sealBackup(options);
    assert.deepEqual(
      await openBackup({
        ...options,
        input: options.output,
        output: undefined,
      }),
      { mediaObjects: 1 },
    );
    const clear = join(options.root, "restored");
    await openBackup({ ...options, input: options.output, output: clear });
    await verifyBackup(clear);
    assert.deepEqual(
      await readFile(join(clear, "database.dump")),
      options.dump,
    );
    const ciphertext = await readFile(join(options.output, "0.enc"));
    assert.equal(ciphertext.includes(options.dump), false);
    const another = join(options.root, "another");
    await sealBackup({ ...options, output: another });
    assert.notDeepEqual(await readFile(join(another, "0.enc")), ciphertext);
  }));
test("wrong key and modified bundle identity fail before publishing plaintext", () =>
  fixture(async (options) => {
    await sealBackup(options);
    const wrongKey = join(options.root, "wrong.key");
    await generateKey(wrongKey);
    const clear = join(options.root, "wrong-output");
    await assert.rejects(
      openBackup({ input: options.output, output: clear, keyFile: wrongKey }),
    );
    await assert.rejects(access(clear));
    const metadata = JSON.parse(
      await readFile(join(options.output, "envelope.json"), "utf8"),
    );
    metadata.id = "00000000-0000-0000-0000-000000000000";
    await writeFile(
      join(options.output, "envelope.json"),
      JSON.stringify(metadata),
    );
    await assert.rejects(
      openBackup({ ...options, input: options.output, output: clear }),
    );
    await assert.rejects(access(clear));
  }));
test("ciphertext corruption fails authentication and leaves no complete restore manifest", () =>
  fixture(async (options) => {
    await sealBackup(options);
    const file = join(options.output, "1.enc"),
      bytes = await readFile(file);
    bytes[22] ^= 1;
    await writeFile(file, bytes);
    await assert.rejects(
      openBackup({ ...options, input: options.output, output: undefined }),
    );
    const clear = join(options.root, "partial");
    await assert.rejects(
      openBackup({ ...options, input: options.output, output: clear }),
    );
    await assert.rejects(access(join(clear, "manifest.json")));
    await assert.rejects(verifyBackup(clear));
  }));
test("truncation, missing objects and cross-backup replacement are rejected", () =>
  fixture(async (options) => {
    await sealBackup(options);
    const other = join(options.root, "other");
    await sealBackup({ ...options, output: other });
    const target = join(options.output, "0.enc"),
      original = await readFile(target);
    await copyFile(join(other, "0.enc"), target);
    await assert.rejects(
      openBackup({ ...options, input: options.output, output: undefined }),
    );
    await writeFile(target, original.subarray(0, original.length - 1));
    await assert.rejects(
      openBackup({ ...options, input: options.output, output: undefined }),
    );
    await rm(target);
    await assert.rejects(
      openBackup({ ...options, input: options.output, output: undefined }),
    );
  }));
test("never overwrites existing outputs, keys or nested source paths", () =>
  fixture(async (options) => {
    const originalKey = await readFile(options.keyFile);
    await assert.rejects(generateKey(options.keyFile));
    assert.deepEqual(await readFile(options.keyFile), originalKey);
    await assert.rejects(
      sealBackup({ ...options, output: join(options.input, "nested") }),
    );
    await sealBackup(options);
    await assert.rejects(sealBackup(options));
    await assert.rejects(
      openBackup({ ...options, input: options.output, output: options.input }),
    );
    assert.deepEqual(
      await readFile(join(options.input, "database.dump")),
      options.dump,
    );
  }));
test("invalid source checksum and incomplete encryption never become a usable backup", () =>
  fixture(async (options) => {
    await writeFile(join(options.input, "database.dump"), "corrupt");
    await assert.rejects(sealBackup(options));
    await assert.rejects(access(options.output));
    await mkdir(options.output);
    await assert.rejects(
      openBackup({ ...options, input: options.output, output: undefined }),
    );
  }));
test("CLI rejects keys in arguments, unknown flags, duplicates and missing paths", () => {
  assert.equal(
    argumentsFor(["verify", "--input", "backup", "--key-file", "private.key"])
      .operation,
    "verify",
  );
  for (const args of [
    [],
    ["keygen"],
    ["seal", "--key", "secret"],
    ["verify", "--key-file", "a", "--input", "x", "--input", "y"],
    ["verify", "--key-file", "a", "--input", "x", "--output", "y"],
  ])
    assert.throws(() => argumentsFor(args));
});
