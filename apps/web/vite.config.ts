import { reactRouter } from "@react-router/dev/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
export default defineConfig({
  resolve: {
    alias: {
      "@chef/product": fileURLToPath(new URL("./product.ts", import.meta.url)),
    },
    dedupe: ["react", "react-dom", "react-router"],
  },
  plugins: [tailwindcss(), reactRouter()],
  server: {
    host: "0.0.0.0",
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": {
        target: process.env.INTERNAL_API_URL ?? "http://127.0.0.1:3001",
      },
    },
  },
});
