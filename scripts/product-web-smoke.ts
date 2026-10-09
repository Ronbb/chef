import assert from "node:assert/strict";
import { createServer } from "node:http";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

// Controlled anonymous API only; no database, production configuration or provider.
if (process.argv[2] === "fixture") {
  const requests = [];
  createServer((request, response) => {
    response.setHeader("Content-Type", "application/json");
    if (request.url === "/__smoke/requests") {
      response.end(JSON.stringify(requests));
      return;
    }
    requests.push({ method: request.method, path: request.url });
    if (request.method === "GET" && request.url === "/api/v1/me") {
      response.writeHead(401).end("{}");
    } else if (request.method === "GET" && request.url === "/api/v2/catalog") {
      response.end(JSON.stringify({ levels: [], developmentFixture: false }));
    } else {
      response.writeHead(500).end("{}");
    }
  }).listen(3001, "0.0.0.0");
} else {
  const [image, product] = process.argv.slice(2);
  assert.ok(
    image && /^[a-zA-Z0-9][a-zA-Z0-9_.:/@-]+$/.test(image),
    "Fixed built image required",
  );
  const labels = {
    hargow: ["Hargow", "粤语学习者", "粤语入门"],
    brioche: ["Brioche", "法语学习者", "A1–A2"],
  }[product];
  assert.ok(labels, "Known product required");
  const run = promisify(execFile);
  const network = "chef-web-smoke-" + randomUUID();
  const api = network + "-api",
    web = network + "-web";
  const docker = async (...args) =>
    (
      await run("docker", args, { timeout: 30000, maxBuffer: 1024 * 1024 })
    ).stdout.trim();
  try {
    await docker("network", "create", network);
    await docker(
      "run",
      "-d",
      "--name",
      api,
      "--network",
      network,
      "--network-alias",
      "smoke-api",
      "--read-only",
      "--cap-drop",
      "ALL",
      "--security-opt",
      "no-new-privileges:true",
      "--mount",
      `type=bind,src=${dirname(fileURLToPath(import.meta.url))},dst=/smoke,readonly`,
      image,
      "node",
      "/smoke/product-web-smoke.ts",
      "fixture",
    );
    await docker(
      "run",
      "-d",
      "--name",
      web,
      "--network",
      network,
      "--read-only",
      "--cap-drop",
      "ALL",
      "--security-opt",
      "no-new-privileges:true",
      "-e",
      "INTERNAL_API_URL=http://smoke-api:3001",
      "-p",
      "127.0.0.1::3000",
      image,
    );
    const port = (await docker("port", web, "3000/tcp")).match(
      /127\.0\.0\.1:(\d+)/,
    )?.[1];
    assert.ok(port, "Isolated localhost port required");
    const origin = `http://127.0.0.1:${port}`;
    let ready = false;
    for (let attempt = 0; attempt < 40; attempt++) {
      try {
        ready = (
          await fetch(origin + "/health", { signal: AbortSignal.timeout(1000) })
        ).ok;
      } catch {}
      if (ready) break;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    assert.ok(ready, "Actual SSR image must become healthy");
    const uid = await docker("exec", web, "node", "-e", "console.log(process.getuid())");
    assert.ok(Number(uid) > 0, "Product Web must run without root");
    for (const path of ["/", "/profile", "/courses", "/login", "/admin"]) {
      const response = await fetch(origin + path, {
        signal: AbortSignal.timeout(10000),
      });
      assert.equal(response.status, path === "/admin" ? 401 : 200, path);
      const html = await response.text();
      assert.ok(html.includes(labels[0]), `${path}: product identity`);
      assert.ok(
        !html.includes("课程准备中"),
        "Legacy launch page must not replace SSR",
      );
      if (path === "/profile")
        for (const label of labels.slice(1))
          assert.ok(html.includes(label), label);
    }
    const requests = JSON.parse(
      await docker(
        "exec",
        api,
        "node",
        "-e",
        "fetch('http://localhost:3001/__smoke/requests').then(r=>r.text()).then(console.log)",
      ),
    );
    assert.ok(requests.some((r) => r.path === "/api/v2/catalog"));
    assert.ok(requests.some((r) => r.path === "/api/v1/me"));
    assert.ok(
      requests.every(
        (r) =>
          r.method === "GET" &&
          ["/api/v1/me", "/api/v2/catalog"].includes(r.path),
      ),
      "No legacy catalog, private reads, writes or paid provider calls",
    );
    console.log(
      `${product}: actual non-root read-only SSR image, profile copy, neutral catalog and anonymous admin denial passed`,
    );
  } finally {
    for (const container of [web, api])
      await docker("rm", "-f", container).catch(() => {});
    await docker("network", "rm", network).catch(() => {});
  }
}
