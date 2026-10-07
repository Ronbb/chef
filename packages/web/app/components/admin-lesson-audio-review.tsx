import { useId, useEffect, useRef, useState } from "react";
import { useBeforeUnload, useBlocker } from "react-router";
import type { AdminLessonAudioReview } from "@brioche/contracts/AdminLessonAudioReview";
import type { AdminLessonAudioStatus } from "@brioche/contracts/AdminLessonAudioStatus";
import { AdminWriteError, adminWrite } from "../lib/admin.client";

export function LessonAudioReview({
  id,
  revision,
  initial,
}: {
  id: string;
  revision: number;
  initial: AdminLessonAudioStatus;
}) {
  const dialogTitleId = useId();
  const [status, setStatus] = useState(initial),
    [reason, setReason] = useState("");
  const [heard, setHeard] = useState(false),
    [pending, setPending] = useState(false),
    [frozen, setFrozen] = useState(false);
  const [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const attempt = useRef<AdminLessonAudioReview | null>(null),
    controller = useRef<AbortController | null>(null),
    busy = useRef(false),
    mounted = useRef(true);
  const heading = useRef<HTMLHeadingElement>(null),
    leave = useRef<HTMLDialogElement>(null);
  const active = pending || frozen,
    blocker = useBlocker(active);
  useBeforeUnload((event) => {
    if (active) {
      event.preventDefault();
      event.returnValue = "";
    }
  });
  useEffect(() => {
    mounted.current = true;
    const cancel = () => controller.current?.abort();
    window.addEventListener("pagehide", cancel);
    return () => {
      mounted.current = false;
      cancel();
      window.removeEventListener("pagehide", cancel);
    };
  }, []);
  useEffect(() => {
    if (!busy.current && !attempt.current) setStatus(initial);
  }, [initial]);
  useEffect(() => {
    if (blocker.state === "blocked" && active) leave.current?.showModal();
    else {
      leave.current?.close();
      if (blocker.state === "blocked") blocker.reset();
    }
  }, [blocker, active]);
  function stay() {
    if (blocker.state === "blocked") blocker.reset();
    leave.current?.close();
    heading.current?.focus();
  }
  async function submit(accepted: boolean) {
    if (
      busy.current ||
      status.published ||
      (!attempt.current && (!reason.trim() || (accepted && !heard)))
    )
      return;
    if (!attempt.current)
      attempt.current = {
        expectedLessonHash: status.lessonHash,
        version: status.version,
        accepted,
        heard,
        reason,
      };
    busy.current = true;
    setPending(true);
    setFrozen(true);
    setError("");
    setNotice("");
    const abort = new AbortController();
    controller.current = abort;
    try {
      const saved = await adminWrite<AdminLessonAudioStatus>(
        `lessons/${encodeURIComponent(id)}/revisions/${revision}/audio-review`,
        attempt.current,
        abort.signal,
      );
      abort.signal.throwIfAborted();
      attempt.current = null;
      setFrozen(false);
      setStatus(saved);
      setHeard(false);
      setReason("");
      setNotice(
        saved.accepted
          ? "整课试听已通过，可以回到后台审批课程。"
          : "退回意见已保存，此版本暂不能发布。",
      );
    } catch (e) {
      if (
        e instanceof AdminWriteError &&
        [400, 409, 410, 413, 422].includes(e.status)
      ) {
        attempt.current = null;
        if (mounted.current) setFrozen(false);
      }
      if (mounted.current && !abort.signal.aborted)
        setError(
          e instanceof Error ? e.message : "审核未确认，请核对同一请求。",
        );
    } finally {
      busy.current = false;
      if (mounted.current) setPending(false);
    }
  }
  if (!status.required) return null;
  return (
    <section className="admin-card lesson-audio-review">
      <h2 ref={heading} tabIndex={-1}>
        整课试听
      </h2>
      <p>
        {status.directAuthorized
          ? "已授权直接发布，未声明人工试听"
          : status.accepted
            ? "已通过最终试听"
            : "尚未通过最终试听"}{" "}
        · 第 {revision} 版
      </p>
      {status.reason && (
        <p className="admin-note">
          {status.reason} · {status.actor}
        </p>
      )}
      {status.published ? (
        <p>此版本已发布，更改录音需要新的课程版本。</p>
      ) : (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void submit(true);
          }}
        >
          <fieldset disabled={pending || frozen}>
            <label className="admin-check">
              <input
                type="checkbox"
                checked={heard}
                onChange={(event) => setHeard(event.target.checked)}
              />
              我已完整试听正文和例句，核对发音、情绪、拼接与点读时间。
            </label>
            <label>
              试听意见
              <textarea
                value={reason}
                maxLength={2000}
                required
                onChange={(event) => setReason(event.target.value)}
              />
            </label>
          </fieldset>
          <button
            className="primary"
            type="submit"
            disabled={!frozen && (!heard || !reason.trim())}
            aria-disabled={pending}
            aria-busy={pending}
          >
            {pending ? "保存中" : frozen ? "核对同一审核请求" : "通过整课试听"}
          </button>
          {!frozen && (
            <button
              className="text-button"
              type="button"
              disabled={!reason.trim()}
              aria-disabled={pending}
              onClick={() => void submit(false)}
            >
              退回录音
            </button>
          )}
        </form>
      )}
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <dialog
        aria-labelledby={dialogTitleId}
        ref={leave}
        className="choice-dialog"
        onCancel={(event) => {
          event.preventDefault();
          stay();
        }}
      >
        <h2 id={dialogTitleId}>审核结果尚未确认</h2>
        <p>
          离开会清除本页的重试参数。返回后请先核对审核记录；取消请求不会回滚服务器已保存的决定。
        </p>
        <button className="primary" type="button" onClick={stay}>
          留在当前页
        </button>
        <button
          className="text-button"
          type="button"
          onClick={() => {
            if (blocker.state === "blocked") blocker.proceed();
          }}
        >
          继续离开
        </button>
      </dialog>
    </section>
  );
}
