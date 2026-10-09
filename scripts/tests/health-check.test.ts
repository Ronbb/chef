import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { once } from "node:events";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  argumentsFor,
  assessContainers,
  checkHealth,
} from "../health-check.ts";

const options = () => argumentsFor(["--project", "brioche-test"]);
const rows = () =>
  ["postgres", "migrate", "server", "web", "traefik"].map((service) => ({
    project: "brioche-test",
    service,
    oneoff: "False",
    status: service === "migrate" ? "exited" : "running",
    health: service === "migrate" ? null : "healthy",
    exitCode: 0,
    oomKilled: false,
    restarts: 0,
  }));
const run = async (args) =>
  args[0] === "ps"
    ? "a".repeat(64)
    : rows()
        .map((row) => JSON.stringify(row))
        .join("\n");
const fetcher = async (url) =>
  url.pathname === "/"
    ? new Response("<!doctype html><html>synthetic test</html>", {
        headers: { "content-type": "text/html" },
      })
    : Response.json({ status: url.pathname === "/api/ready" ? "ready" : "ok" });

test("health options fix project scope and reject credentials, paths and ambiguous limits", () => {
  assert.equal(options().origin, "http://127.0.0.1:30075");
  assert.equal(
    argumentsFor(["--project", "brioche", "--origin", "https://example.test"])
      .origin,
    "https://example.test",
  );
  for (const args of [
    [],
    ["--project", "UPPER"],
    ["--project", "brioche", "--project", "other"],
    ["--project", "brioche", "--timeout-ms", "0"],
    ["--project", "brioche", "--timeout-ms", "Infinity"],
    ["--project", "brioche", "--minimum-free-gib", "2"],
    ["--project", "brioche", "--layout", "unknown"],
    ["--project", "brioche", "--database-project", "shared"],
    [
      "--project",
      "brioche",
      "--layout",
      "product",
      "--database-project",
      "bad/name",
    ],
    ...[
      "http://user:secret@example.test",
      "http://example.test/api",
      "http://example.test/?token=secret",
      "http://example.test/#secret",
      "file:///tmp",
    ].map((origin) => ["--project", "brioche", "--origin", origin]),
  ])
    assert.throws(() => argumentsFor(args));
});

test("split product inventory checks identity and learning, preserving legacy layout", () => {
  const productRows = ["identity", "learning", "web", "router"].map(
    (service) => ({ ...rows()[2], service }),
  );
  assert(
    assessContainers(productRows, "brioche-test", "product").every(
      (item) => item.ok,
    ),
  );
  assert(
    assessContainers(productRows, "brioche-test").some((item) => !item.ok),
  );
  for (const change of [
    (r) => {
      r[0].health = "unhealthy";
    },
    (r) => {
      r[1].oomKilled = true;
    },
    (r) => {
      r[1].project = "other-product";
    },
    (r) => {
      r.push({ ...r[3] });
    },
    (r) => {
      r.pop();
    },
  ]) {
    const changed = structuredClone(productRows);
    change(changed);
    assert(
      assessContainers(changed, "brioche-test", "product").some(
        (item) => !item.ok,
      ),
    );
  }
});

test("shared database is explicitly scoped and cannot be omitted from a requested check", async () => {
  const config = argumentsFor([
    "--project",
    "chef-hargow",
    "--layout",
    "product",
    "--database-project",
    "shared-db",
  ]);
  let databaseHealth = "healthy";
  const calls = [];
  const splitRun = async (args) => {
    calls.push(args);
    if (args[0] === "ps")
      return (
        args.includes("label=com.docker.compose.project=shared-db") ? "b" : "a"
      ).repeat(64);
    const database = args.includes("b".repeat(64));
    const found = (
      database ? ["postgres"] : ["identity", "learning", "web", "router"]
    ).map((service) => ({
      ...rows()[2],
      project: database ? "shared-db" : "chef-hargow",
      service,
      health: database ? databaseHealth : "healthy",
    }));
    return found.map((row) => JSON.stringify(row)).join("\n");
  };
  const healthy = await checkHealth(config, { run: splitRun, fetcher });
  assert.equal(healthy.status, "healthy");
  assert.equal(healthy.containers.length, 4);
  assert.equal(healthy.database.length, 1);
  assert(
    calls.some(
      (args) =>
        args.includes("label=com.docker.compose.project=shared-db") &&
        args.includes("label=com.docker.compose.service=postgres"),
    ),
  );
  databaseHealth = "unhealthy";
  const failed = await checkHealth(config, { run: splitRun, fetcher });
  assert.equal(failed.status, "failed");
  assert.equal(failed.database[0].ok, false);
  const missing = await checkHealth(config, {
    run: async (args) =>
      args.includes("label=com.docker.compose.project=shared-db")
        ? ""
        : splitRun(args),
    fetcher,
  });
  assert.equal(missing.status, "failed");
  assert.equal(missing.database[0].reason, "missing-service");
});

test("invalid CLI options exit 2 without reporting supplied credentials", () => {
  const result = spawnSync(
    process.execPath,
    [
      ...process.execArgv,
      fileURLToPath(new URL("../health-check.ts", import.meta.url)),
      "--project",
      "brioche",
      "--origin",
      "http://user:private-test-value@example.test",
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 2);
  assert.equal(JSON.parse(result.stdout).status, "failed");
  assert(!result.stdout.includes("private-test-value"));
  assert.equal(result.stderr, "");
});

test("inventory rejects missing, duplicate, unhealthy, OOM, failed migration and wrong project", () => {
  assert(assessContainers(rows(), "brioche-test").every((item) => item.ok));
  for (const change of [
    (r) => r.pop(),
    (r) => r.push({ ...r[0] }),
    (r) => (r[2].health = "unhealthy"),
    (r) => (r[2].oomKilled = true),
    (r) => (r[1].exitCode = 1),
    (r) => (r[1].status = "running"),
    (r) => (r[0].project = "other"),
    (r) => (r[0].restarts = -1),
  ]) {
    const r = rows();
    change(r);
    assert(assessContainers(r, "brioche-test").some((item) => !item.ok));
  }
  const r = rows();
  r.push({ ...r[0], oneoff: "True" });
  assert(assessContainers(r, "brioche-test").every((item) => item.ok));
});

test("combined report checks all endpoints and disk and excludes private response content", async () => {
  const result = await checkHealth(
    { ...options(), diskPath: "synthetic", minimumFreeGiB: 5 },
    {
      run,
      fetcher,
      filesystem: async () => ({ bavail: 6n, bsize: 1024n ** 3n }),
    },
  );
  assert.equal(result.status, "healthy");
  assert.equal(result.http.length, 4);
  assert.equal(result.disk.freeBytes, String(6n * 1024n ** 3n));
  const low = await checkHealth(
    { ...options(), diskPath: "synthetic", minimumFreeGiB: 5 },
    {
      run,
      fetcher,
      filesystem: async () => ({ bavail: 1n, bsize: 1024n ** 3n }),
    },
  );
  assert.equal(low.status, "failed");
  assert.equal(low.disk.reason, "low-free-space");
  const privateText = "PRIVATE_COOKIE_AND_ENV_DO_NOT_OUTPUT";
  const bad = await checkHealth(options(), {
    run: async () => {
      throw Error(privateText);
    },
    fetcher: async () => new Response(privateText, { status: 503 }),
  });
  assert.equal(bad.status, "failed");
  assert(!JSON.stringify(bad).includes(privateText));
});

test("real HTTP redirects, invalid JSON, wrong readiness and oversized streams fail", async () => {
  let mode = "redirect";
  const server = createServer((request, response) => {
    if (mode === "redirect") {
      response.writeHead(302, { location: "/private" });
      response.end();
    } else if (mode === "large") {
      response.writeHead(200, { "content-type": "text/html" });
      response.end("x".repeat(600 * 1024));
    } else if (mode === "invalid") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end("secret-invalid-json");
    } else {
      response.writeHead(200, { "content-type": "application/json" });
      response.end('{"status":"not-ready"}');
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  try {
    const config = {
      ...options(),
      origin: `http://127.0.0.1:${(server.address() as import("node:net").AddressInfo).port}`,
    };
    for (mode of ["redirect", "large", "invalid", "wrong"]) {
      const report = await checkHealth(config, { run });
      assert.equal(report.status, "failed");
      assert(report.http.every((item) => !item.ok));
      assert(!JSON.stringify(report).includes("secret-invalid-json"));
    }
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
});

test("HTTP request timeout is bounded and Docker discovery uses only the explicit project", async () => {
  const calls = [];
  let headersOnly = false;
  const server = createServer((request, response) => {
    if (headersOnly) {
      response.writeHead(200, { "content-type": "application/json" });
      response.write('{"status":');
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  try {
    const config = {
      ...options(),
      timeoutMs: 100,
      origin: `http://127.0.0.1:${(server.address() as import("node:net").AddressInfo).port}`,
    };
    const result = await checkHealth(config, {
      run: async (args) => {
        calls.push(args);
        return run(args);
      },
    });
    assert.equal(result.status, "failed");
    assert(
      result.http.every((item) => item.reason === "unreachable-or-invalid"),
    );
    assert.deepEqual(calls[0], [
      "ps",
      "-a",
      "-q",
      "--no-trunc",
      "--filter",
      "label=com.docker.compose.project=brioche-test",
    ]);
    assert(calls[1].includes("container"));
    assert(!calls[1].some((argument) => argument.includes(".Config.Env")));
    headersOnly = true;
    const stalledBody = await checkHealth(config, { run });
    assert.equal(stalledBody.status, "failed");
    assert(
      stalledBody.http.every(
        (item) => item.reason === "unreachable-or-invalid",
      ),
    );
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
});
