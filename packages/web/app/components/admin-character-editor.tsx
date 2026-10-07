import { useEffect, useRef, useState, type FormEvent } from "react";
import { useRevalidator } from "react-router";
import type { AdminCharacterVoice } from "@brioche/contracts/AdminCharacterVoice";
import type { AdminCharacterRequest } from "@brioche/contracts/AdminCharacterRequest";
import type { AdminAssets } from "@brioche/contracts/AdminAssets";
import { adminWrite } from "../lib/admin.client";

export function CharacterEditor({
  initial,
}: {
  initial?: AdminCharacterVoice;
}) {
  const dialog = useRef<HTMLDialogElement>(null),
    form = useRef<HTMLFormElement>(null);
  const busy = useRef(false),
    write = useRef<AbortController | null>(null),
    read = useRef<AbortController | null>(null);
  const [pending, setPending] = useState(false),
    [loading, setLoading] = useState(false),
    [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [assets, setAssets] = useState<AdminAssets>({ items: [], next: null });
  const [query, setQuery] = useState("");
  const [avatar, setAvatar] = useState<{ id: string; revision: number } | null>(
    null,
  );
  const refresh = useRevalidator();
  useEffect(
    () => () => {
      write.current?.abort();
      read.current?.abort();
    },
    [],
  );
  async function load(q: string, next?: AdminAssets["next"]) {
    read.current?.abort();
    const controller = new AbortController();
    read.current = controller;
    setLoading(true);
    setError("");
    const params = new URLSearchParams({ q });
    if (next) {
      params.set("afterId", next.assetId);
      params.set("afterRevision", String(next.revision));
    }
    try {
      const response = await fetch(`/api/v1/operator/assets?${params}`, {
        cache: "no-store",
        signal: AbortSignal.any([
          controller.signal,
          AbortSignal.timeout(10000),
        ]),
      });
      if (!response.ok) throw Error("无法读取头像素材，请核对登录状态后重试。");
      const result: AdminAssets = await response.json();
      controller.signal.throwIfAborted();
      setAssets(result);
    } catch (e) {
      if (!controller.signal.aborted)
        setError(e instanceof Error ? e.message : "素材读取失败，请重试。");
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }
  function open() {
    form.current?.reset();
    setError("");
    setNotice("");
    setQuery("");
    setAvatar(
      initial
        ? { id: initial.character.avatarId, revision: initial.avatarRevision }
        : null,
    );
    dialog.current?.showModal();
    void load("");
  }
  async function save(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (busy.current || !avatar) return;
    const values = new FormData(e.currentTarget);
    const request: AdminCharacterRequest = {
      characterId: String(values.get("characterId")),
      expectedRevision: initial?.character.revision ?? 0,
      displayName: String(values.get("displayName")),
      avatarId: avatar.id,
      avatarRevision: avatar.revision,
      reason: String(values.get("reason")),
    };
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      const result = await adminWrite<AdminCharacterVoice>(
        "characters/revisions",
        request,
        controller.signal,
      );
      controller.signal.throwIfAborted();
      dialog.current?.close();
      setNotice(
        `角色 ${result.character.displayName} · 版本 ${result.character.revision} 已登记。`,
      );
      refresh.revalidate();
    } catch (e) {
      if (!controller.signal.aborted)
        setError(
          e instanceof Error ? e.message : "登记未确认，请刷新核对后重试。",
        );
    } finally {
      busy.current = false;
      if (!controller.signal.aborted) setPending(false);
    }
  }
  return (
    <>
      <button className="text-button" onClick={open}>
        {initial ? "修改姓名与头像" : "新建角色"}
      </button>
      {notice && <p role="status">{notice}</p>}
      <dialog
        ref={dialog}
        className="admin-dialog"
        aria-labelledby={
          initial
            ? `character-edit-${initial.character.characterId}`
            : "character-create"
        }
        onCancel={(e) => {
          if (busy.current) e.preventDefault();
        }}
        onClose={() => {
          read.current?.abort();
          setAssets({ items: [], next: null });
          setError("");
        }}
      >
        <form ref={form} className="asset-upload-form" onSubmit={save}>
          <h2
            id={
              initial
                ? `character-edit-${initial.character.characterId}`
                : "character-create"
            }
          >
            {initial ? "创建角色新版本" : "新建角色"}
          </h2>
          <label>
            角色编号
            <input
              name="characterId"
              defaultValue={initial?.character.characterId ?? ""}
              readOnly={!!initial}
              pattern="[a-z0-9][a-z0-9-]*"
              maxLength={100}
              required
            />
          </label>
          <label>
            名字
            <input
              name="displayName"
              defaultValue={initial?.character.displayName ?? ""}
              maxLength={100}
              required
            />
          </label>
          <p>新版本 {(initial?.character.revision ?? 0) + 1} · 法语</p>
          <div className="avatar-search">
            <label>
              搜索头像素材
              <input
                type="search"
                maxLength={200}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    if (!busy.current && !loading) void load(query);
                  }
                }}
              />
            </label>
            <button
              className="secondary"
              type="button"
              aria-disabled={loading || pending}
              onClick={() => {
                if (!busy.current && !loading) void load(query);
              }}
            >
              查找
            </button>
          </div>
          {loading && <p role="status">正在读取素材…</p>}
          <div className="avatar-picker" role="group" aria-label="选择头像">
            {assets.items
              .filter((i) => i.asset.width === i.asset.height)
              .map(({ asset }) => (
                <button
                  type="button"
                  key={`${asset.assetId}:${asset.revision}`}
                  aria-pressed={
                    avatar?.id === asset.assetId &&
                    avatar.revision === asset.revision
                  }
                  aria-disabled={pending}
                  onClick={() => {
                    if (!busy.current)
                      setAvatar({
                        id: asset.assetId,
                        revision: asset.revision,
                      });
                  }}
                >
                  <img
                    src={asset.url}
                    alt={asset.altZh}
                    width={64}
                    height={64}
                  />
                  <span>
                    {asset.assetId} v{asset.revision}
                  </span>
                </button>
              ))}
          </div>
          {!loading &&
            !assets.items.some((i) => i.asset.width === i.asset.height) && (
              <p>这一页没有方形头像素材。</p>
            )}
          {assets.next && (
            <button
              type="button"
              className="text-button"
              aria-disabled={loading || pending}
              onClick={() => {
                if (!busy.current && !loading) void load(query, assets.next);
              }}
            >
              下一页素材
            </button>
          )}
          {avatar && (
            <p>
              已选择 {avatar.id} · 版本 {avatar.revision}
            </p>
          )}
          <label>
            修改理由
            <textarea name="reason" maxLength={300} required />
          </label>
          <p>新角色版本需要单独配置声音档案；已发布课程继续使用原角色版本。</p>
          {error && <p role="alert">{error}</p>}
          <div className="admin-dialog-actions">
            <button
              type="button"
              className="secondary"
              aria-disabled={pending}
              onClick={() => {
                if (!busy.current) dialog.current?.close();
              }}
            >
              取消
            </button>
            <button
              type="submit"
              className="primary"
              disabled={!avatar}
              aria-disabled={pending}
              aria-busy={pending}
            >
              {pending ? "正在登记…" : "确认登记"}
            </button>
          </div>
        </form>
      </dialog>
    </>
  );
}
