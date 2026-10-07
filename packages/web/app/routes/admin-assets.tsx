import { Form, Link, data, useLocation } from "react-router";
import type { AdminAssets } from "@brioche/contracts/AdminAssets";
import { getIdentity, getPrivate } from "../lib/api.server";
import { usePageCursorFocus } from "../components/page-cursor-focus";
import { AssetUpload } from "../components/admin-asset-upload";
import type { Route } from "./+types/admin-assets";

export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const query = new URLSearchParams();
  const input = new URL(request.url).searchParams;
  for (const key of ["afterId", "afterRevision", "q"]) {
    const value = input.get(key);
    if (value !== null) query.set(key, value);
  }
  const result = await getPrivate<AdminAssets>(
    request,
    `/api/v1/operator/assets${query.size ? `?${query}` : ""}`,
  );
  return data({ ...result, q: input.get("q") ?? "" }, { headers: headers() });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
export default function Assets({ loaderData }: Route.ComponentProps) {
  const location = useLocation();
  const heading = usePageCursorFocus(location.search);
  const next = new URLSearchParams();
  if (loaderData.next) {
    next.set("afterId", loaderData.next.assetId);
    next.set("afterRevision", String(loaderData.next.revision));
    if (loaderData.q) next.set("q", loaderData.q);
  }
  return (
    <section className="admin-page page-arrive">
      <div className="admin-heading">
        <h1 ref={heading} tabIndex={-1}>
          图片素材
        </h1>
        <Link className="text-button" to="/admin">
          管理员后台
        </Link>
      </div>
      <AssetUpload />
      <Form method="get" className="admin-toolbar" key={loaderData.q}>
        <label>
          搜索素材
          <input
            type="search"
            name="q"
            maxLength={200}
            defaultValue={loaderData.q}
            placeholder="素材编号或图片说明"
          />
        </label>
        <button className="secondary" type="submit">
          搜索
        </button>
      </Form>
      <div className="admin-asset-grid">
        {loaderData.items.map(
          ({ asset, source, license, creator, rightsConfirmed, byteSize }) => (
            <article
              className="admin-card admin-asset"
              key={`${asset.assetId}:${asset.revision}`}
            >
              <img
                src={asset.url}
                alt={asset.altZh}
                width={asset.width}
                height={asset.height}
                loading="lazy"
              />
              <h2>{asset.altZh}</h2>
              <p className="eyebrow">
                {asset.assetId} · 版本 {asset.revision}
              </p>
              <dl>
                <dt>来源</dt>
                <dd>{source}</dd>
                <dt>许可</dt>
                <dd>{license}</dd>
                <dt>创作者</dt>
                <dd>{creator}</dd>
                <dt>署名</dt>
                <dd>{asset.creditZh}</dd>
                <dt>授权记录</dt>
                <dd>{rightsConfirmed ? "已确认" : "未确认"}</dd>
                <dt>文件</dt>
                <dd>
                  {asset.mimeType} · {asset.width} × {asset.height} ·{" "}
                  {(byteSize / 1024).toFixed(1)} KB
                </dd>
                <dt>SHA-256</dt>
                <dd className="admin-asset-hash">{asset.sha256}</dd>
              </dl>
            </article>
          ),
        )}
      </div>
      {!loaderData.items.length && <p>没有符合条件的素材。</p>}
      <div className="admin-card-actions">
        {location.search && (
          <Link className="text-button" to="/admin/assets">
            全部素材
          </Link>
        )}
        {loaderData.next && (
          <Link className="secondary" to={`/admin/assets?${next}`}>
            下一页
          </Link>
        )}
      </div>
    </section>
  );
}
