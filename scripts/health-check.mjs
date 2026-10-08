import { spawn } from "node:child_process";
import { statfs } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const services = ["postgres", "migrate", "server", "web", "traefik"];
const productServices = ["identity", "learning", "web", "router"];
const limit = 512 * 1024;
function requireCondition(value) {
  if (!value) throw new Error("Invalid health-check options");
}
export function argumentsFor(args) {
  const options = {};
  const allowed = [
    "project",
    "origin",
    "timeout-ms",
    "disk-path",
    "minimum-free-gib",
    "layout",
    "database-project",
  ];
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i]?.slice(2);
    requireCondition(
      args[i]?.startsWith("--") &&
        allowed.includes(key) &&
        !Object.hasOwn(options, key) &&
        args[i + 1] &&
        !args[i + 1].startsWith("--"),
    );
    options[key] = args[i + 1];
  }
  requireCondition(/^[a-z0-9][a-z0-9_-]{0,62}$/.test(options.project ?? ""));
  const layout = options.layout ?? "combined";
  requireCondition(["combined", "product"].includes(layout));
  requireCondition(
    !options["database-project"] ||
      (layout === "product" &&
        /^[a-z0-9][a-z0-9_-]{0,62}$/.test(options["database-project"])),
  );
  const origin = new URL(options.origin ?? "http://127.0.0.1:30075");
  requireCondition(
    ["http:", "https:"].includes(origin.protocol) &&
      !origin.username &&
      !origin.password &&
      origin.pathname === "/" &&
      !origin.search &&
      !origin.hash,
  );
  const timeoutMs = Number(options["timeout-ms"] ?? 5000);
  const minimumFreeGiB = Number(options["minimum-free-gib"] ?? 5);
  requireCondition(
    Number.isInteger(timeoutMs) && timeoutMs >= 100 && timeoutMs <= 30000,
  );
  requireCondition(
    Number.isInteger(minimumFreeGiB) &&
      minimumFreeGiB >= 1 &&
      minimumFreeGiB <= 1048576,
  );
  requireCondition(!options["minimum-free-gib"] || options["disk-path"]);
  return {
    project: options.project,
    layout,
    databaseProject: options["database-project"] ?? null,
    origin: origin.origin,
    timeoutMs,
    diskPath: options["disk-path"] ? resolve(options["disk-path"]) : null,
    minimumFreeGiB,
  };
}

export function dockerOutput(args, timeoutMs) {
  return new Promise((resolveOutput, reject) => {
    const child = spawn("docker", args, {
      shell: false,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let size = 0;
    const buffers = [];
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error("Docker check timed out"));
    }, timeoutMs);
    child.stderr.resume();
    child.stdout.on("data", (buffer) => {
      size += buffer.length;
      if (size > limit) {
        child.kill();
        reject(new Error("Docker output exceeds limit"));
      } else buffers.push(buffer);
    });
    child.once("error", () => {
      clearTimeout(timer);
      reject(new Error("Docker unavailable"));
    });
    child.once("close", (code) => {
      clearTimeout(timer);
      if (code !== 0) reject(new Error("Docker check failed"));
      else resolveOutput(Buffer.concat(buffers).toString("utf8").trim());
    });
  });
}
// Read only selected state fields and Compose identity labels, never Env or health logs.
const format =
  '{"service":{{json (index .Config.Labels "com.docker.compose.service")}},"project":{{json (index .Config.Labels "com.docker.compose.project")}},"oneoff":{{json (index .Config.Labels "com.docker.compose.oneoff")}},"status":{{json .State.Status}},"health":{{if .State.Health}}{{json .State.Health.Status}}{{else}}null{{end}},"exitCode":{{.State.ExitCode}},"oomKilled":{{.State.OOMKilled}},"restarts":{{.RestartCount}}}';
export function assessContainers(rows, project, layout = "combined") {
  const expected =
    layout === "product"
      ? productServices
      : layout === "database"
        ? ["postgres"]
        : services;
  const regular = rows.filter((row) => row.oneoff !== "True");
  const result = expected.map((service) => {
    const matches = regular.filter(
      (row) => row.project === project && row.service === service,
    );
    if (matches.length !== 1)
      return {
        service,
        ok: false,
        reason: matches.length ? "duplicate-service" : "missing-service",
      };
    const row = matches[0];
    const valid =
      typeof row.oomKilled === "boolean" &&
      Number.isSafeInteger(row.exitCode) &&
      Number.isSafeInteger(row.restarts) &&
      row.restarts >= 0;
    const ok =
      valid &&
      !row.oomKilled &&
      (service === "migrate"
        ? row.status === "exited" && row.exitCode === 0
        : row.status === "running" && row.health === "healthy");
    return {
      service,
      ok,
      reason: ok ? "ready" : row.oomKilled ? "oom-killed" : "not-ready",
      restarts: valid ? row.restarts : null,
    };
  });
  if (
    regular.some(
      (row) => row.project !== project || !expected.includes(row.service),
    )
  )
    result.push({
      service: "inventory",
      ok: false,
      reason: "unexpected-container",
    });
  return result;
}
async function containers(options, run, databaseOnly = false) {
  try {
    const listed = await run(
      [
        "ps",
        "-a",
        "-q",
        "--no-trunc",
        "--filter",
        `label=com.docker.compose.project=${options.project}`,
        ...(databaseOnly
          ? ["--filter", "label=com.docker.compose.service=postgres"]
          : []),
      ],
      options.timeoutMs,
    );
    const ids = listed ? listed.split(/\s+/) : [];
    if (ids.length > 32 || ids.some((id) => !/^[a-f0-9]{64}$/.test(id)))
      throw new Error("Invalid inventory");
    const output = ids.length
      ? await run(
          ["container", "inspect", "--format", format, ...ids],
          options.timeoutMs,
        )
      : "";
    const rows = output
      ? output.split(/\r?\n/).map((line) => JSON.parse(line))
      : [];
    return assessContainers(
      rows,
      options.project,
      databaseOnly ? "database" : options.layout,
    );
  } catch {
    return [
      {
        service: "inventory",
        ok: false,
        reason: "docker-unavailable-or-invalid",
      },
    ];
  }
}
async function boundedText(response) {
  const reader = response.body?.getReader();
  if (!reader) return "";
  const chunks = [];
  let size = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) throw new Error("Response exceeds limit");
      chunks.push(value);
    }
    return Buffer.concat(chunks).toString("utf8");
  } finally {
    await reader.cancel().catch(() => {});
  }
}
async function endpoint(options, path, expected, fetcher) {
  const start = performance.now();
  try {
    const response = await fetcher(new URL(path, options.origin), {
      redirect: "manual",
      cache: "no-store",
      signal: AbortSignal.timeout(options.timeoutMs),
      headers: { Accept: expected ? "application/json" : "text/html" },
    });
    if (response.status !== 200) {
      await response.body?.cancel();
      return {
        path,
        ok: false,
        reason: "http-status",
        status: response.status,
        durationMs: Math.round(performance.now() - start),
      };
    }
    const text = await boundedText(response);
    const ok = expected
      ? (response.headers.get("content-type") ?? "").includes(
          "application/json",
        ) && JSON.parse(text).status === expected
      : (response.headers.get("content-type") ?? "").includes("text/html") &&
        /<!doctype html/i.test(text);
    return {
      path,
      ok,
      reason: ok ? "ready" : "unexpected-response",
      durationMs: Math.round(performance.now() - start),
    };
  } catch {
    return {
      path,
      ok: false,
      reason: "unreachable-or-invalid",
      durationMs: Math.round(performance.now() - start),
    };
  }
}
export async function checkHealth(
  options,
  { run = dockerOutput, fetcher = fetch, filesystem = statfs } = {},
) {
  const [inventory, database, http] = await Promise.all([
    containers(options, run),
    options.databaseProject
      ? containers({ ...options, project: options.databaseProject }, run, true)
      : null,
    Promise.all(
      [
        ["/api/health", "ok"],
        ["/api/ready", "ready"],
        ["/health", "ok"],
        ["/", null],
      ].map(([path, expected]) => endpoint(options, path, expected, fetcher)),
    ),
  ]);
  let disk = null;
  if (options.diskPath) {
    try {
      const stats = await filesystem(options.diskPath, { bigint: true });
      const free = stats.bavail * stats.bsize;
      const minimum = BigInt(options.minimumFreeGiB) * 1024n ** 3n;
      disk = {
        ok: free >= minimum,
        reason: free >= minimum ? "ready" : "low-free-space",
        freeBytes: free.toString(),
        minimumFreeBytes: minimum.toString(),
      };
    } catch {
      disk = { ok: false, reason: "disk-unavailable" };
    }
  }
  const ok =
    inventory.every((item) => item.ok) &&
    (!database || database.every((item) => item.ok)) &&
    http.every((item) => item.ok) &&
    (!disk || disk.ok);
  return {
    schemaVersion: "1.0",
    checkedAt: new Date().toISOString(),
    status: ok ? "healthy" : "failed",
    containers: inventory,
    database,
    http,
    disk,
  };
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  try {
    const report = await checkHealth(argumentsFor(process.argv.slice(2)));
    process.stdout.write(JSON.stringify(report) + "\n");
    process.exitCode = report.status === "healthy" ? 0 : 1;
  } catch {
    process.stdout.write(
      JSON.stringify({
        schemaVersion: "1.0",
        status: "failed",
        reason: "invalid-options-or-check-failure",
      }) + "\n",
    );
    process.exitCode = 2;
  }
}
