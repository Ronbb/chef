import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomUUID, createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile, cp, open, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

test(
  "Docker backup restores large binary streams and refuses unsafe targets",
  {
    skip: process.env.BRIOCHE_BACKUP_DOCKER_TEST !== "1",
    timeout: 120000,
  },
  async () => {
    const id = `brioche-backup-test-${randomUUID()}`;
    const sourceVolume = `${id}-source`;
    const restoredVolume = `${id}-restored`;
    const rejectedVolume = `${id}-rejected`;
    const missingVolume = `${id}-missing`;
    const helper = `${id}-media`;
    const root = await mkdtemp(join(tmpdir(), "brioche-backup-docker-"));
    const image =
      "postgres:18.6-bookworm@sha256:3725f4e2499eef5134592b3b4ab79a543ed7f8e533b05b5b637af926630f6650";
    const run = (command, args, allowFailure = false, input?: string | Buffer) =>
      new Promise<{ code: number | null; stdout: string; stderr: string }>((resolveRun, reject) => {
        const child = spawn(command, args, {
          shell: false,
          stdio: [input ? "pipe" : "ignore", "pipe", "pipe"],
        });
        const buffers = [];
        const errors = [];
        child.stdout.on("data", (bytes) => buffers.push(bytes));
        child.stderr.on("data", (bytes) => errors.push(bytes));
        child.once("error", reject);
        child.once("exit", (code) => {
          const value = {
            code,
            stdout: Buffer.concat(buffers).toString().trim(),
            stderr: Buffer.concat(errors).toString(),
          };
          if (code === 0 || allowFailure) resolveRun(value);
          else reject(new Error(`${command} failed: ${value.stderr}`));
        });
        if (input) child.stdin.end(input);
      });
    const docker = (args: string[], allowFailure = false, input?: string | Buffer) =>
      run("docker", args, allowFailure, input);
    const sql = (database, statement) =>
      docker([
        "exec",
        id,
        "psql",
        "-X",
        "-At",
        "-v",
        "ON_ERROR_STOP=1",
        "-U",
        "postgres",
        "-d",
        database,
        "-c",
        statement,
      ]);
    const cli = (args, allowFailure = false) =>
      run(
        process.execPath,
        [...process.execArgv, fileURLToPath(new URL("../backup.ts", import.meta.url)), ...args],
        allowFailure,
      );
    try {
      await docker([
        "run",
        "--detach",
        "--name",
        id,
        "--network",
        "none",
        "-e",
        "POSTGRES_HOST_AUTH_METHOD=trust",
        "-e",
        "POSTGRES_DB=backup_source",
        image,
      ]);
      let ready = false;
      for (let i = 0; i < 30; i++) {
        if (
          (
            await docker(
              [
                "exec",
                id,
                "pg_isready",
                // The image's initialization server accepts Unix sockets before
                // shutting down. TCP readiness proves the final server is up.
                "-h",
                "127.0.0.1",
                "-U",
                "postgres",
                "-d",
                "backup_source",
              ],
              true,
            )
          ).code === 0
        ) {
          ready = true;
          break;
        }
        await new Promise((resolveWait) => setTimeout(resolveWait, 500));
      }
      assert.ok(ready, "isolated database became ready");
      await docker(["volume", "create", sourceVolume]);
      await docker([
        "run",
        "--detach",
        "--name",
        helper,
        "--network",
        "none",
        "--mount",
        `type=volume,source=${sourceVolume},target=/media`,
        "--entrypoint",
        "sleep",
        image,
        "infinity",
      ]);
      const bytes = Buffer.from(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" fill="#ffa62f"/></svg>',
      );
      const hash = createHash("sha256").update(bytes).digest("hex");
      await docker(
        ["exec", "-i", helper, "tee", `/media/${hash}.svg`],
        false,
        bytes,
      );
      await sql(
        "backup_source",
        `CREATE TABLE media_assets(descriptor jsonb); CREATE TABLE audio_assets(descriptor jsonb); INSERT INTO media_assets VALUES ('{"sha256":"${hash}","mimeType":"image/svg+xml"}'); CREATE TABLE payloads AS SELECT g AS id,md5(g::text) AS payload FROM generate_series(1,100000) g;`,
      );
      await sql(
        "backup_source",
        "CREATE SCHEMA restore_identity; CREATE ROLE restore_unprivileged NOINHERIT; CREATE FUNCTION public.restore_lock() RETURNS BOOLEAN LANGUAGE sql SECURITY DEFINER AS 'SELECT true'; REVOKE ALL ON FUNCTION public.restore_lock() FROM PUBLIC;",
      );
      const audition = Buffer.from("private audition binary\u0000\u00ff");
      const original = Buffer.from("private original AIGC binary\u0000\u00ff");
      const auditionHash = createHash("sha256").update(audition).digest("hex"),
        originalHash = createHash("sha256").update(original).digest("hex");
      for (const [sha, buffer] of [
        [auditionHash, audition],
        [originalHash, original],
      ])
        await docker(
          ["exec", "-i", helper, "tee", `/media/${sha}.wav`],
          false,
          buffer,
        );
      await sql(
        "backup_source",
        `CREATE TABLE voice_audition_events(status text,result jsonb); INSERT INTO voice_audition_events VALUES ('ready','{"sha256":"${auditionHash}","providerSha256":"${originalHash}"}'),('submitted',NULL),('ready','{"sha256":"${auditionHash}","providerSha256":"${originalHash}"}');`,
      );
      const clip = Buffer.from("private course clip binary\u0000\u00fe");
      const clipOriginal = Buffer.from(
        "private course original AIGC binary\u0000\u00fe",
      );
      const clipHash = createHash("sha256").update(clip).digest("hex"),
        clipOriginalHash = createHash("sha256")
          .update(clipOriginal)
          .digest("hex");
      for (const [sha, buffer] of [
        [clipHash, clip],
        [clipOriginalHash, clipOriginal],
      ])
        await docker(
          ["exec", "-i", helper, "tee", `/media/${sha}.wav`],
          false,
          buffer,
        );
      await sql(
        "backup_source",
        `CREATE TABLE course_speech_clip_events(status text,result jsonb); INSERT INTO course_speech_clip_events VALUES ('ready','{"sha256":"${clipHash}","providerSha256":"${clipOriginalHash}"}'),('submitted',NULL),('unknown',NULL),('ready','{"sha256":"${clipHash}","providerSha256":"${clipOriginalHash}"}'),('ready','{"sha256":"${auditionHash}","providerSha256":"${originalHash}"}');`,
      );
      const snapshot = join(root, "snapshot");
      await cli([
        "backup",
        "--database-container",
        id,
        "--database",
        "backup_source",
        "--user",
        "postgres",
        "--media-volume",
        sourceVolume,
        "--output",
        snapshot,
      ]);
      const manifest = JSON.parse(
        await readFile(join(snapshot, "manifest.json"), "utf8"),
      );
      assert.ok(
        manifest.dump.bytes > 1024 * 1024,
        "archive exceeds pipe buffer size",
      );
      assert.equal(manifest.media.length, 5);
      for (const sha of [
        auditionHash,
        originalHash,
        clipHash,
        clipOriginalHash,
      ])
        assert.ok(
          manifest.media.some((record) => record.name === `${sha}.wav`),
        );
      await cli([
        "restore",
        "--database-container",
        id,
        "--database",
        "backup_restored",
        "--user",
        "postgres",
        "--media-volume",
        restoredVolume,
        "--input",
        snapshot,
      ]);
      const signature =
        "SELECT count(*),md5(string_agg(payload,'' ORDER BY id)) FROM payloads";
      assert.equal(
        (await sql("backup_source", signature)).stdout,
        (await sql("backup_restored", signature)).stdout,
      );
      const clipSignature =
        "SELECT count(*),md5(string_agg(status||COALESCE(result::text,''),',' ORDER BY status,result::text)) FROM course_speech_clip_events";
      assert.equal(
        (await sql("backup_source", clipSignature)).stdout,
        (await sql("backup_restored", clipSignature)).stdout,
      );
      assert.equal(
        (
          await sql(
            "backup_source",
            "SELECT has_function_privilege('restore_unprivileged','public.restore_lock()','EXECUTE')",
          )
        ).stdout,
        "f",
      );
      assert.equal(
        (
          await sql(
            "backup_restored",
            "SELECT has_function_privilege('restore_unprivileged','public.restore_lock()','EXECUTE')",
          )
        ).stdout,
        "t",
        "no-acl restores PostgreSQL's default PUBLIC execute",
      );
      await docker(
        [
          "exec",
          "-i",
          id,
          "psql",
          "-X",
          "-U",
          "postgres",
          "-d",
          "backup_restored",
          "-v",
          "identity_schema=restore_identity",
          "-v",
          "learning_schema=public",
        ],
        false,
        await readFile(
          new URL(
            "../../infra/database/restore-boundaries.sql",
            import.meta.url,
          ),
        ),
      );
      assert.equal(
        (
          await sql(
            "backup_restored",
            "SELECT has_function_privilege('restore_unprivileged','public.restore_lock()','EXECUTE')",
          )
        ).stdout,
        "f",
      );
      assert.equal(
        (await sql("backup_restored", "SELECT public.restore_lock()")).stdout,
        "t",
        "owner maintenance remains available",
      );
      const again = await cli(
        [
          "restore",
          "--database-container",
          id,
          "--database",
          "backup_restored",
          "--user",
          "postgres",
          "--media-volume",
          rejectedVolume,
          "--input",
          snapshot,
        ],
        true,
      );
      assert.notEqual(again.code, 0);
      assert.match(again.stderr, /database already exists/);
      assert.notEqual(
        (await docker(["volume", "inspect", rejectedVolume], true)).code,
        0,
      );
      const volumeDenied = await cli(
        [
          "restore",
          "--database-container",
          id,
          "--database",
          "backup_rejected",
          "--user",
          "postgres",
          "--media-volume",
          restoredVolume,
          "--input",
          snapshot,
        ],
        true,
      );
      assert.notEqual(volumeDenied.code, 0);
      assert.match(volumeDenied.stderr, /media volume already exists/);
      const corrupt = join(root, "corrupt");
      await cp(snapshot, corrupt, { recursive: true });
      const file = await open(join(corrupt, "database.dump"), "r+");
      await file.write(Buffer.from([0]), 0, 1, 0);
      await file.close();
      const corruption = await cli(
        [
          "restore",
          "--database-container",
          id,
          "--database",
          "backup_rejected",
          "--user",
          "postgres",
          "--media-volume",
          rejectedVolume,
          "--input",
          corrupt,
        ],
        true,
      );
      assert.notEqual(corruption.code, 0);
      assert.match(corruption.stderr, /checksum/);
      assert.equal(
        (
          await sql(
            "postgres",
            "SELECT count(*) FROM pg_database WHERE datname='backup_rejected'",
          )
        ).stdout,
        "0",
      );
      assert.notEqual(
        (await docker(["volume", "inspect", rejectedVolume], true)).code,
        0,
      );
      assert.equal((await cli(["verify", "--input", snapshot])).code, 0);
      // A self-consistent manifest may still omit a private clip used by the DB.
      const missing = join(root, "missing");
      await cp(snapshot, missing, { recursive: true });
      await writeFile(
        join(missing, "manifest.json"),
        JSON.stringify({
          ...manifest,
          media: manifest.media.filter(
            (r) => r.name !== `${clipOriginalHash}.wav`,
          ),
        }),
      );
      assert.equal((await cli(["verify", "--input", missing])).code, 0);
      const omitted = await cli(
        [
          "restore",
          "--database-container",
          id,
          "--database",
          "backup_missing",
          "--user",
          "postgres",
          "--media-volume",
          missingVolume,
          "--input",
          missing,
        ],
        true,
      );
      assert.notEqual(omitted.code, 0);
      assert.match(
        omitted.stderr,
        /omits media referenced by restored database/,
      );
      // An intact checksum alone is not proof that an arbitrary file is a PG archive.
      const invalidArchive = Buffer.from("Not a PostgreSQL archive");
      await writeFile(join(corrupt, "database.dump"), invalidArchive);
      const badManifest = {
        ...manifest,
        dump: {
          name: "database.dump",
          bytes: invalidArchive.length,
          sha256: createHash("sha256").update(invalidArchive).digest("hex"),
        },
      };
      await writeFile(
        join(corrupt, "manifest.json"),
        JSON.stringify(badManifest),
      );
      const invalid = await cli(
        [
          "restore",
          "--database-container",
          id,
          "--database",
          "backup_rejected",
          "--user",
          "postgres",
          "--media-volume",
          rejectedVolume,
          "--input",
          corrupt,
        ],
        true,
      );
      assert.notEqual(invalid.code, 0);
      assert.equal(
        (
          await sql(
            "postgres",
            "SELECT count(*) FROM pg_database WHERE datname='backup_rejected'",
          )
        ).stdout,
        "0",
      );
    } finally {
      for (const container of [helper, id]) {
        await docker(["stop", "--time", "1", container], true);
        await docker(["rm", "--volumes", container], true);
      }
      for (const volume of [sourceVolume, restoredVolume, missingVolume])
        await docker(["volume", "rm", volume], true);
      assert.ok(root.startsWith(join(tmpdir(), "brioche-backup-docker-")));
      await rm(root, { recursive: true });
    }
  },
);
