import { Form, Link, data, useLocation } from "react-router";
import { useEffect, useRef, useState } from "react";
import type { AdminRecordings } from "@brioche/contracts/AdminRecordings";
import type { AudioAsset } from "@brioche/contracts/AudioAsset";
import { getIdentity, getPrivate } from "../lib/api.server";
import { usePageCursorFocus } from "../components/page-cursor-focus";
import { RecordingPlayer } from "../lib/recording-playback";
import { Icon } from "../components/icon";
import { useLearning } from "../components/learning";
import { RecordingUpload } from "../components/admin-recording-upload";
import type { Route } from "./+types/admin-recordings";

export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const query = new URLSearchParams(),
    input = new URL(request.url).searchParams;
  for (const key of ["afterId", "afterRevision", "q"]) {
    const value = input.get(key);
    if (value !== null) query.set(key, value);
  }
  const result = await getPrivate<AdminRecordings>(
    request,
    `/api/v1/operator/recordings${query.size ? `?${query}` : ""}`,
  );
  return data({ ...result, q: input.get("q") ?? "" }, { headers: headers() });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
export default function Recordings({ loaderData }: Route.ComponentProps) {
  const location = useLocation(),
    heading = usePageCursorFocus(location.search);
  const { toast, stop } = useLearning();
  const playback = useRef<RecordingPlayer | null>(null);
  const [active, setActive] = useState<string | null>(null),
    [status, setStatus] = useState("idle"),
    [progress, setProgress] = useState(0);
  useEffect(() => {
    playback.current = new RecordingPlayer();
    return () => playback.current?.stop();
  }, []);
  useEffect(() => {
    playback.current?.stop();
    setActive(null);
    setStatus("idle");
    setProgress(0);
  }, [location.search]);
  useEffect(() => {
    const release = () => {
      playback.current?.stop();
      setStatus("idle");
      setActive(null);
    };
    window.addEventListener("pagehide", release);
    return () => window.removeEventListener("pagehide", release);
  }, []);
  function listen(asset: AudioAsset) {
    const id = `${asset.assetId}:${asset.revision}`;
    if (id === active && playback.current?.isActive) {
      if (status === "paused") playback.current.resume();
      else {
        playback.current.pause();
        setStatus("paused");
      }
      return;
    }
    stop();
    setActive(id);
    setProgress(0);
    playback.current?.play(
      { url: asset.url, startMs: 0, endMs: asset.durationMs, cues: [] },
      1,
      {
        status: setStatus,
        progress: setProgress,
        end: () => {
          setStatus("idle");
          setActive(null);
        },
        error: (blocked) => {
          setStatus("idle");
          setActive(null);
          toast(
            blocked
              ? "浏览器未允许播放，请再次点击。"
              : "录音无法读取，请刷新核对登录状态与文件。",
          );
        },
      },
    );
  }
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
          录音管理
        </h1>
        <Link className="text-button" to="/admin">
          管理员后台
        </Link>
      </div>
      <RecordingUpload />
      <Form method="get" className="admin-toolbar" key={loaderData.q}>
        <label>
          搜索录音
          <input
            type="search"
            name="q"
            maxLength={200}
            defaultValue={loaderData.q}
            placeholder="录音编号或署名"
          />
        </label>
        <button className="secondary" type="submit">
          搜索
        </button>
      </Form>
      <div className="admin-list">
        {loaderData.items.map(
          ({
            asset,
            source,
            license,
            creator,
            rightsConfirmed,
            byteSize,
            sampleRate,
            channels,
          }) => {
            const selected = active === `${asset.assetId}:${asset.revision}`;
            return (
              <article
                className="admin-card admin-asset admin-recording"
                key={`${asset.assetId}:${asset.revision}`}
              >
                <h2>{asset.assetId}</h2>
                <p className="eyebrow">
                  版本 {asset.revision} · {(asset.durationMs / 1000).toFixed(2)}{" "}
                  秒
                </p>
                <button
                  className="recording-preview"
                  aria-label={`${selected && status === "playing" ? "暂停" : "试听"} ${asset.assetId} 版本 ${asset.revision}`}
                  aria-busy={selected && status === "loading"}
                  onClick={() => listen(asset)}
                >
                  <span className="recording-preview-track">
                    <span
                      style={{ width: `${selected ? progress * 100 : 0}%` }}
                    />
                  </span>
                  <Icon
                    name={selected && status === "playing" ? "pause" : "play"}
                  />
                  <span className="recording-preview-track" />
                </button>
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
                    {asset.mimeType} · {sampleRate} Hz · {channels} 声道 ·{" "}
                    {(byteSize / 1024).toFixed(1)} KB
                  </dd>
                  <dt>SHA-256</dt>
                  <dd className="admin-asset-hash">{asset.sha256}</dd>
                </dl>
              </article>
            );
          },
        )}
      </div>
      {!loaderData.items.length && <p>没有符合条件的录音。</p>}
      <div className="admin-card-actions">
        {location.search && (
          <Link className="text-button" to="/admin/recordings">
            全部录音
          </Link>
        )}
        {loaderData.next && (
          <Link className="secondary" to={`/admin/recordings?${next}`}>
            下一页
          </Link>
        )}
      </div>
    </section>
  );
}
