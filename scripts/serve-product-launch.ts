import { createServer } from "node:http";
import { readFile } from "node:fs/promises";

// Isolated public launch shell: deliberately no identity, course or database API.
const config = JSON.parse(
  await readFile(process.env.PRODUCT_CONFIG ?? "product.json", "utf8"),
);
if (!/^[a-z][a-z0-9-]{0,63}$/.test(config.id) || !config.launch)
  throw new Error("Product launch configuration required");
const escape = (value) =>
  String(value).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const { heading, description, languageLabel } = config.launch;
for (const value of [
  config.name,
  config.tagline,
  heading,
  description,
  languageLabel,
])
  if (typeof value !== "string" || !value.trim() || value.length > 1000)
    throw new Error("Invalid product launch copy");
const html = `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="theme-color" content="#f7f8ef"><title>${escape(config.name)} · ${escape(config.tagline)}</title><style>
*{box-sizing:border-box}body{margin:0;background:#f7f8ef;color:#29473e;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}header,main,footer{max-width:1080px;margin:auto;padding:28px clamp(24px,5vw,64px)}header{display:flex;align-items:center;gap:14px}header img{width:44px;height:44px}header strong{font-size:30px;letter-spacing:-1px}main{min-height:72vh;display:grid;grid-template-columns:1.1fr 1fr;gap:48px;align-items:center}.badge{display:inline-block;padding:8px 14px;border:1px solid #b9cebc;border-radius:24px;font-size:14px}h1{font-size:clamp(40px,6vw,72px);line-height:1.22;white-space:pre-line;letter-spacing:-2px;margin:28px 0}p{line-height:1.9;color:#637569;max-width:32em}.status{display:inline-flex;align-items:center;gap:10px;margin-top:24px;padding:14px 20px;background:#e2eddc;border-radius:16px;font-weight:600}.dot{width:8px;height:8px;background:#769766;border-radius:50%}.art{width:100%;border-radius:36px;background:#e2eddc;padding:18px;animation:arrive 650ms ease both}footer{color:#7c8b7d;font-size:13px}section{animation:arrive 550ms ease both}@keyframes arrive{from{opacity:0;transform:translateY(16px)}to{opacity:1;transform:translateY(0)}}@media(max-width:700px){main{grid-template-columns:1fr;gap:30px;padding-top:20px}.art{max-width:420px;justify-self:center}header{padding-bottom:12px}h1{margin-top:22px}}@media(prefers-reduced-motion:reduce){section,.art{animation:none}}
</style></head><body><header><img src="/brand.svg" alt=""><strong>${escape(config.name)}.</strong></header><main><section><span class="badge">${escape(languageLabel)}</span><h1>${escape(heading)}</h1><p>${escape(description)}</p><div class="status"><span class="dot" aria-hidden="true"></span>课程准备中</div></section><img class="art" src="/brand.svg" alt="${escape(config.name)} 品牌插图"></main><footer>${escape(config.tagline)}</footer></body></html>`;
const brand = await readFile(process.env.PRODUCT_BRAND ?? "brand.svg");
createServer((req, res) => {
  res.setHeader("X-Content-Type-Options", "nosniff");
  res.setHeader(
    "Content-Security-Policy",
    "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
  );
  const path = req.url?.split("?")[0];
  if (
    !["GET", "HEAD"].includes(req.method) ||
    !["/", "/health", "/brand.svg"].includes(path)
  ) {
    res
      .writeHead(404, { "Content-Type": "text/plain; charset=utf-8" })
      .end("Not found");
    return;
  }
  const body = path === "/health" ? "ok" : path === "/brand.svg" ? brand : html;
  res.writeHead(200, {
    "Content-Type":
      path === "/brand.svg"
        ? "image/svg+xml"
        : path === "/health"
          ? "text/plain; charset=utf-8"
          : "text/html; charset=utf-8",
    "Cache-Control": "no-store",
  });
  res.end(req.method === "HEAD" ? undefined : body);
}).listen(Number(process.env.PORT ?? 3000), "0.0.0.0");
