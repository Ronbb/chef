import { useEffect, useRef, useState } from "react";
import product from "@chef/product";
import { Link } from "react-router";
import type { AdminAlignment } from "@brioche/contracts/AdminAlignment";
import type { AdminSpeechPackageRequest } from "@brioche/contracts/AdminSpeechPackageRequest";
import type { AdminSpeechPackageImport } from "@brioche/contracts/AdminSpeechPackageImport";
import type { AdminSpeechPackageResult } from "@brioche/contracts/AdminSpeechPackageResult";
import type { AdminSpeechPackageResults } from "@brioche/contracts/AdminSpeechPackageResults";
import { adminArchive, adminWrite, AdminWriteError } from "../lib/admin.client";

export function SpeechPackage({
  alignment,
  lessonRevision,
  disabled,
  onPending,
}: {
  alignment: AdminAlignment;
  lessonRevision: number;
  disabled: boolean;
  onPending: (value: boolean) => void;
}) {
  const [revision, setRevision] = useState(String(lessonRevision + 1));
  const [gap, setGap] = useState("250");
  const [source, setSource] = useState("Qwen 法语语音合成（固定角色声音版本）");
  const [license, setLicense] = useState("");
  const [creator, setCreator] = useState("");
  const [credit, setCredit] = useState(`AI 合成语音 · ${product.name}`);
  const [reason, setReason] = useState("");
  const [rights, setRights] = useState(false);
  const [pending, setPending] = useState(false);
  const [frozen, setFrozen] = useState(false);
  const [result, setResult] = useState<AdminSpeechPackageResult | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const busy = useRef(false);
  const request = useRef<AbortController | null>(null);
  const download = useRef<string | null>(null);
  const attempt = useRef<AdminSpeechPackageImport | null>(null);
  const mounted = useRef(true);
  const ready =
    alignment.clips.length > 0 &&
    alignment.clips.every((clip) => clip.accepted === true);
  useEffect(() => {
    mounted.current = true;
    const cancel = () => request.current?.abort();
    window.addEventListener("pagehide", cancel);
    return () => {
      mounted.current = false;
      window.removeEventListener("pagehide", cancel);
      request.current?.abort();
      if (download.current) URL.revokeObjectURL(download.current);
    };
  }, []);
  async function assemble(register = false) {
    if (
      busy.current ||
      (disabled && !frozen) ||
      !ready ||
      (!register && frozen)
    )
      return;
    setError("");
    setNotice("");
    const version = Number(revision),
      gapMs = Number(gap);
    if (
      !Number.isInteger(version) ||
      version <= lessonRevision ||
      version > 2147483647 ||
      !Number.isInteger(gapMs) ||
      gapMs < 0 ||
      gapMs > 1000 ||
      !rights ||
      [source, license, creator, credit, reason].some((value) => !value.trim())
    ) {
      setError("请核对新版本、停顿、来源授权及填写的信息。");
      return;
    }
    const body: AdminSpeechPackageRequest = {
      expectedReportHash: alignment.reportHash,
      lessonRevision: version,
      gapMs,
      rightsConfirmed: rights,
      source,
      license,
      creator,
      creditZh: credit,
      reason,
    };
    busy.current = true;
    setPending(true);
    onPending(true);
    const controller = new AbortController();
    request.current = controller;
    let unresolved = false;
    try {
      if (register) {
        if (!attempt.current)
          attempt.current = {
            id: Array.from(crypto.getRandomValues(new Uint8Array(16)), (n) =>
              n.toString(16).padStart(2, "0"),
            ).join(""),
            package: body,
          };
        unresolved = true;
        setFrozen(true);
        const saved = await adminWrite<AdminSpeechPackageResult>(
          `speech-alignments/${alignment.id}/package/import`,
          attempt.current,
          controller.signal,
        );
        controller.signal.throwIfAborted();
        attempt.current = null;
        unresolved = false;
        setFrozen(false);
        setResult(saved);
        setRefresh((value) => value + 1);
        setNotice(
          `已登记录音并导入课程 v${saved.revision} 草稿。请预览完整录音，完成最终试听后再审批发布。`,
        );
        return;
      }
      const blob = await adminArchive(
        `speech-alignments/${alignment.id}/package`,
        body,
        controller.signal,
      );
      controller.signal.throwIfAborted();
      if (download.current) URL.revokeObjectURL(download.current);
      const url = URL.createObjectURL(blob);
      download.current = url;
      const link = document.createElement("a");
      link.href = url;
      link.download = `speech-package-${alignment.id}-v${version}.tar`;
      document.body.append(link);
      link.click();
      link.remove();
      setNotice(
        "录音课包已下载。登记录音、导入新草稿并完成最终试听后，再审批发布。",
      );
    } catch (error) {
      if (
        register &&
        error instanceof AdminWriteError &&
        [400, 409, 413, 422].includes(error.status)
      ) {
        attempt.current = null;
        unresolved = false;
        setFrozen(false);
      }
      if (!controller.signal.aborted)
        setError(
          error instanceof Error ? error.message : "组装未完成，请重新核对。",
        );
    } finally {
      busy.current = false;
      if (mounted.current) {
        setPending(false);
        onPending(unresolved);
      }
    }
  }
  return (
    <form
      className="admin-editor"
      onSubmit={(event) => {
        event.preventDefault();
        void assemble(true);
      }}
    >
      <h2>组装录音课包</h2>
      <p className="muted">
        登记录音并导入新课程草稿，保留完整来源记录；预览试听后再审批发布。
      </p>
      {!ready && (
        <p className="muted">全部片段通过审听与时间轴核对后，可以组装。</p>
      )}
      <fieldset disabled={disabled || pending || frozen || !ready}>
        <label>
          新课程版本
          <input
            type="number"
            min={lessonRevision + 1}
            max={2147483647}
            step="1"
            required
            value={revision}
            onChange={(e) => setRevision(e.target.value)}
          />
        </label>
        <label>
          句间停顿（毫秒）
          <input
            type="number"
            min="0"
            max="1000"
            step="1"
            required
            value={gap}
            onChange={(e) => setGap(e.target.value)}
          />
        </label>
        <label>
          录音来源
          <input
            required
            maxLength={2000}
            value={source}
            onChange={(e) => setSource(e.target.value)}
          />
        </label>
        <label>
          使用授权依据
          <textarea
            required
            maxLength={2000}
            value={license}
            onChange={(e) => setLicense(e.target.value)}
          />
        </label>
        <label>
          创作或授权主体
          <input
            required
            maxLength={2000}
            value={creator}
            onChange={(e) => setCreator(e.target.value)}
          />
        </label>
        <label>
          公开署名
          <input
            required
            maxLength={500}
            value={credit}
            onChange={(e) => setCredit(e.target.value)}
          />
        </label>
        <label>
          组装说明
          <textarea
            required
            maxLength={2000}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={rights}
            onChange={(e) => setRights(e.target.checked)}
          />
          我确认录音及角色声音可按以上授权使用。
        </label>
      </fieldset>
      <button
        className="primary"
        disabled={!ready || (disabled && !pending && !frozen)}
        aria-disabled={pending}
        aria-busy={pending}
      >
        {pending
          ? "正在组装"
          : frozen
            ? "核对同一课包导入请求"
            : "登记录音并导入草稿"}
      </button>
      <button
        type="button"
        className="text-button"
        disabled={disabled || pending || frozen || !ready}
        onClick={() => void assemble(false)}
      >
        下载录音课包
      </button>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      {result && (
        <Link
          className="text-button"
          to={`/author-preview?lessonId=${encodeURIComponent(result.lessonId)}&revision=${result.revision}`}
        >
          预览新草稿
        </Link>
      )}
      <PackageReceipts alignmentId={alignment.id} refresh={refresh} />
    </form>
  );
}

function PackageReceipts({
  alignmentId,
  refresh,
}: {
  alignmentId: string;
  refresh: number;
}) {
  const [page, setPage] = useState<AdminSpeechPackageResults | null>(null);
  const [after, setAfter] = useState("");
  const [retry, setRetry] = useState(0);
  const [error, setError] = useState("");
  useEffect(() => {
    const controller = new AbortController();
    setPage(null);
    setError("");
    void (async () => {
      try {
        const response = await fetch(
          `/api/v1/operator/speech-alignments/${alignmentId}/packages${after ? `?after=${after}` : ""}`,
          {
            cache: "no-store",
            signal: AbortSignal.any([
              controller.signal,
              AbortSignal.timeout(10000),
            ]),
          },
        );
        if (!response.ok) throw Error("导入记录读取失败，请重新核对。");
        const incoming = (await response.json()) as AdminSpeechPackageResults;
        controller.signal.throwIfAborted();
        setPage(incoming);
      } catch (error) {
        if (!controller.signal.aborted)
          setError(
            error instanceof Error ? error.message : "导入记录读取失败。",
          );
      }
    })();
    return () => controller.abort();
  }, [alignmentId, after, refresh, retry]);
  return (
    <div className="admin-list">
      <h3>已导入的录音草稿</h3>
      {error && (
        <>
          <p role="alert">{error}</p>
          <button
            type="button"
            className="text-button"
            onClick={() => setRetry((value) => value + 1)}
          >
            重新读取导入记录
          </button>
        </>
      )}
      {page?.items.map((item) => (
        <Link
          className="settings-row"
          key={item.id}
          to={`/author-preview?lessonId=${encodeURIComponent(item.lessonId)}&revision=${item.revision}`}
        >
          <span>
            {item.lessonId} v{item.revision}
          </span>
          <span className="muted">{item.recordingCount} 条录音</span>
        </Link>
      ))}
      {page && page.items.length === 0 && (
        <p className="muted">暂无课包导入记录。</p>
      )}
      {after && (
        <button
          type="button"
          className="text-button"
          onClick={() => setAfter("")}
        >
          回到首批记录
        </button>
      )}
      {page?.next && (
        <button
          type="button"
          className="text-button"
          onClick={() => setAfter(page.next!)}
        >
          后续导入记录
        </button>
      )}
    </div>
  );
}
