import { before, after, test } from "node:test";
import assert from "node:assert/strict";
import { promisify } from "node:util";
import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { createServer } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { productWebUrl, browserCliUrl } from "../test-product.ts";

const execute = promisify(execFile),
  session = "chef-product-isolation-" + randomUUID();
let server,
  origin,
  opened = false;
async function browser(...args) {
  const { stdout } = await execute(
    process.execPath,
    [...process.execArgv, fileURLToPath(browserCliUrl()), "--session", session, "--json", ...args],
    { timeout: 30000, maxBuffer: 2 * 1024 * 1024 },
  );
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, result.error ?? "Browser command failed");
  return result.data;
}
async function evaluate(code) {
  return (await browser("eval", code)).result;
}
before(async () => {
  const base =
    "/@fs/" + fileURLToPath(productWebUrl("product.ts")).replaceAll("\\", "/");
  server = await createServer({
    configFile: false,
    root: fileURLToPath(new URL(".", import.meta.url)),
    resolve: { dedupe: ["react", "react-dom", "react-router"] },
    plugins: [
      tailwindcss(),
      {
        name: "trusted-hargow-isolation-fixture",
        resolveId(id) {
          if (id === "@chef/product") return "\0trusted-isolation-product";
        },
        load(id) {
          if (id === "\0trusted-isolation-product")
            return `import base from ${JSON.stringify(base)}; export default {...base, id:"hargow", sessionNamespace:"hargow", name:"Hargow"};`;
        },
      },
    ],
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0 },
  });
  await server.listen();
  origin = "http://127.0.0.1:" + server.httpServer.address().port;
});
after(async () => {
  try {
    if (opened) await browser("close");
  } finally {
    await server?.close();
  }
});

test("actual Hargow pending route restores only Hargow writes and account cleanup preserves Brioche", async () => {
  await browser("open", origin + "/?case=pending");
  opened = true;
  await browser(
    "wait",
    "--fn",
    "!!window.qa?.ready && document.querySelectorAll('.library-entry').length===2",
  );
  const before = await evaluate(`(() => {
    const h = Object.keys(sessionStorage).filter(key=>key.startsWith('hargow.learning.v1:'));
    for (const key of h) sessionStorage.setItem(key.replace('hargow.learning.v1:', 'brioche.learning.v1:'), sessionStorage.getItem(key));
    return Object.fromEntries(Object.keys(sessionStorage).filter(key=>key.startsWith('brioche.learning.v1:')).map(key=>[key,sessionStorage.getItem(key)]));
  })()`);
  assert.equal(Object.keys(before).length, 3);
  const snapshot = await browser("snapshot", "-i");
  assert.ok(JSON.stringify(snapshot).includes("确认原提交"));
  assert.equal(
    await evaluate("document.querySelectorAll('.library-entry').length"),
    2,
  );
  await evaluate("qa.confirmExternal(0)");
  await browser(
    "wait",
    "--fn",
    "document.querySelectorAll('.library-entry').length===1",
  );
  assert.deepEqual(
    await evaluate(
      "Object.fromEntries(Object.keys(sessionStorage).filter(key=>key.startsWith('brioche.learning.v1:')).map(key=>[key,sessionStorage.getItem(key)]))",
    ),
    before,
  );
  await evaluate("qa.changeUser()");
  await browser(
    "wait",
    "--fn",
    "!Object.keys(sessionStorage).some(key=>key.startsWith('hargow.learning.v1:qa-account:')) && document.querySelectorAll('.library-entry').length===1",
  );
  assert.equal(
    await evaluate(
      "Object.keys(sessionStorage).filter(key=>key.startsWith('hargow.learning.v1:qa-next:')).length",
    ),
    1,
  );
  assert.deepEqual(
    await evaluate(
      "Object.fromEntries(Object.keys(sessionStorage).filter(key=>key.startsWith('brioche.learning.v1:')).map(key=>[key,sessionStorage.getItem(key)]))",
    ),
    before,
  );
  assert.equal(await evaluate("qa.ownedWrites.length"), 0);
  const { errors } = await browser("errors");
  assert.deepEqual(errors, []);
});
