import { useEffect, useRef, useState, type FormEvent } from "react";
import { useRevalidator } from "react-router";
import type { AdminAssetUpload } from "@brioche/contracts/AdminAssetUpload";
import type { AdminAssetCursor } from "@brioche/contracts/AdminAssetCursor";
import { adminWrite } from "../lib/admin.client";

export function AssetUpload() {
  const dialog = useRef<HTMLDialogElement>(null),
    form = useRef<HTMLFormElement>(null);
  const write = useRef<AbortController | null>(null),
    busy = useRef(false);
  const [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const refresh = useRevalidator();
  useEffect(() => () => write.current?.abort(), []);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy.current) return;
    const values = new FormData(event.currentTarget),
      file = values.get("file");
    if (!(file instanceof File) || !file.size || file.size > 32 * 1024 * 1024) {
      setError("请选择不超过32 MB的图片。");
      return;
    }
    const ext = file.name.split(".").pop()?.toLowerCase();
    const mime: Record<string, string> = {
      svg: "image/svg+xml",
      png: "image/png",
      jpg: "image/jpeg",
      jpeg: "image/jpeg",
      webp: "image/webp",
    };
    const mimeType = ext ? mime[ext] : undefined;
    if (!mimeType) {
      setError("请选择 SVG、PNG、JPEG 或 WebP 图片。");
      return;
    }
    const request: AdminAssetUpload = {
      assetId: String(values.get("assetId")),
      revision: Number(values.get("revision")),
      mimeType,
      altZh: String(values.get("altZh")),
      creditZh: String(values.get("creditZh")),
      source: String(values.get("source")),
      license: String(values.get("license")),
      creator: String(values.get("creator")),
      rightsConfirmed: values.get("rightsConfirmed") === "on",
      reason: String(values.get("reason")),
    };
    const payload = new FormData();
    payload.set("document", JSON.stringify(request));
    payload.set("file", file);
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      const result = await adminWrite<AdminAssetCursor>(
        "assets",
        payload,
        controller.signal,
      );
      controller.signal.throwIfAborted();
      dialog.current?.close();
      setNotice(`素材 ${result.assetId} · 版本 ${result.revision} 已登记。`);
      refresh.revalidate();
    } catch (e) {
      if (!controller.signal.aborted)
        setError(
          e instanceof Error &&
            !["AbortError", "TimeoutError", "TypeError"].includes(e.name)
            ? e.message
            : "上传未确认，请刷新核对素材版本后重试。",
        );
    } finally {
      busy.current = false;
      if (!controller.signal.aborted) setPending(false);
    }
  }
  return (
    <>
      <button
        className="primary"
        onClick={() => {
          form.current?.reset();
          setError("");
          setNotice("");
          dialog.current?.showModal();
        }}
      >
        上传图片
      </button>
      {notice && <p role="status">{notice}</p>}
      <dialog
        ref={dialog}
        className="admin-dialog"
        aria-labelledby="asset-upload-title"
        onCancel={(e) => {
          if (busy.current) e.preventDefault();
        }}
        onClose={() => {
          form.current?.reset();
          setError("");
        }}
      >
        <form ref={form} className="asset-upload-form" onSubmit={submit}>
          <h2 id="asset-upload-title">登记图片素材</h2>
          <label>
            图片文件
            <input
              name="file"
              type="file"
              accept=".svg,.png,.jpg,.jpeg,.webp"
              required
            />
          </label>
          <label>
            素材编号
            <input
              name="assetId"
              pattern="[a-z0-9][a-z0-9-]*"
              maxLength={100}
              required
            />
          </label>
          <label>
            版本
            <input
              name="revision"
              type="number"
              min={1}
              max={2147483647}
              step={1}
              defaultValue={1}
              required
            />
          </label>
          <label>
            图片说明
            <input name="altZh" maxLength={300} required />
          </label>
          <label>
            来源
            <input name="source" maxLength={500} required />
          </label>
          <label>
            许可或授权依据
            <input name="license" maxLength={500} required />
          </label>
          <label>
            创作者
            <input name="creator" maxLength={300} required />
          </label>
          <label>
            展示署名
            <input name="creditZh" maxLength={300} required />
          </label>
          <label className="admin-upload-consent">
            <input name="rightsConfirmed" type="checkbox" required />
            已确认在本项目中使用这张图片的权利
          </label>
          <label>
            登记理由
            <textarea name="reason" maxLength={300} required />
          </label>
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
              className="primary"
              type="submit"
              aria-disabled={pending}
              aria-busy={pending}
            >
              {pending ? "正在检查与登记…" : "确认登记"}
            </button>
          </div>
        </form>
      </dialog>
    </>
  );
}
