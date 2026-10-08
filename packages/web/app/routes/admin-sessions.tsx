import { productNamespace } from "../lib/product-runtime";
import { useCommittedDialog } from "../components/committed-dialog";
import { Link, data, useLocation, useRevalidator } from "react-router";
import { useId, useEffect, useRef, useState } from "react";
import type { AdminSessions } from "@brioche/contracts/AdminSessions";
import type { AdminSession } from "@brioche/contracts/AdminSession";
import type { AdminRevokeSessionResult } from "@brioche/contracts/AdminRevokeSessionResult";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite } from "../lib/admin.client";
import { announceIdentityChange } from "../lib/identity-sync";
import { clearLearningDrafts } from "../lib/learning-draft";
import { useLearning } from "../components/learning";
import { usePageCursorFocus } from "../components/page-cursor-focus";
import type { Route } from "./+types/admin-sessions";

export async function loader({ request, params }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  if (!/^[1-9][0-9]{0,18}$/.test(params.accountId))
    throw new Response("账号编号无效。", { status: 400 });
  const query = new URLSearchParams();
  const after = new URL(request.url).searchParams.get("afterId");
  if (after !== null) query.set("afterId", after);
  return data(
    await getPrivate<AdminSessions>(
      request,
      `/api/v1/operator/accounts/${params.accountId}/sessions${query.size ? `?${query}` : ""}`,
    ),
    { headers: { "Cache-Control": "private, no-store", Vary: "Cookie" } },
  );
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}

export default function Sessions({
  loaderData: sessions,
}: Route.ComponentProps) {
  const dialogTitleId = useId();
  const location = useLocation();
  const revalidator = useRevalidator();
  const heading = usePageCursorFocus(location.search);
  const learning = useLearning();
  const dialog = useRef<HTMLDialogElement>(null);
  const openDialog = useCommittedDialog(dialog);
  const write = useRef<AbortController | null>(null);
  const busy = useRef(false);
  useEffect(() => () => write.current?.abort(), []);
  const [target, setTarget] = useState<AdminSession | null>(null);
  const [reason, setReason] = useState("");
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [notice, setNotice] = useState("");
  function open(session: AdminSession) {
    setTarget(session);
    setReason("");
    setError("");
    setNotice("");
    openDialog();
  }
  async function submit() {
    if (busy.current || !target || !reason.trim()) return;
    busy.current = true;
    const controller = new AbortController();
    write.current = controller;
    setPending(true);
    setError("");
    try {
      const result = await adminWrite<AdminRevokeSessionResult>(
        `accounts/${sessions.account.id}/sessions/${target.id}/revoke`,
        { reason },
        controller.signal,
      );
      controller.signal.throwIfAborted();
      dialog.current?.close();
      if (result.current) {
        learning.stop();
        if (learning.profile)
          clearLearningDrafts(learning.profile.id, productNamespace);
        announceIdentityChange(productNamespace);
        window.location.assign("/login");
      } else {
        setNotice("登录会话已撤销。");
        void revalidator.revalidate();
      }
    } catch (error) {
      if (!controller.signal.aborted)
        setError(
          error instanceof Error
            ? error.message
            : "操作未确认，请刷新核对后重试。",
        );
    } finally {
      busy.current = false;
      if (!controller.signal.aborted) setPending(false);
    }
  }
  const path = `/admin/accounts/${sessions.account.id}/sessions`;
  return (
    <section className="admin-page page-arrive">
      <div className="admin-heading">
        <h1 ref={heading} tabIndex={-1}>
          登录会话
        </h1>
        <Link className="text-button" to="/admin/accounts">
          账号管理
        </Link>
      </div>
      <h2>{sessions.account.displayName}</h2>
      <p>{sessions.account.email}</p>
      <p role="status">{notice}</p>
      {!sessions.items.length && <p>没有有效的登录会话。</p>}
      <div className="admin-list">
        {sessions.items.map((session, index) => (
          <article className="admin-card" key={session.id}>
            <div className="admin-card-heading">
              <h2>登录会话 {index + 1}</h2>
              {session.current && <span>当前浏览器</span>}
            </div>
            <p>
              有效至{" "}
              <time dateTime={session.expiresAt}>
                {new Intl.DateTimeFormat("zh-CN", {
                  timeZone: "Asia/Shanghai",
                  dateStyle: "medium",
                  timeStyle: "short",
                }).format(new Date(session.expiresAt))}
              </time>
              （中国时间）
            </p>
            <button className="text-button" onClick={() => open(session)}>
              撤销此会话
            </button>
          </article>
        ))}
      </div>
      {sessions.nextId && (
        <Link
          className="text-button"
          to={`${path}?${new URLSearchParams({ afterId: sessions.nextId })}`}
        >
          下一页
        </Link>
      )}
      {new URLSearchParams(location.search).has("afterId") && (
        <Link className="text-button" to={path}>
          返回第一页
        </Link>
      )}
      <dialog
        aria-labelledby={dialogTitleId}
        ref={dialog}
        className="choice-dialog admin-dialog"
        onCancel={(event) => {
          if (busy.current) event.preventDefault();
        }}
        onClose={() => {
          setTarget(null);
          setReason("");
          setError("");
        }}
      >
        <h2 id={dialogTitleId}>撤销登录会话</h2>
        <p>
          {target?.current
            ? "这是当前浏览器的会话，撤销后你会退出登录。"
            : "这个会话将退出登录，其他会话保持有效。"}
        </p>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <label htmlFor="session-reason">操作理由</label>
          <textarea
            id="session-reason"
            required
            maxLength={300}
            value={reason}
            readOnly={pending}
            onChange={(event) => setReason(event.target.value)}
          />
          <p role="alert">{error}</p>
          <button
            className="primary"
            disabled={!reason.trim()}
            aria-disabled={pending}
            aria-busy={pending}
          >
            {pending ? "正在撤销" : "确认撤销"}
          </button>
        </form>
        <button
          className="text-button"
          aria-disabled={pending}
          onClick={() => {
            if (!busy.current) dialog.current?.close();
          }}
        >
          关闭
        </button>
      </dialog>
    </section>
  );
}
