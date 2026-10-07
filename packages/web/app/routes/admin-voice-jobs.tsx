import { useCommittedDialog } from "../components/committed-dialog";
import { Link, data, useLocation, useRevalidator } from "react-router";
import { useId, useEffect, useRef, useState } from "react";
import type { AdminVoiceJob } from "@brioche/contracts/AdminVoiceJob";
import type { AdminVoiceJobs } from "@brioche/contracts/AdminVoiceJobs";
import type { VoiceJobStatus } from "@brioche/contracts/VoiceJobStatus";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite } from "../lib/admin.client";
import { usePageCursorFocus } from "../components/page-cursor-focus";
import type { Route } from "./+types/admin-voice-jobs";

export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const params = new URL(request.url).searchParams;
  const query = new URLSearchParams();
  if (params.has("afterId")) query.set("afterId", params.get("afterId")!);
  const id = params.get("jobId");
  if (id && !/^[a-f0-9]{32}$/.test(id))
    throw new Response("任务编号无效。", { status: 400 });
  const list = await getPrivate<AdminVoiceJobs>(
    request,
    `/api/v1/operator/voice-jobs${query.size ? `?${query}` : ""}`,
  );
  const selected = id
    ? await getPrivate<AdminVoiceJob>(
        request,
        `/api/v1/operator/voice-jobs/${id}`,
      )
    : null;
  return data({ ...list, selected }, { headers: headers() });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
const labels: Record<VoiceJobStatus, string> = {
  submitted: "创建请求已提交",
  unknown: "创建结果待核对",
  failed: "提供方拒绝创建",
  processing: "音色正在处理",
  checking: "正在核对音色",
  ready: "音色可用，尚未试听或应用",
  unavailable: "音色不可用",
  modelMismatch: "绑定模型不一致",
  checkFailed: "查询未确认，可再次核对",
};
export default function VoiceJobs({ loaderData }: Route.ComponentProps) {
  const dialogTitleId = useId();
  const location = useLocation(),
    heading = usePageCursorFocus(location.search),
    refresh = useRevalidator();
  const [target, setTarget] = useState<AdminVoiceJob | null>(null),
    [voice, setVoice] = useState(""),
    [reason, setReason] = useState(""),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [pending, setPending] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null),
    busy = useRef(false),
    write = useRef<AbortController | null>(null);
  const openDialog = useCommittedDialog(dialog);
  useEffect(() => () => write.current?.abort(), []);
  const items = loaderData.selected
    ? [
        loaderData.selected,
        ...loaderData.items.filter((i) => i.id !== loaderData.selected!.id),
      ]
    : loaderData.items;
  const waiting = items.some(
    (i) => i.status === "submitted" || i.status === "checking",
  );
  useEffect(() => {
    if (!waiting || refresh.state !== "idle") return;
    const timer = setTimeout(() => refresh.revalidate(), 2000);
    return () => clearTimeout(timer);
  }, [waiting, refresh.state, refresh]);
  async function check(e: React.FormEvent) {
    e.preventDefault();
    if (
      busy.current ||
      !target ||
      !reason.trim() ||
      (!target.voiceId && !voice.trim())
    )
      return;
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      await adminWrite<AdminVoiceJob>(
        `voice-jobs/${target.id}/check`,
        {
          expectedVersion: target.version,
          voiceId: target.voiceId ? null : voice.trim(),
          reason,
        },
        controller.signal,
      );
      controller.signal.throwIfAborted();
      dialog.current?.close();
      setNotice("核对已开始，不会重新创建音色。");
      refresh.revalidate();
    } catch (e) {
      if (!controller.signal.aborted)
        setError(e instanceof Error ? e.message : "核对未确认，请刷新记录。");
    } finally {
      busy.current = false;
      if (!controller.signal.aborted) setPending(false);
    }
  }
  return (
    <section className="admin-page page-arrive">
      <div className="page-heading">
        <h1 ref={heading} tabIndex={-1}>
          音色创建任务
        </h1>
        <Link className="text-button" to="/admin">
          管理首页
        </Link>
      </div>
      <p>
        创建记录绑定固定角色、声音版本和参考授权。请求结果不明确时，先核对已有音色；不会自动重发创建请求。
      </p>
      {!loaderData.configured && (
        <p role="status">提供方服务尚未配置，当前可以查看已有记录。</p>
      )}
      <button
        className="text-button"
        aria-disabled={refresh.state !== "idle"}
        onClick={() => {
          if (refresh.state === "idle") refresh.revalidate();
        }}
      >
        刷新记录
      </button>
      <Link className="text-button" to="/admin/characters">
        从角色库创建音色
      </Link>
      {notice && <p role="status">{notice}</p>}
      {!items.length && <p>还没有音色创建任务。</p>}
      <div className="admin-list">
        {items.map((item) => (
          <article key={item.id} className="admin-card voice-job-card">
            <h2>
              {item.characterId} · 角色 v{item.characterRevision} · 声音 v
              {item.voiceRevision}
            </h2>
            <p className="voice-job-state" role="status">
              {labels[item.status]}
            </p>
            <p>{item.model}</p>
            {item.status === "ready" && (
              <Link
                className="text-button"
                to={`/admin/voice-auditions?jobId=${item.id}`}
              >
                角色声音试听
              </Link>
            )}
            <p>任务前缀：{item.prefix}</p>
            {item.voiceId && (
              <p className="voice-job-id">音色：{item.voiceId}</p>
            )}
            {item.requestId && <p>提供方回执：{item.requestId}</p>}
            <p>
              <time dateTime={item.updatedAt}>
                {new Intl.DateTimeFormat("zh-CN", {
                  timeZone: "Asia/Shanghai",
                  dateStyle: "medium",
                  timeStyle: "short",
                }).format(new Date(item.updatedAt))}
              </time>
            </p>
            <Link
              className="text-button"
              to={`/admin/voice-jobs?jobId=${item.id}`}
            >
              固定任务记录
            </Link>
            {loaderData.configured &&
              !["submitted", "checking"].includes(item.status) && (
                <button
                  className="text-button"
                  aria-disabled={pending}
                  onClick={() => {
                    if (busy.current) return;
                    setTarget(item);
                    setVoice("");
                    setReason("");
                    setError("");
                    openDialog();
                  }}
                >
                  {item.voiceId ? "核对提供方状态" : "找回已有音色"}
                </button>
              )}
          </article>
        ))}
      </div>
      {loaderData.next && (
        <Link
          className="text-button"
          to={`/admin/voice-jobs?afterId=${loaderData.next}`}
        >
          下一页任务
        </Link>
      )}
      <dialog
        aria-labelledby={dialogTitleId}
        className="admin-dialog"
        ref={dialog}
        onCancel={(e) => {
          if (busy.current) e.preventDefault();
        }}
        onClose={() => {
          setTarget(null);
          setError("");
        }}
      >
        <h2 id={dialogTitleId}>核对已有音色</h2>
        <p>这次只查询提供方，不重新创建音色。</p>
        <form className="reference-delivery-form" onSubmit={check}>
          {target && !target.voiceId && (
            <label>
              音色 ID
              <input
                name="recoveryVoice"
                type="text"
                required
                maxLength={200}
                readOnly={pending}
                value={voice}
                onChange={(e) => setVoice(e.target.value)}
              />
              <span>在提供方控制台找到前缀 {target.prefix} 的音色。</span>
            </label>
          )}
          <label>
            核对理由
            <input
              name="voiceCheckReason"
              type="text"
              required
              maxLength={500}
              readOnly={pending}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
            />
          </label>
          {error && <p role="alert">{error}</p>}
          <button
            className="primary"
            type="submit"
            aria-disabled={pending}
            aria-busy={pending}
          >
            {pending ? "正在提交…" : "开始核对"}
          </button>
          <button
            type="button"
            className="text-button"
            aria-disabled={pending}
            onClick={() => {
              if (!busy.current) dialog.current?.close();
            }}
          >
            取消
          </button>
        </form>
      </dialog>
    </section>
  );
}
