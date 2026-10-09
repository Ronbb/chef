import { spawn } from "node:child_process";
import { readdir } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const suites = {
  unit: "tests",
  ssr: "ssr-tests",
  browser: "browser-tests",
  "browser-ssr": "browser-ssr-tests",
};
const suite = suites[process.argv[2]];
if (!suite) throw new Error("Expected unit, ssr, browser or browser-ssr");
const root = resolve(process.argv[3] ?? process.cwd());
const directory = fileURLToPath(
  new URL(`../packages/web/${suite}/`, import.meta.url),
);
const files = (await readdir(directory))
  .filter((file) => /\.test\.ts$/.test(file))
  .map((file) => resolve(directory, file));
const child = spawn(
  process.execPath,
  ["--import", "tsx", "--test", ...process.argv.slice(4), ...files],
  {
    stdio: "inherit",
    env: {
      ...process.env,
      CHEF_PRODUCT_WEB: resolve(root, "apps/web"),
      CHEF_BROWSER_CLI: resolve(
        root,
        "node_modules/agent-browser/bin/agent-browser.js",
      ),
    },
  },
);
child.on("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
