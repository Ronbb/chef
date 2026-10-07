import { useCommittedDialog } from "../components/committed-dialog";
import {
  Link,
  data,
  useLocation,
  useRevalidator,
  useNavigate,
} from "react-router";
import { useId, useEffect, useRef, useState } from "react";
import type { AdminCharacterVoice } from "@brioche/contracts/AdminCharacterVoice";
import type { AdminReferenceGrant } from "@brioche/contracts/AdminReferenceGrant";
import type { AdminReferenceGrants } from "@brioche/contracts/AdminReferenceGrants";
import type { AdminReferenceGrantResult } from "@brioche/contracts/AdminReferenceGrantResult";
import type { AdminVoiceJob } from "@brioche/contracts/AdminVoiceJob";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite } from "../lib/admin.client";
import { usePageCursorFocus } from "../components/page-cursor-focus";
import type { Route } from "./+types/admin-voice-references";

export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const input = new URL(request.url).searchParams,
    after = input.get("afterId");
  const query = new URLSearchParams();
  if (after !== null) query.set("afterId", after);
  const grants = await getPrivate<AdminReferenceGrants>(
    request,
    `/api/v1/operator/voice-references${query.size ? `?${query}` : ""}`,
  );
  const id = input.get("characterId"),
    cr = input.get("characterRevision"),
    vr = input.get("voiceRevision");
  let selected: AdminCharacterVoice | null = null;
  if (id || cr || vr) {
    if (
      !id ||
      !/^[a-zA-Z0-9_-]{1,100}$/.test(id) ||
      !cr ||
      !vr ||
      ![cr, vr].every(
        (v) => /^[1-9][0-9]{0,9}$/.test(v) && Number(v) <= 2147483647,
      )
    )
      throw new Response("版本无效。", { status: 400 });
    selected = await getPrivate<AdminCharacterVoice>(
      request,
      `/api/v1/operator/characters/${id}/${cr}/voices/${vr}`,
    );
  }
  return data({ ...grants, selected }, { headers: headers() });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
export default function ReferenceGrants({ loaderData }: Route.ComponentProps) {
  const dialogTitleId = useId();
  const navigate = useNavigate();
  const [costConfirmed, setCostConfirmed] = useState(false);
  const location = useLocation(),
    heading = usePageCursorFocus(location.search),
    refresh = useRevalidator();
  const [reason, setReason] = useState(""),
    [confirmed, setConfirmed] = useState(false);
  const [result, setResult] = useState<AdminReferenceGrantResult | null>(null);
  const [url, setUrl] = useState(""),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [pending, setPending] = useState(false),
    [target, setTarget] = useState<AdminReferenceGrant | null>(null);
  const [revokeReason, setRevokeReason] = useState("");
  const busy = useRef(false),
    write = useRef<AbortController | null>(null),
    dialog = useRef<HTMLDialogElement>(null);
  const openDialog = useCommittedDialog(dialog);
  useEffect(() => () => write.current?.abort(), []);
  const selected = loaderData.selected;
  const selectionKey = selected
    ? `${selected.character.characterId}:${selected.character.revision}:${selected.voiceRevision}`
    : "";
  useEffect(() => {
    setResult(null);
    setUrl("");
    setReason("");
    setConfirmed(false);
    setCostConfirmed(false);
  }, [selectionKey]);
  async function createVoice(e: React.FormEvent) {
    e.preventDefault();
    if (busy.current || !result || !costConfirmed) return;
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      const job = await adminWrite<AdminVoiceJob>(
        "voice-jobs",
        {
          grantId: result.grant.id,
          token: result.path.split("/").at(-1),
          costConfirmed,
          reason,
        },
        controller.signal,
      );
      controller.signal.throwIfAborted();
      navigate(`/admin/voice-jobs?jobId=${job.id}`);
    } catch (e) {
      if (!controller.signal.aborted)
        setError(
          `${e instanceof Error ? e.message : "创建结果未确认。"} 请先查看音色创建任务，核对后再操作。`,
        );
    } finally {
      busy.current = false;
      if (!controller.signal.aborted) setPending(false);
    }
  }
  async function issue(event: React.FormEvent) {
    event.preventDefault();
    if (busy.current || !selected || !confirmed || !reason.trim()) return;
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      const saved = await adminWrite<AdminReferenceGrantResult>(
        "voice-references",
        {
          characterId: selected.character.characterId,
          characterRevision: selected.character.revision,
          voiceRevision: selected.voiceRevision,
          singleSpeakerConfirmed: confirmed,
          reason,
        },
        controller.signal,
      );
      controller.signal.throwIfAborted();
      setResult(saved);
      setCostConfirmed(false);
      setUrl(new URL(saved.path, window.location.origin).href);
      setNotice("限时交付已授权。尚未创建提供方音色。");
      refresh.revalidate();
    } catch (e) {
      if (!controller.signal.aborted)
        setError(
          e instanceof Error ? e.message : "授权未确认，请刷新核对记录。",
        );
    } finally {
      busy.current = false;
      if (!controller.signal.aborted) setPending(false);
    }
  }
  async function revoke(event: React.FormEvent) {
    event.preventDefault();
    if (busy.current || !target || !revokeReason.trim()) return;
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      await adminWrite<AdminReferenceGrant>(
        `voice-references/${target.id}/revoke`,
        { reason: revokeReason },
        controller.signal,
      );
      controller.signal.throwIfAborted();
      if (result?.grant.id === target.id) {
        setResult(null);
        setUrl("");
      }
      dialog.current?.close();
      setNotice("交付凭据已撤销。");
      refresh.revalidate();
    } catch (e) {
      if (!controller.signal.aborted)
        setError(e instanceof Error ? e.message : "撤销未确认，请刷新核对。");
    } finally {
      busy.current = false;
      if (!controller.signal.aborted) setPending(false);
    }
  }
  return (
    <section className="admin-page page-arrive">
      <div className="admin-heading">
        <h1 ref={heading} tabIndex={-1}>
          参考录音交付
        </h1>
        <Link className="text-button" to="/admin/characters">
          角色库
        </Link>
      </div>
      <p>
        交付凭据15分钟有效，最多读取32次，可随时撤销。它只提供选定录音，不创建音色或发布课程。
      </p>
      <p role="status">{notice}</p>
      {error && !target && <p role="alert">{error}</p>}
      {selected?.profile?.referenceAudio && (
        <article className="admin-card">
          <h2>
            {selected.character.displayName} · 声音 v{selected.voiceRevision}
          </h2>
          <p>
            {selected.profile.referenceAudio.assetId} · v
            {selected.profile.referenceAudio.revision}
          </p>
          <p>
            用于声音复刻的授权：
            {selected.profile.referenceAudio.cloningPermission}
          </p>
          {!result ? (
            <form className="reference-delivery-form" onSubmit={issue}>
              <label>
                <input
                  type="checkbox"
                  required
                  checked={confirmed}
                  aria-disabled={pending}
                  onChange={(e) => {
                    if (!busy.current) setConfirmed(e.target.checked);
                  }}
                />
                已确认是清晰的单人语音，并同意向 Qwen 交付此参考录音
              </label>
              <label>
                交付理由
                <input
                  required
                  maxLength={500}
                  value={reason}
                  name="deliveryReason"
                  readOnly={pending}
                  onChange={(e) => setReason(e.target.value)}
                />
              </label>
              <button
                className="primary"
                aria-disabled={pending}
                aria-busy={pending}
                type="submit"
              >
                {pending ? "正在授权…" : "授权限时交付"}
              </button>
            </form>
          ) : (
            <div>
              <label>
                本次限时地址
                <textarea
                  className="reference-delivery-url"
                  readOnly
                  value={url}
                  spellCheck={false}
                />
              </label>
              <p>
                地址含访问凭据，仅本次显示。丢失后请撤销旧凭据，再重新授权。
              </p>
              <button
                className="text-button"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(url);
                    setNotice("限时地址已复制。");
                  } catch {
                    setError("无法自动复制，请选择地址文本复制。");
                  }
                }}
              >
                复制限时地址
              </button>
              <form className="reference-delivery-form" onSubmit={createVoice}>
                <label>
                  <input
                    name="voiceCreationConsent"
                    type="checkbox"
                    required
                    checked={costConfirmed}
                    aria-disabled={pending}
                    onChange={(e) => {
                      if (!busy.current) setCostConfirmed(e.target.checked);
                    }}
                  />
                  确认向 Qwen 创建音色，此操作可能产生费用。
                </label>
                <button
                  className="primary"
                  type="submit"
                  aria-disabled={pending}
                  aria-busy={pending}
                >
                  {pending ? "正在提交…" : "创建角色音色"}
                </button>
              </form>
              <Link className="text-button" to="/admin/voice-jobs">
                查看音色创建任务
              </Link>
            </div>
          )}
        </article>
      )}
      {!loaderData.items.length && <p>没有参考录音交付记录。</p>}
      <div className="admin-list">
        {loaderData.items.map((item) => (
          <article key={item.id} className="admin-card">
            <h2>
              {item.characterId} · 角色 v{item.characterRevision} · 声音 v
              {item.voiceRevision}
            </h2>
            <p>
              {item.assetId} · v{item.assetRevision} · 已读取 {item.readCount}
              /32 次
            </p>
            <p>
              {item.revoked ? "已撤销" : "有效至"}{" "}
              {!item.revoked && (
                <time dateTime={item.expiresAt}>
                  {new Intl.DateTimeFormat("zh-CN", {
                    timeZone: "Asia/Shanghai",
                    dateStyle: "medium",
                    timeStyle: "short",
                  }).format(new Date(item.expiresAt))}
                </time>
              )}
            </p>
            {!item.revoked && (
              <button
                className="text-button"
                aria-disabled={pending}
                onClick={() => {
                  if (busy.current) return;
                  setTarget(item);
                  setRevokeReason("");
                  setError("");
                  openDialog();
                }}
              >
                撤销交付凭据
              </button>
            )}
          </article>
        ))}
      </div>
      {loaderData.next && (
        <Link
          className="text-button"
          to={`/admin/voice-references?afterId=${loaderData.next}`}
        >
          下一页记录
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
          setError("");
        }}
      >
        <h2 id={dialogTitleId}>撤销参考录音交付</h2>
        <p>{target?.assetId}</p>
        <p>已经读取的音频无法收回；后续请求将被拒绝。</p>
        <form onSubmit={revoke}>
          <label>
            撤销理由
            <input
              required
              maxLength={500}
              readOnly={pending}
              value={revokeReason}
              name="revokeReason"
              onChange={(e) => setRevokeReason(e.target.value)}
            />
          </label>
          {error && <p role="alert">{error}</p>}
          <button
            className="primary"
            type="submit"
            aria-disabled={pending}
            aria-busy={pending}
          >
            {pending ? "正在撤销…" : "确认撤销"}
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
