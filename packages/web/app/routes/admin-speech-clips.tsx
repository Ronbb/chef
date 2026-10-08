import { useCommittedDialog } from "../components/committed-dialog";
import { data, Link, useRevalidator } from "react-router";
import { useId, useEffect, useRef, useState } from "react";
import type { AdminSpeechPlan } from "@brioche/contracts/AdminSpeechPlan";
import type { AdminSpeechClip } from "@brioche/contracts/AdminSpeechClip";
import type { AdminSpeechClips } from "@brioche/contracts/AdminSpeechClips";
import type { AdminSpeechClipRequest } from "@brioche/contracts/AdminSpeechClipRequest";
import type { AdminSpeechClipReview } from "@brioche/contracts/AdminSpeechClipReview";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite } from "../lib/admin.client";
import { RecordingPlayer } from "../lib/recording-playback";
import { useLearning } from "../components/learning";
import { speechTargetLocale } from "../lib/speech-authoring";
import type { Route } from "./+types/admin-speech-clips";
export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const id = new URL(request.url).searchParams.get("planId");
  if (!id || !/^[a-f0-9]{32}$/.test(id))
    throw new Response("配音计划无效。", { status: 400 });
  const plan = await getPrivate<AdminSpeechPlan>(
    request,
    `/api/v1/operator/speech-plans/${id}`,
  );
  const clips = await getPrivate<AdminSpeechClips>(
    request,
    `/api/v1/operator/speech-plans/${id}/clips`,
  );
  return data({ plan, clips }, { headers: headers() });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
const labels: Record<string, string> = {
  submitted: "正在生成",
  ready: "待审听",
  failed: "生成失败",
  unknown: "结果未确认",
};
async function waitClip(
  id: string,
  signal: AbortSignal,
): Promise<AdminSpeechClip> {
  const response = await fetch(`/api/v1/operator/speech-clips/${id}`, {
    cache: "no-store",
    signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]),
  });
  if (!response.ok) throw Error("无法核对生成结果，请刷新查看原任务。");
  return response.json();
}
function delay(signal: AbortSignal) {
  return new Promise<void>((resolve, reject) => {
    signal.throwIfAborted();
    const abort = () => {
      clearTimeout(timer);
      reject(signal.reason);
    };
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", abort);
      resolve();
    }, 2000);
    signal.addEventListener("abort", abort, { once: true });
  });
}
export default function SpeechClips({
  loaderData: { plan, clips },
}: Route.ComponentProps) {
  const dialogTitleId = useId();
  const refresh = useRevalidator(),
    { toast, stop } = useLearning();
  const dialog = useRef<HTMLDialogElement>(null),
    player = useRef<RecordingPlayer | null>(null);
  const openDialog = useCommittedDialog(dialog);
  const controller = useRef<AbortController | null>(null),
    busy = useRef(false);
  const attempt = useRef<AdminSpeechClipRequest | null>(null),
    reviewAttempt = useRef<{ id: string; body: AdminSpeechClipReview } | null>(
      null,
    );
  const [mode, setMode] = useState<
    | { kind: "generate"; keys: string[] }
    | { kind: "review"; clip: AdminSpeechClip }
    | null
  >(null);
  const [pending, setPending] = useState(false),
    [frozen, setFrozen] = useState(false),
    [reason, setReason] = useState("");
  const [consent, setConsent] = useState(false),
    [retryUnknown, setRetryUnknown] = useState(false),
    [accepted, setAccepted] = useState(true);
  const [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [active, setActive] = useState<string | null>(null);
  const [status, setStatus] = useState("idle"),
    [progress, setProgress] = useState(0);
  const byKey = new Map(clips.items.map((c) => [c.generationKey, c]));
  const targets = [
    ...new Map(plan.targets.map((t) => [t.generationKey, t])).values(),
  ];
  const missing = targets
    .filter((t) => {
      const c = byKey.get(t.generationKey);
      return (
        !c ||
        c.status === "failed" ||
        (c.status === "ready" && c.accepted === false)
      );
    })
    .map((t) => t.generationKey);
  useEffect(() => {
    player.current = new RecordingPlayer();
    const release = () => {
      player.current?.stop();
      controller.current?.abort();
    };
    window.addEventListener("pagehide", release);
    return () => {
      release();
      window.removeEventListener("pagehide", release);
    };
  }, []);
  const scope = useRef(plan.id);
  useEffect(() => {
    if (scope.current === plan.id) return;
    scope.current = plan.id;
    controller.current?.abort();
    player.current?.stop();
    busy.current = false;
    attempt.current = null;
    reviewAttempt.current = null;
    setPending(false);
    setFrozen(false);
    setActive(null);
    setMode(null);
    dialog.current?.close();
    setError("");
    setNotice("");
  }, [plan.id]);
  const waiting = clips.items.some((c) => c.status === "submitted");
  useEffect(() => {
    if (!waiting || pending || refresh.state !== "idle") return;
    const timer = setTimeout(() => refresh.revalidate(), 2000);
    return () => clearTimeout(timer);
  }, [waiting, pending, refresh.state, refresh]);
  function open(next: NonNullable<typeof mode>) {
    if (busy.current || attempt.current || reviewAttempt.current) return;
    setMode(next);
    setReason("");
    setConsent(false);
    setRetryUnknown(false);
    setAccepted(true);
    setError("");
    setFrozen(false);
    openDialog();
  }
  function close() {
    if (busy.current) return;
    attempt.current = null;
    reviewAttempt.current = null;
    setFrozen(false);
    setMode(null);
    dialog.current?.close();
    refresh.revalidate();
  }
  function listen(clip: AdminSpeechClip) {
    if (active === clip.id && player.current?.isActive) {
      if (status === "paused") player.current.resume();
      else {
        player.current.pause();
        setStatus("paused");
      }
      return;
    }
    stop();
    player.current?.stop();
    setActive(clip.id);
    setProgress(0);
    player.current?.play(
      {
        url: `/api/v1/operator/speech-clips/${clip.id}/file`,
        startMs: 0,
        endMs: clip.durationMs!,
        cues: [],
      },
      1,
      {
        status: setStatus,
        progress: setProgress,
        end: () => {
          setActive(null);
          setStatus("idle");
        },
        error: () => {
          setActive(null);
          setStatus("idle");
          toast("音频无法播放，请核对文件与登录状态。");
        },
      },
    );
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (
      busy.current ||
      !mode ||
      !consent ||
      !reason.trim() ||
      new TextEncoder().encode(reason).length > 1000
    )
      return;
    if (
      mode.kind === "generate" &&
      !attempt.current &&
      mode.keys.some((key) => byKey.get(key)?.status === "unknown") &&
      !retryUnknown
    ) {
      setError("请先核对原任务，并确认再次合成可能重复收费。");
      return;
    }
    busy.current = true;
    setPending(true);
    setFrozen(true);
    setError("");
    const abort = new AbortController();
    controller.current = abort;
    try {
      if (mode.kind === "review") {
        reviewAttempt.current ??= {
          id: mode.clip.id,
          body: { heard: true, accepted, reason },
        };
        await adminWrite(
          `speech-clips/${reviewAttempt.current.id}/review`,
          reviewAttempt.current.body,
          abort.signal,
        );
        reviewAttempt.current = null;
        setNotice("审听决定已保存，尚未登记或发布课程录音。");
      } else {
        const keys = attempt.current
          ? [attempt.current.generationKey]
          : mode.keys;
        for (const key of keys) {
          abort.signal.throwIfAborted();
          const previous = byKey.get(key);
          attempt.current ??= {
            id: Array.from(crypto.getRandomValues(new Uint8Array(16)), (n) =>
              n.toString(16).padStart(2, "0"),
            ).join(""),
            planId: plan.id!,
            generationKey: key,
            expectedPlanHash: plan.planHash,
            expectedPreviousId: previous?.id ?? null,
            costConfirmed: consent,
            retryUnknownConfirmed: retryUnknown,
            reason,
          };
          let clip = await adminWrite<AdminSpeechClip>(
            "speech-clips",
            attempt.current,
            abort.signal,
          );
          attempt.current = null;
          const deadline = Date.now() + 310000;
          while (clip.status === "submitted" && Date.now() < deadline) {
            await delay(abort.signal);
            clip = await waitClip(clip.id, abort.signal);
          }
          if (clip.status !== "ready")
            throw Error(
              "这个片段的结果需要核对，已停止后续生成。请查看原任务。",
            );
        }
        setNotice("本次片段已生成，接下来逐个试听；尚未登记或发布课程录音。");
      }
      setFrozen(false);
      setMode(null);
      dialog.current?.close();
      refresh.revalidate();
    } catch (e) {
      if (!abort.signal.aborted) {
        setError(e instanceof Error ? e.message : "结果未确认，请核对原请求。");
        refresh.revalidate();
      }
    } finally {
      if (controller.current === abort) {
        busy.current = false;
        setPending(false);
      }
    }
  }
  return (
    <section className="settings-page speech-plans-page page-arrive">
      <div className="page-title">
        <h1>课程配音</h1>
        <Link
          className="text-button"
          to={`/admin/speech-plans?lessonId=${plan.lessonId}&revision=${plan.lessonRevision}&planId=${plan.id}`}
        >
          返回固定计划
        </Link>
      </div>
      <p>
        {plan.lessonId} · v{plan.lessonRevision} · {targets.length} 个独立片段
      </p>
      <Link
        className="text-button"
        to={`/admin/speech-alignments?planId=${plan.id}`}
      >
        逐词时间轴核对
      </Link>
      <p>
        生成使用本计划固定的角色声音和情绪。已有有效音频会复用；未确认的任务需要单独核对。
      </p>
      {!clips.configured && (
        <p role="status">提供方尚未配置，可查看和试听已有结果。</p>
      )}
      <button
        className="primary"
        disabled={
          !clips.configured || missing.length === 0 || pending || frozen
        }
        onClick={() => open({ kind: "generate", keys: missing })}
      >
        生成未完成片段（{missing.length}）
      </button>
      {targets.length > 0 &&
      targets.every((t) => {
        const clip = byKey.get(t.generationKey);
        return clip?.status === "ready" && clip.accepted === true;
      }) ? (
        <a
          className="text-button"
          href={`/api/v1/operator/speech-plans/${plan.id}/export`}
          download
        >
          下载已审听音频与配音清单
        </a>
      ) : (
        <p className="muted">
          全部片段审听通过后，可以下载音频用于逐词对齐和正式录音登记。
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <div className="speech-clip-list">
        {targets.map((t) => {
          const c = byKey.get(t.generationKey);
          return (
            <article className="lesson-note" key={t.generationKey}>
              <p className="muted">
                {t.voice.characterId} · 声音 v{t.voice.voiceRevision}
              </p>
              <p className="fr" lang={speechTargetLocale(plan, t)}>
                {t.text}
              </p>
              <p>{t.emotion}</p>
              <p>
                {c
                  ? c.accepted === true
                    ? "审听通过"
                    : c.accepted === false
                      ? "审听退回"
                      : (labels[c.status] ?? c.status)
                  : "尚未生成"}
                {c?.reusedFrom ? " · 复用已生成音频" : ""}
              </p>
              {c?.status === "ready" && (
                <>
                  <button className="text-button" onClick={() => listen(c)}>
                    {active === c.id && status !== "paused"
                      ? "暂停试听"
                      : "试听"}
                  </button>
                  {active === c.id && (
                    <div className="recording-preview-track">
                      <span style={{ width: `${progress * 100}%` }} />
                    </div>
                  )}
                  {c.accepted === null && (
                    <button
                      className="text-button"
                      disabled={pending || frozen}
                      onClick={() => open({ kind: "review", clip: c })}
                    >
                      记录审听决定
                    </button>
                  )}
                </>
              )}
              {(!c ||
                c.status === "failed" ||
                c.status === "unknown" ||
                c.accepted === false) && (
                <button
                  className="text-button"
                  disabled={!clips.configured || pending || frozen}
                  onClick={() =>
                    open({ kind: "generate", keys: [t.generationKey] })
                  }
                >
                  {c?.status === "unknown" ? "核对后重新生成" : "生成这个片段"}
                </button>
              )}
            </article>
          );
        })}
      </div>
      <dialog
        aria-labelledby={dialogTitleId}
        ref={dialog}
        className="admin-dialog"
        onCancel={(e) => {
          e.preventDefault();
          close();
        }}
      >
        <form onSubmit={submit}>
          <h2 id={dialogTitleId}>
            {mode?.kind === "review" ? "记录审听" : "确认生成"}
          </h2>
          {mode?.kind === "generate" && (
            <p>
              最多提交 {mode.keys.length}{" "}
              个片段。合成可能产生费用，复用缓存不发起合成。关闭页面会停止后续提交，已发送的任务仍可能完成。
            </p>
          )}
          <fieldset disabled={frozen || pending}>
            <label>
              操作理由
              <textarea
                required
                value={reason}
                onChange={(e) => setReason(e.target.value)}
              />
            </label>
            {mode?.kind === "review" && (
              <label>
                <input
                  type="checkbox"
                  checked={accepted}
                  onChange={(e) => setAccepted(e.target.checked)}
                />
                审听通过
              </label>
            )}
            <label>
              <input
                type="checkbox"
                checked={consent}
                onChange={(e) => setConsent(e.target.checked)}
              />
              {mode?.kind === "review"
                ? "我已实际试听这个片段"
                : "我确认本次合成可能收费"}
            </label>
            {mode?.kind === "generate" &&
              mode.keys.some((k) => byKey.get(k)?.status === "unknown") && (
                <label>
                  <input
                    type="checkbox"
                    checked={retryUnknown}
                    onChange={(e) => setRetryUnknown(e.target.checked)}
                  />
                  已核对原任务，接受再次合成可能重复收费
                </label>
              )}
          </fieldset>
          {error && <p role="alert">{error}</p>}
          <button
            className="primary"
            aria-disabled={pending || !consent}
            type="submit"
          >
            {pending
              ? "正在处理"
              : attempt.current || reviewAttempt.current
                ? "核对同一请求"
                : "确认"}
          </button>
          {pending ? (
            <button
              className="text-button"
              type="button"
              onClick={() => {
                controller.current?.abort();
                setError(
                  "后续提交已停止，请核对原任务。已发出的请求没有撤销。",
                );
              }}
            >
              停止后续提交
            </button>
          ) : (
            <button className="text-button" type="button" onClick={close}>
              关闭并核对任务
            </button>
          )}
        </form>
      </dialog>
    </section>
  );
}
