import test from "node:test";
import assert from "node:assert/strict";
import {
  adminArchive,
  adminWrite,
  AdminWriteError,
} from "../app/lib/admin.client.ts";
test("canceled admin writes cannot issue a token after late CSRF bootstrap", async () => {
  const original = globalThis.fetch;
  const controller = new AbortController();
  const paths: string[] = [];
  try {
    globalThis.fetch = async (input) => {
      paths.push(String(input));
      return {
        ok: true,
        json: async () => {
          controller.abort();
          return { csrfToken: "obsolete" };
        },
      } as Response;
    };
    await assert.rejects(
      adminWrite(
        "accounts/token",
        { email: "synthetic@example.test" },
        controller.signal,
      ),
      { name: "AbortError" },
    );
    assert.deepEqual(paths, ["/api/v1/auth/csrf"]);
    await assert.rejects(adminWrite("accounts/token", {}, controller.signal), {
      name: "AbortError",
    });
    assert.equal(paths.length, 1);
  } finally {
    globalThis.fetch = original;
  }
});

test("archive downloads require a complete bounded TAR response", async () => {
  const original = globalThis.fetch;
  let response: Response;
  const signal = new AbortController().signal;
  try {
    globalThis.fetch = async (input) =>
      String(input).endsWith("/csrf")
        ? new Response(JSON.stringify({ csrfToken: "controlled" }))
        : response;
    response = new Response("not an archive");
    await assert.rejects(
      adminArchive("speech-alignments/fixture/package", {}, signal),
      /响应无效/,
    );
    response = new Response(new Uint8Array(), {
      headers: { "content-type": "application/x-tar" },
    });
    await assert.rejects(
      adminArchive("speech-alignments/fixture/package", {}, signal),
      /为空/,
    );
    const bytes = new Uint8Array([1, 2, 3, 4]);
    response = new Response(bytes, {
      headers: { "content-type": "application/x-tar" },
    });
    const archive = await adminArchive(
      "speech-alignments/fixture/package",
      {},
      signal,
    );
    assert.equal(archive.type, "application/x-tar");
    assert.deepEqual(new Uint8Array(await archive.arrayBuffer()), bytes);
    let cancelled = false;
    response = new Response(
      new ReadableStream({
        pull(controller) {
          controller.enqueue(new Uint8Array(1024 * 1024));
        },
        cancel() {
          cancelled = true;
        },
      }),
      { headers: { "content-type": "application/x-tar" } },
    );
    await assert.rejects(
      adminArchive("speech-alignments/fixture/package", {}, signal),
      /下载上限/,
    );
    assert.equal(cancelled, true);
  } finally {
    globalThis.fetch = original;
  }
});

test("archive cancellation rejects late bootstrap and incomplete bodies", async () => {
  const original = globalThis.fetch;
  try {
    const bootstrapAbort = new AbortController();
    let calls = 0;
    globalThis.fetch = async () => {
      calls++;
      bootstrapAbort.abort();
      return new Response(JSON.stringify({ csrfToken: "controlled" }));
    };
    await assert.rejects(
      adminArchive(
        "speech-alignments/fixture/package",
        {},
        bootstrapAbort.signal,
      ),
      { name: "AbortError" },
    );
    assert.equal(calls, 1);
    const bodyAbort = new AbortController();
    let cancelled = false;
    globalThis.fetch = async (input) =>
      String(input).endsWith("/csrf")
        ? new Response(JSON.stringify({ csrfToken: "controlled" }))
        : new Response(
            new ReadableStream({
              pull(controller) {
                controller.enqueue(new Uint8Array([1]));
                bodyAbort.abort();
              },
              cancel() {
                cancelled = true;
              },
            }),
            { headers: { "content-type": "application/x-tar" } },
          );
    await assert.rejects(
      adminArchive("speech-alignments/fixture/package", {}, bodyAbort.signal),
      { name: "AbortError" },
    );
    assert.equal(cancelled, true);
  } finally {
    globalThis.fetch = original;
  }
});

test("only a rejected write supplies a definite validation status", async () => {
  const original = globalThis.fetch;
  try {
    let calls = 0;
    globalThis.fetch = async () => {
      calls++;
      return new Response(null, { status: 400 });
    };
    await assert.rejects(adminWrite("speech-alignments", {}), (error) => {
      assert.ok(error instanceof Error);
      assert.equal(error instanceof AdminWriteError, false);
      return true;
    });
    assert.equal(calls, 1);
    globalThis.fetch = async (input) => {
      calls++;
      return String(input).endsWith("/csrf")
        ? new Response(JSON.stringify({ csrfToken: "controlled" }))
        : new Response(null, { status: 422 });
    };
    await assert.rejects(adminWrite("speech-alignments", {}), (error) => {
      assert.ok(error instanceof AdminWriteError);
      assert.equal(error.status, 422);
      return true;
    });
    assert.equal(calls, 3);
  } finally {
    globalThis.fetch = original;
  }
});

test("multipart admin writes preserve browser boundaries and the CSRF header", async () => {
  const original = globalThis.fetch;
  const body = new FormData();
  body.set("document", "{}");
  body.set("file", new Blob(["svg"]), "test.svg");
  try {
    let writes = 0;
    globalThis.fetch = async (input, init) => {
      if (String(input).endsWith("/csrf"))
        return new Response(JSON.stringify({ csrfToken: "controlled" }));
      writes++;
      assert.equal(init?.body, body);
      const headers = new Headers(init?.headers);
      assert.equal(headers.has("content-type"), false);
      assert.equal(headers.get("x-csrf-token"), "controlled");
      return new Response(
        JSON.stringify({ assetId: "test-image", revision: 1 }),
      );
    };
    assert.deepEqual(await adminWrite("assets", body), {
      assetId: "test-image",
      revision: 1,
    });
    assert.equal(writes, 1);
  } finally {
    globalThis.fetch = original;
  }
});
