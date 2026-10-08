import {
  data,
  Link,
  useBlocker,
  useBeforeUnload,
  useRevalidator,
} from "react-router";
import { useEffect, useRef, useState } from "react";
import type { AdminAlignment } from "@brioche/contracts/AdminAlignment";
import type { AdminAlignments } from "@brioche/contracts/AdminAlignments";
import type { AdminAlignmentImport } from "@brioche/contracts/AdminAlignmentImport";
import type { AdminAlignmentReview } from "@brioche/contracts/AdminAlignmentReview";
import type { AdminAlignmentClip } from "@brioche/contracts/AdminAlignmentClip";
import type { AdminSpeechPlan } from "@brioche/contracts/AdminSpeechPlan";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite, AdminWriteError } from "../lib/admin.client";
import { useLearning } from "../components/learning";
import { RecordingPlayer } from "../lib/recording-playback";
import { SpeechPackage } from "../components/admin-speech-package";
import { speechTargetLocale } from "../lib/speech-authoring";
import type { Route } from "./+types/admin-speech-alignments";

export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const params = new URL(request.url).searchParams;
  const planId = params.get("planId"),
    id = params.get("alignmentId"),
    after = params.get("after");
  if (
    !planId ||
    !/^[a-f0-9]{32}$/.test(planId) ||
    (id && !/^[a-f0-9]{32}$/.test(id)) ||
    (after && !/^[a-f0-9]{32}$/.test(after))
  )
    throw new Response("时间轴参数无效。", { status: 400 });
  const plan = await getPrivate<AdminSpeechPlan>(
    request,
    `/api/v1/operator/speech-plans/${planId}`,
  );
  const entries = await getPrivate<AdminAlignments>(
    request,
    `/api/v1/operator/speech-plans/${planId}/alignments${after ? `?after=${after}` : ""}`,
  );
  const alignment = id
    ? await getPrivate<AdminAlignment>(
        request,
        `/api/v1/operator/speech-alignments/${id}`,
      )
    : null;
  if (alignment && alignment.planId !== planId)
    throw new Response("时间轴不属于当前计划。", { status: 409 });
  return data({ plan, entries, alignment }, { headers: headers() });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
function attemptId() {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (n) =>
    n.toString(16).padStart(2, "0"),
  ).join("");
}
export default function Alignments({ loaderData }: Route.ComponentProps) {
  const { plan, entries } = loaderData;
  const revalidator = useRevalidator();
  const [alignment, setAlignment] = useState(loaderData.alignment);
  const [file, setFile] = useState<File | null>(null),
    [reason, setReason] = useState("");
  const [pending, setPending] = useState(false),
    [frozen, setFrozen] = useState(false),
    [error, setError] = useState("");
  const [index, setIndex] = useState(0),
    [reviewLocked, setReviewLocked] = useState(false);
  const [packagePending, setPackagePending] = useState(false);
  const hasRequest = pending || frozen || reviewLocked || packagePending;
  const blocker = useBlocker(hasRequest);
  useBeforeUnload((event) => {
    if (hasRequest) {
      event.preventDefault();
      event.returnValue = "";
    }
  });
  const attempt = useRef<AdminAlignmentImport | null>(null),
    controller = useRef<AbortController | null>(null),
    busy = useRef(false);
  const selection = useRef({ planId: plan.id, id: loaderData.alignment?.id });
  useEffect(() => {
    const incoming = loaderData.alignment;
    if (selection.current.planId !== plan.id || incoming) {
      if (
        selection.current.planId !== plan.id ||
        selection.current.id !== incoming?.id
      )
        setIndex(0);
      selection.current = { planId: plan.id, id: incoming?.id };
      setAlignment(incoming);
    }
  }, [loaderData.alignment, plan.id]);
  useEffect(() => {
    const cancel = () => controller.current?.abort();
    window.addEventListener("pagehide", cancel);
    return () => {
      cancel();
      window.removeEventListener("pagehide", cancel);
    };
  }, []);
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (busy.current || reviewLocked) return;
    if (
      !attempt.current &&
      (!file || file.size > 4 * 1024 * 1024 || !reason.trim())
    ) {
      setError("请选择不超过4 MB的对齐结果并填写导入理由。");
      return;
    }
    busy.current = true;
    setPending(true);
    setError("");
    const abort = new AbortController();
    controller.current = abort;
    try {
      if (!attempt.current) {
        const reportJson = await file!.text();
        abort.signal.throwIfAborted();
        attempt.current = {
          id: attemptId(),
          planId: plan.id!,
          expectedPlanHash: plan.planHash,
          reportJson,
          reason,
        };
      }
      setFrozen(true);
      const result = await adminWrite<AdminAlignment>(
        "speech-alignments",
        attempt.current,
        abort.signal,
      );
      attempt.current = null;
      setFrozen(false);
      selection.current = { planId: plan.id, id: result.id };
      setAlignment(result);
      setIndex(0);
      void revalidator.revalidate();
    } catch (e) {
      if (!abort.signal.aborted) {
        if (
          e instanceof AdminWriteError &&
          [400, 413, 422].includes(e.status)
        ) {
          attempt.current = null;
          setFrozen(false);
        }
        setError(
          e instanceof Error ? e.message : "导入未确认，请核对同一请求。",
        );
      }
    } finally {
      busy.current = false;
      if (!abort.signal.aborted) setPending(false);
    }
  }
  return (
    <section className="settings-page page-arrive">
      {blocker.state === "blocked" && (
        <div className="settings-group" role="alert">
          <p>当前请求尚未确认，结果可能已经保存。离开后请从导入记录核对。</p>
          <button className="text-button" onClick={() => blocker.reset()}>
            继续核对
          </button>
          <button className="text-button" onClick={() => blocker.proceed()}>
            离开并稍后核对
          </button>
        </div>
      )}
      <div className="page-title">
        <h1>时间轴核对</h1>
        <Link
          className="text-button"
          to={`/admin/speech-clips?planId=${plan.id}`}
        >
          返回配音
        </Link>
      </div>
      <p>
        {plan.lessonId} · v{plan.lessonRevision}
      </p>
      <p className="muted">
        导入本机对齐结果，再逐个试听与校对。通过核对不会自动登记录音或发布课程。
      </p>
      <form onSubmit={submit} className="settings-group alignment-import">
        <fieldset disabled={pending || frozen || reviewLocked}>
          <label>
            对齐结果 JSON
            <input
              type="file"
              accept=".json,application/json"
              onChange={(e) => setFile(e.target.files?.[0] ?? null)}
            />
          </label>
          <label>
            导入理由
            <textarea
              required
              value={reason}
              onChange={(e) => setReason(e.target.value)}
            />
          </label>
        </fieldset>
        <button
          className="primary"
          disabled={reviewLocked}
          aria-disabled={pending}
          aria-busy={pending}
        >
          {pending ? "正在核对" : frozen ? "核对同一导入请求" : "导入预测"}
        </button>
        {error && <p role="alert">{error}</p>}
      </form>
      {alignment && (
        <>
          <div className="page-title">
            <h2>逐片段核对</h2>
            <span>
              {index + 1} / {alignment.clips.length}
            </span>
          </div>
          <Timeline
            key={`${alignment.id}:${index}:${alignment.clips[index].accepted}`}
            alignment={alignment}
            clip={alignment.clips[index]}
            language={speechTargetLocale(
              plan,
              plan.targets.find(
                (target) =>
                  target.generationKey === alignment.clips[index].generationKey,
              ),
            )}
            onSaved={(result) => {
              setAlignment(result);
              void revalidator.revalidate();
            }}
            onLocked={setReviewLocked}
          />
          <div className="alignment-navigation">
            <button
              className="text-button"
              disabled={hasRequest || index === 0}
              onClick={() => setIndex(index - 1)}
            >
              上个片段
            </button>
            <button
              className="text-button"
              disabled={hasRequest || index === alignment.clips.length - 1}
              onClick={() => setIndex(index + 1)}
            >
              下个片段
            </button>
          </div>
          <SpeechPackage
            key={alignment.id}
            alignment={alignment}
            lessonRevision={plan.lessonRevision}
            disabled={hasRequest}
            onPending={setPackagePending}
          />
        </>
      )}
      <h2>导入记录</h2>
      {entries.items.length === 0 ? (
        <p className="muted">尚未导入时间轴。</p>
      ) : (
        entries.items.map((item) => (
          <Link
            className="settings-row"
            key={item.id}
            to={`?planId=${plan.id}&alignmentId=${item.id}`}
          >
            <span>{item.createdAt.slice(0, 10)}</span>
            <span>
              {item.acceptedCount} / {item.clipCount} 已核对
            </span>
          </Link>
        ))
      )}
      {entries.next && (
        <Link
          className="text-button"
          to={`?planId=${plan.id}&after=${entries.next}`}
        >
          更早的导入记录
        </Link>
      )}
    </section>
  );
}
function Timeline({
  alignment,
  clip,
  language,
  onSaved,
  onLocked,
}: {
  alignment: AdminAlignment;
  clip: AdminAlignmentClip;
  language: string;
  onSaved: (value: AdminAlignment) => void;
  onLocked: (value: boolean) => void;
}) {
  const [words, setWords] = useState(clip.words),
    [reason, setReason] = useState("");
  const [heard, setHeard] = useState(false),
    [checked, setChecked] = useState(false),
    [pending, setPending] = useState(false),
    [frozen, setFrozen] = useState(false),
    [error, setError] = useState("");
  const [active, setActive] = useState<string | null>(null),
    [status, setStatus] = useState("idle"),
    [progress, setProgress] = useState(0);
  const player = useRef<RecordingPlayer | null>(null),
    attempt = useRef<AdminAlignmentReview | null>(null),
    controller = useRef<AbortController | null>(null),
    busy = useRef(false);
  const { stop, toast } = useLearning();
  useEffect(() => {
    player.current = new RecordingPlayer();
    const cancel = () => {
      player.current?.stop();
      controller.current?.abort();
    };
    window.addEventListener("pagehide", cancel);
    return () => {
      cancel();
      window.removeEventListener("pagehide", cancel);
    };
  }, []);
  function listen(startMs = 0, endMs = clip.durationMs) {
    if (active === `${startMs}:${endMs}` && player.current?.isActive) {
      if (status === "paused") player.current.resume();
      else player.current.pause();
      return;
    }
    stop();
    player.current?.stop();
    setActive(`${startMs}:${endMs}`);
    setProgress(0);
    player.current?.play(
      {
        url: `/api/v1/operator/speech-clips/${clip.clipId}/file`,
        startMs,
        endMs,
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
          toast("录音暂时无法播放，请核对后重试。");
        },
      },
    );
  }
  async function save(accepted: boolean) {
    if (busy.current) return;
    if (
      !attempt.current &&
      (!heard || !reason.trim() || (accepted && !checked))
    ) {
      setError("请先实际试听、核对时间并填写理由。");
      return;
    }
    if (!attempt.current && accepted) {
      let previous = 0;
      for (const word of words) {
        if (
          word.startMs === null ||
          word.endMs === null ||
          !Number.isInteger(word.startMs) ||
          !Number.isInteger(word.endMs) ||
          word.startMs < previous ||
          word.startMs >= word.endMs ||
          word.endMs > clip.durationMs
        ) {
          setError("请填写完整、依次排列且不超出录音的单词时间。");
          return;
        }
        previous = word.endMs;
      }
    }
    if (!attempt.current)
      attempt.current = {
        expectedReportHash: alignment.reportHash,
        heard,
        timingsChecked: checked,
        accepted,
        words: accepted ? words : [],
        reason,
      };
    busy.current = true;
    onLocked(true);
    setPending(true);
    setFrozen(true);
    setError("");
    const abort = new AbortController();
    controller.current = abort;
    try {
      const result = await adminWrite<AdminAlignment>(
        `speech-alignments/${alignment.id}/clips/${clip.clipId}/review`,
        attempt.current,
        abort.signal,
      );
      attempt.current = null;
      onLocked(false);
      onSaved(result);
    } catch (e) {
      if (!abort.signal.aborted) {
        if (
          e instanceof AdminWriteError &&
          [400, 413, 422].includes(e.status)
        ) {
          attempt.current = null;
          setFrozen(false);
          onLocked(false);
        }
        setError(e instanceof Error ? e.message : "结果未确认，请核对原请求。");
      }
    } finally {
      busy.current = false;
      if (!abort.signal.aborted) setPending(false);
    }
  }
  return (
    <article className="alignment-stage">
      <p className="fr" lang={language}>
        {clip.text}
      </p>
      <button type="button" className="text-button" onClick={() => listen()}>
        {active === `0:${clip.durationMs}`
          ? status === "paused"
            ? "继续整句"
            : "暂停整句"
          : "试听整句"}
      </button>
      {active && (
        <div className="recording-preview-track">
          <span style={{ width: `${progress * 100}%` }} />
        </div>
      )}
      {clip.issues.length > 0 && (
        <p className="muted">
          原始预测存在 {clip.issues.length}{" "}
          类待核对问题。可以填写实际时间；原预测保留。
        </p>
      )}
      <fieldset className="alignment-word-times">
        <legend>单词时间（毫秒）</legend>
        {words.map((word, i) => (
          <div className="alignment-word" key={`${word.start}:${word.end}`}>
            <button
              className="fr text-button"
              lang={language}
              type="button"
              disabled={
                word.startMs === null ||
                word.endMs === null ||
                word.startMs >= word.endMs ||
                word.endMs > clip.durationMs
              }
              onClick={() => listen(word.startMs!, word.endMs!)}
            >
              {word.text}
            </button>
            {(["startMs", "endMs"] as const).map((name) => (
              <label key={name}>
                {name === "startMs" ? "起点" : "终点"}
                <input
                  aria-label={`${word.text} ${name === "startMs" ? "起点" : "终点"}（毫秒）`}
                  disabled={pending || frozen || clip.accepted !== null}
                  type="number"
                  min="0"
                  max={clip.durationMs}
                  step="1"
                  value={word[name] ?? ""}
                  onChange={(e) =>
                    setWords(
                      words.map((w, index) =>
                        index === i
                          ? {
                              ...w,
                              [name]:
                                e.target.value === ""
                                  ? null
                                  : Number(e.target.value),
                            }
                          : w,
                      ),
                    )
                  }
                />
              </label>
            ))}
          </div>
        ))}
        <label>
          核对理由
          <textarea
            disabled={pending || frozen || clip.accepted !== null}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
        </label>
        <label>
          <input
            disabled={pending || frozen || clip.accepted !== null}
            type="checkbox"
            checked={heard}
            onChange={(e) => setHeard(e.target.checked)}
          />
          我已实际试听这个片段
        </label>
        <label>
          <input
            disabled={pending || frozen || clip.accepted !== null}
            type="checkbox"
            checked={checked}
            onChange={(e) => setChecked(e.target.checked)}
          />
          我已核对逐词时间
        </label>
      </fieldset>
      {clip.accepted !== null ? (
        <p role="status">
          {clip.accepted
            ? "时间轴已核对通过"
            : "时间轴已退回；需要导入新的修订结果"}
        </p>
      ) : (
        <>
          <button
            className="primary"
            aria-disabled={pending}
            aria-busy={pending}
            onClick={() => save(true)}
          >
            {pending ? "正在保存" : frozen ? "核对同一审核请求" : "确认时间轴"}
          </button>
          {!frozen && (
            <button className="text-button" onClick={() => save(false)}>
              退回这个片段
            </button>
          )}
        </>
      )}
      {error && <p role="alert">{error}</p>}
    </article>
  );
}
