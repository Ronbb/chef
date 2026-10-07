import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { createServer as createHttpServer } from "node:http";
import { createServer as createViteServer } from "vite";
import { fileURLToPath } from "node:url";

const reads = [];
const previousBase = process.env.INTERNAL_API_URL;
const api = createHttpServer((request, response) => {
  reads.push({ path: request.url, cookie: request.headers.cookie });
  response.setHeader("Content-Type", "application/json");
  if (request.headers.cookie !== "__Host-hargow.sid=controlled-hargow") {
    response.writeHead(401).end("{}");
    return;
  }
  response.end(
    JSON.stringify(
      request.url === "/api/v1/me"
        ? {
            id: "1",
            email: "shared@example.test",
            displayName: "Shared account",
            role: "learner",
            version: 17,
            settings: {
              timeZone: "Asia/Hong_Kong",
              weeklyDays: 5,
              dailyMinutes: 10,
              showTranslation: true,
              speechRate: 1,
            },
          }
        : { items: ["hargow-only"] },
    ),
  );
});
let vite, boundary;
before(async () => {
  await new Promise((resolve) => api.listen(0, "127.0.0.1", resolve));
  process.env.INTERNAL_API_URL = `http://127.0.0.1:${api.address().port}`;
  vite = await createViteServer({
    configFile: false,
    root: fileURLToPath(new URL("../", import.meta.url)),
    plugins: [
      {
        name: "trusted-hargow-config",
        resolveId(id) {
          if (id === "@chef/product") return "\0trusted-hargow-product";
        },
        load(id) {
          if (id === "\0trusted-hargow-product")
            return 'export default { id:"hargow",sessionNamespace:"hargow" };';
        },
      },
    ],
    server: { middlewareMode: true },
    logLevel: "error",
  });
  boundary = await vite.ssrLoadModule("/app/lib/api.server.ts");
});
after(async () => {
  await vite?.close();
  await new Promise((resolve) => api.close(resolve));
  if (previousBase === undefined) delete process.env.INTERNAL_API_URL;
  else process.env.INTERNAL_API_URL = previousBase;
});

test("actual Hargow SSR identity and private reads use only Hargow's session and product settings", async () => {
  reads.length = 0;
  const request = new Request("https://hargow.example.test/profile", {
    headers: {
      cookie:
        "__Host-brioche.sid=must-not-forward; __Host-hargow.sid=controlled-hargow; tracking=omit",
    },
  });
  const identity = await boundary.getIdentity(request);
  assert.equal(identity.user.id, "1");
  assert.equal(identity.user.version, 17);
  assert.equal(identity.user.role, "learner");
  assert.equal(identity.user.settings.timeZone, "Asia/Hong_Kong");
  assert.equal(identity.enabled, true);
  assert.deepEqual(
    await boundary.getPrivate(request, "/api/v1/me/saved-items"),
    { items: ["hargow-only"] },
  );
  assert.deepEqual(reads, [
    { path: "/api/v1/me", cookie: "__Host-hargow.sid=controlled-hargow" },
    {
      path: "/api/v1/me/saved-items",
      cookie: "__Host-hargow.sid=controlled-hargow",
    },
  ]);
});

test("another product's cookie never authorizes Hargow SSR or private reads", async () => {
  reads.length = 0;
  const request = new Request("https://hargow.example.test/profile", {
    headers: { cookie: "__Host-brioche.sid=must-not-forward" },
  });
  assert.deepEqual(await boundary.getIdentity(request), {
    user: null,
    enabled: true,
  });
  await assert.rejects(
    boundary.getPrivate(request, "/api/v1/me/saved-items"),
    (error) => error instanceof Response && error.status === 401,
  );
  assert.equal(reads.length, 2);
  assert.ok(reads.every((read) => read.cookie === undefined));
});

test("ambiguous Hargow sessions fail before any internal request", async () => {
  reads.length = 0;
  const request = new Request("https://hargow.example.test/profile", {
    headers: { cookie: "__Host-hargow.sid=a; hargow.sid=b" },
  });
  await assert.rejects(
    boundary.getIdentity(request),
    (error) => error instanceof Response && error.status === 401,
  );
  await assert.rejects(
    boundary.getPrivate(request, "/api/v1/me/saved-items"),
    (error) => error instanceof Response && error.status === 401,
  );
  assert.equal(reads.length, 0);
});
