import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "vite";
import { fileURLToPath } from "node:url";

test("actual shared runtime validates trusted product namespaces before loading private consumers", async () => {
  for (const [config, expected] of [
    [{ id: "chef-fixture" }, "brioche"],
    [{ id: "brioche" }, "brioche"],
    [{ id: "brioche", sessionNamespace: "brioche" }, "brioche"],
    [{ id: "hargow", sessionNamespace: "hargow" }, "hargow"],
    [{ id: "hargow" }, null],
    [{ id: "hargow", sessionNamespace: "brioche" }, null],
    [{ id: "brioche", sessionNamespace: "hargow" }, null],
    [{ id: "hargow", sessionNamespace: "Hargow" }, null],
    [{ id: "hargow", sessionNamespace: null }, null],
  ]) {
    const server = await createServer({
      configFile: false,
      root: fileURLToPath(new URL("../", import.meta.url)),
      plugins: [
        {
          name: "trusted-runtime-config",
          resolveId(id) {
            if (id === "@chef/product") return "\0trusted-runtime-product";
          },
          load(id) {
            if (id === "\0trusted-runtime-product")
              return `export default ${JSON.stringify(config)};`;
          },
        },
      ],
      server: { middlewareMode: true, hmr: false },
      logLevel: "silent",
    });
    try {
      if (expected) {
        const runtime = await server.ssrLoadModule(
          "/app/lib/product-runtime.ts",
        );
        assert.equal(runtime.productNamespace, expected);
      } else {
        await assert.rejects(
          server.ssrLoadModule("/app/lib/product-runtime.ts"),
          (error) =>
            [
              "Invalid product session namespace",
              "Product session namespace does not match product",
            ].includes(error.message),
        );
      }
    } finally {
      await server.close();
    }
  }
});
