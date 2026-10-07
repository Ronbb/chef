import { useCommittedDialog } from "../components/committed-dialog";
import { Link, data, useLocation, useRevalidator } from "react-router";
import { useId, useEffect, useRef, useState } from "react";
import type { AdminPendingTokens } from "@brioche/contracts/AdminPendingTokens";
import type { AdminPendingToken } from "@brioche/contracts/AdminPendingToken";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite } from "../lib/admin.client";
import { usePageCursorFocus } from "../components/page-cursor-focus";
import type { Route } from "./+types/admin-tokens";
export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const query = new URLSearchParams();
  const input = new URL(request.url).searchParams;
  for (const key of ["afterId", "kind"]) {
    const value = input.get(key);
    if (value !== null) query.set(key, value);
  }
  return data(
    await getPrivate<AdminPendingTokens>(
      request,
      `/api/v1/operator/accounts/pending-tokens${query.size ? `?${query}` : ""}`,
    ),
    { headers: headers() },
  );
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
export default function Tokens({ loaderData: tokens }: Route.ComponentProps) {
  const dialogTitleId = useId();
  const location = useLocation();
  const heading = usePageCursorFocus(location.search);
  const refresh = useRevalidator();
  const [target, setTarget] = useState<AdminPendingToken | null>(null);
  const [reason, setReason] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [pending, setPending] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const openDialog = useCommittedDialog(dialog);
  const busy = useRef(false);
  const write = useRef<AbortController | null>(null);
  useEffect(() => () => write.current?.abort(), []);
  function open(item: AdminPendingToken) {
    setTarget(item);
    setReason("");
    setError("");
    openDialog();
  }
  async function submit() {
    if (busy.current || !target || !reason.trim()) return;
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      await adminWrite<boolean>(
        `accounts/pending-tokens/${target.id}/revoke`,
        { reason },
        controller.signal,
      );
      controller.signal.throwIfAborted();
      dialog.current?.close();
      setNotice("链接已撤销。");
      refresh.revalidate();
    } catch (e) {
      if (!controller.signal.aborted)
        setError(e instanceof Error ? e.message : "操作未确认，请刷新核对。");
    } finally {
      busy.current = false;
      if (!controller.signal.aborted) setPending(false);
    }
  }
  const kind = new URLSearchParams(location.search).get("kind");
  const next = new URLSearchParams();
  if (kind) next.set("kind", kind);
  if (tokens.nextId) next.set("afterId", tokens.nextId);
  return (
    <section className="admin-page page-arrive">
      <div className="admin-heading">
        <h1 ref={heading} tabIndex={-1}>
          待使用的链接
        </h1>
        <Link className="text-button" to="/admin/accounts">
          账号管理
        </Link>
      </div>
      <p>
        这里只列出有效、尚未使用的邀请与密码重置记录。原链接只在发放时显示。
      </p>
      <nav aria-label="链接类型">
        <Link
          className="text-button"
          aria-current={!kind ? "page" : undefined}
          to="/admin/tokens"
        >
          全部
        </Link>
        <Link
          className="text-button"
          aria-current={kind === "invite" ? "page" : undefined}
          to="/admin/tokens?kind=invite"
        >
          邀请
        </Link>
        <Link
          className="text-button"
          aria-current={kind === "reset" ? "page" : undefined}
          to="/admin/tokens?kind=reset"
        >
          密码重置
        </Link>
      </nav>
      <p role="status">{notice}</p>
      {!tokens.items.length && <p>没有待使用的链接。</p>}
      <div className="admin-list">
        {tokens.items.map((item) => (
          <article className="admin-card" key={item.id}>
            <h2>{item.email}</h2>
            <p>
              {item.kind === "reset"
                ? "密码重置"
                : item.role === "operator"
                  ? "管理员邀请"
                  : "学习者邀请"}
            </p>
            <p>
              有效至{" "}
              <time dateTime={item.expiresAt}>
                {new Intl.DateTimeFormat("zh-CN", {
                  timeZone: "Asia/Shanghai",
                  dateStyle: "medium",
                  timeStyle: "short",
                }).format(new Date(item.expiresAt))}
              </time>
            </p>
            <button className="text-button" onClick={() => open(item)}>
              撤销链接
            </button>
          </article>
        ))}
      </div>
      {tokens.nextId && (
        <Link className="text-button" to={`/admin/tokens?${next}`}>
          下一页
        </Link>
      )}
      <dialog
        aria-labelledby={dialogTitleId}
        ref={dialog}
        className="admin-dialog"
        onCancel={(e) => {
          if (busy.current) e.preventDefault();
        }}
        onClose={() => {
          setTarget(null);
          setReason("");
          setError("");
        }}
      >
        <h2 id={dialogTitleId}>撤销链接</h2>
        <p>{target?.email}</p>
        <p>撤销后，原链接将无法使用。已有账号和已登录的会话继续保留。</p>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <label htmlFor="token-reason">撤销原因</label>
          <textarea
            id="token-reason"
            required
            maxLength={300}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
          <p role="alert">{error}</p>
          <button
            className="primary"
            type="submit"
            aria-busy={pending}
            aria-disabled={pending}
          >
            确认撤销
          </button>
          <button
            className="text-button"
            type="button"
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
