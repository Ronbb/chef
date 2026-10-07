import { useCommittedDialog } from "../components/committed-dialog";
import { Link, data, useLocation, useRevalidator } from "react-router";
import { useId, useEffect, useRef, useState } from "react";
import type { AdminCharacterVoice } from "@brioche/contracts/AdminCharacterVoice";
import type { CharacterVoiceProfile } from "@brioche/contracts/CharacterVoiceProfile";
import { QWEN_FRENCH_SYSTEM_VOICES } from "@brioche/contracts/tts-voices";
import { ChoiceDialog } from "../components/choice-dialog";
import type { AdminAudition } from "@brioche/contracts/AdminAudition";
import type { AdminAuditions } from "@brioche/contracts/AdminAuditions";
import type { AdminAuditionRequest } from "@brioche/contracts/AdminAuditionRequest";
import type { AdminVoiceJob } from "@brioche/contracts/AdminVoiceJob";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite } from "../lib/admin.client";
import { RecordingPlayer } from "../lib/recording-playback";
import { Icon } from "../components/icon";
import { useLearning } from "../components/learning";
import { usePageCursorFocus } from "../components/page-cursor-focus";
import type { Route } from "./+types/admin-voice-auditions";
export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const params = new URL(request.url).searchParams,
    query = new URLSearchParams();
  const jobId = params.get("jobId"),
    id = params.get("auditionId");
  for (const value of [jobId, id])
    if (value && !/^[a-f0-9]{32}$/.test(value))
      throw new Response("编号无效。", { status: 400 });
  const characterId = params.get("characterId"),
    characterRevision = params.get("characterRevision");
  if (
    (characterId !== null) !== (characterRevision !== null) ||
    (characterId !== null &&
      (!/^[A-Za-z0-9_-]{1,100}$/.test(characterId) ||
        !/^[1-9][0-9]*$/.test(characterRevision!) ||
        Number(characterRevision) > 2147483647)) ||
    (jobId && characterId)
  )
    throw new Response("角色版本无效。", { status: 400 });
  if (characterId) {
    query.set("characterId", characterId);
    query.set("characterRevision", characterRevision!);
  }
  if (jobId) query.set("cloneJobId", jobId);
  if (params.has("afterId")) query.set("afterId", params.get("afterId")!);
  const list = await getPrivate<AdminAuditions>(
    request,
    `/api/v1/operator/voice-auditions${query.size ? `?${query}` : ""}`,
  );
  const source = jobId
    ? await getPrivate<AdminVoiceJob>(
        request,
        `/api/v1/operator/voice-jobs/${jobId}`,
      )
    : null;
  const characterSource = characterId
    ? await getPrivate<AdminCharacterVoice>(
        request,
        `/api/v1/operator/characters/${characterId}/${characterRevision}`,
      )
    : null;
  const selected = id
    ? await getPrivate<AdminAudition>(
        request,
        `/api/v1/operator/voice-auditions/${id}`,
      )
    : null;
  return data(
    { ...list, source, characterSource, selected },
    { headers: headers() },
  );
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
const labels: Record<string, string> = {
  submitted: "正在生成试听",
  ready: "试听已生成",
  unknown: "生成结果未确认，请先核对提供方记录",
  failed: "提供方拒绝了这次请求",
};
const voiceChoices = QWEN_FRENCH_SYSTEM_VOICES.map(([value, label]) => ({
  value,
  label,
  detail: "法语 · Flash 3.1",
}));
const defaultProfile: CharacterVoiceProfile = {
  personality: "Friendly and thoughtful.",
  speakingStyle: "Natural, clear French at a calm pace.",
  defaultEmotion: "Warm and relaxed.",
  provider: "qwen",
  model: "qwen-audio-3.1-tts-flash",
  voiceId: QWEN_FRENCH_SYSTEM_VOICES[0][0],
  voiceKind: "system",
  locale: "fr-FR",
  rate: 1,
  referenceAudio: null,
};
export default function Auditions({ loaderData }: Route.ComponentProps) {
  const dialogTitleId = useId();
  const location = useLocation(),
    heading = usePageCursorFocus(location.search),
    refresh = useRevalidator();
  const { toast, stop } = useLearning();
  const dialog = useRef<HTMLDialogElement>(null),
    busy = useRef(false),
    write = useRef<AbortController | null>(null),
    attempt = useRef<AdminAuditionRequest | null>(null);
  const openDialog = useCommittedDialog(dialog);
  const [mode, setMode] = useState<"create" | AdminAudition | null>(null),
    [text, setText] = useState(
      "Bonjour ! Je voudrais une baguette, s’il vous plaît. C’est combien ? Merci, au revoir !",
    ),
    [emotion, setEmotion] = useState(
      "Warm greeting, polite request, curious question, then a pleased farewell.",
    ),
    [reason, setReason] = useState(""),
    [consent, setConsent] = useState(false),
    [accepted, setAccepted] = useState(true),
    [pending, setPending] = useState(false),
    [attempted, setAttempted] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [profile, setProfile] = useState<CharacterVoiceProfile>(defaultProfile);
  const player = useRef<RecordingPlayer | null>(null),
    [active, setActive] = useState<string | null>(null),
    [status, setStatus] = useState("idle"),
    [progress, setProgress] = useState(0);
  useEffect(() => {
    player.current = new RecordingPlayer();
    return () => {
      player.current?.stop();
      write.current?.abort();
    };
  }, []);
  useEffect(() => {
    player.current?.stop();
    setActive(null);
    setStatus("idle");
    setProgress(0);
    write.current?.abort();
    busy.current = false;
    setPending(false);
    dialog.current?.close();
    setMode(null);
    attempt.current = null;
  }, [location.search]);
  useEffect(() => {
    const release = () => {
      player.current?.stop();
      setActive(null);
      setStatus("idle");
    };
    window.addEventListener("pagehide", release);
    return () => window.removeEventListener("pagehide", release);
  }, []);
  const items = loaderData.selected
    ? [
        loaderData.selected,
        ...loaderData.items.filter((i) => i.id !== loaderData.selected!.id),
      ]
    : loaderData.items;
  const waiting = items.some((i) => i.status === "submitted");
  useEffect(() => {
    if (!waiting || refresh.state !== "idle") return;
    const timer = setTimeout(() => refresh.revalidate(), 2000);
    return () => clearTimeout(timer);
  }, [waiting, refresh.state, refresh]);
  function open(next: NonNullable<typeof mode>) {
    if (busy.current) return;
    player.current?.stop();
    setActive(null);
    setStatus("idle");
    if (next === "create" && loaderData.characterSource) {
      const previous = loaderData.characterSource.profile;
      setProfile({
        ...defaultProfile,
        ...previous,
        provider: defaultProfile.provider,
        model: defaultProfile.model,
        voiceKind: "system",
        referenceAudio: null,
        voiceId: voiceChoices.some((v) => v.value === previous?.voiceId)
          ? previous!.voiceId
          : defaultProfile.voiceId,
      });
    }
    setMode(next);
    setReason("");
    setConsent(false);
    setAccepted(true);
    setError("");
    setAttempted(false);
    attempt.current = null;
    openDialog();
  }
  function listen(item: AdminAudition) {
    if (item.id === active && player.current?.isActive) {
      if (status === "paused") player.current.resume();
      else {
        player.current.pause();
        setStatus("paused");
      }
      return;
    }
    stop();
    setActive(item.id);
    setProgress(0);
    player.current?.play(
      {
        url: `/api/v1/operator/voice-auditions/${item.id}/file`,
        startMs: 0,
        endMs: item.durationMs!,
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
          toast("试听无法播放，请核对登录状态与文件。");
        },
      },
    );
  }
  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (busy.current || !mode || !consent || !reason.trim()) return;
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      if (mode === "create") {
        if (!attempt.current) {
          const source = loaderData.source;
          const character = loaderData.characterSource;
          if (!character && (!source || source.status !== "ready"))
            throw Error("请先核对音色状态。");
          attempt.current = {
            id: Array.from(crypto.getRandomValues(new Uint8Array(16)), (n) =>
              n.toString(16).padStart(2, "0"),
            ).join(""),
            cloneJobId: character ? null : source!.id,
            expectedCloneVersion: character ? null : source!.version,
            candidate: character
              ? {
                  characterId: character.character.characterId,
                  characterRevision: character.character.revision,
                  expectedVoiceRevision: character.voiceRevision,
                  profile: structuredClone(profile),
                }
              : null,
            text,
            emotion,
            costConfirmed: true,
            reason,
          };
          setAttempted(true);
        }
        const result = await adminWrite<AdminAudition>(
          "voice-auditions",
          attempt.current,
          controller.signal,
        );
        controller.signal.throwIfAborted();
        setNotice(`试听任务已记录：${result.id}。等待期间只刷新本服务记录。`);
      } else {
        const result = await adminWrite<AdminAudition>(
          `voice-auditions/${mode.id}/review`,
          {
            accepted,
            heard: true,
            expectedVoiceRevision: mode.baseVoiceRevision,
            reason,
          },
          controller.signal,
        );
        controller.signal.throwIfAborted();
        setNotice(
          result.accepted
            ? `试听已通过，新增声音 v${result.appliedVoiceRevision}。课程录音仍需单独生成与发布。`
            : "已记录试听未通过，角色原声音保持。",
        );
      }
      dialog.current?.close();
      refresh.revalidate();
    } catch (e) {
      if (!controller.signal.aborted)
        setError(
          e instanceof Error ? e.message : "操作未确认，请核对任务记录。",
        );
    } finally {
      if (!controller.signal.aborted) {
        busy.current = false;
        setPending(false);
      }
    }
  }
  const next = new URLSearchParams();
  if (loaderData.next) next.set("afterId", loaderData.next);
  if (loaderData.source) next.set("jobId", loaderData.source.id);
  if (loaderData.characterSource) {
    next.set("characterId", loaderData.characterSource.character.characterId);
    next.set(
      "characterRevision",
      String(loaderData.characterSource.character.revision),
    );
  }
  return (
    <section className="settings-page page-arrive">
      <div className="settings-heading">
        <h1 ref={heading} tabIndex={-1}>
          角色声音试听
        </h1>
        <Link className="text-button" to="/admin">
          管理员后台
        </Link>
      </div>
      <p>试听通过后追加声音版本，已发布课程和原声音档案保持原样。</p>
      <Link className="text-button" to="/admin/characters">
        角色库
      </Link>
      <Link className="text-button" to="/admin/voice-jobs">
        音色创建任务
      </Link>
      {!loaderData.configured && <p role="status">当前未配置生成服务。</p>}
      {loaderData.characterSource && (
        <article className="voice-job-card">
          <header className="character-profile-head">
            <img
              width="64"
              height="64"
              alt=""
              src={`/api/v1/operator/characters/${loaderData.characterSource.character.characterId}/${loaderData.characterSource.character.revision}/avatar`}
            />
            <div>
              <h2>{loaderData.characterSource.character.displayName}</h2>
              <p>
                角色 v{loaderData.characterSource.character.revision} ·{" "}
                {loaderData.characterSource.voiceRevision
                  ? `原声音 v${loaderData.characterSource.voiceRevision}`
                  : "尚未配置声音"}
              </p>
            </div>
          </header>
          <p>选择法语系统音色与角色语气，试听后再确认新声音。</p>
          {loaderData.configured && (
            <button
              className="secondary"
              onClick={() => open("create")}
              aria-disabled={pending}
            >
              生成一段试听
            </button>
          )}
        </article>
      )}
      {loaderData.source && (
        <article className="voice-job-card">
          <h2>{loaderData.source.characterId}</h2>
          <p>
            角色 v{loaderData.source.characterRevision} · 原声音 v
            {loaderData.source.voiceRevision}
          </p>
          {loaderData.configured && loaderData.source.status === "ready" ? (
            <button
              className="secondary"
              onClick={() => open("create")}
              aria-disabled={pending}
            >
              生成一段试听
            </button>
          ) : (
            <p>先在音色任务中核对可用状态。</p>
          )}
        </article>
      )}
      <button
        className="text-button"
        onClick={() => refresh.revalidate()}
        aria-busy={refresh.state !== "idle"}
      >
        刷新任务记录
      </button>
      {notice && <p role="status">{notice}</p>}
      <div className="admin-card-list">
        {items.map((item) => (
          <article className="voice-job-card" key={item.id}>
            <h2>
              {item.characterId} · 角色 v{item.characterRevision}
            </h2>
            <p role="status">{labels[item.status] ?? "未知任务状态"}</p>
            <p lang="fr">{item.text}</p>
            <p>{item.emotion}</p>
            <p>音色：{item.voiceId}</p>
            <dl>
              <dt>个性特点</dt>
              <dd>{item.profile.personality}</dd>
              <dt>说话习惯</dt>
              <dd>{item.profile.speakingStyle}</dd>
              <dt>默认语气</dt>
              <dd>{item.profile.defaultEmotion}</dd>
              <dt>语速</dt>
              <dd>{item.profile.rate}×</dd>
            </dl>
            <Link
              className="text-button"
              to={`/admin/voice-auditions?auditionId=${item.id}`}
            >
              固定试听记录
            </Link>
            {item.status === "ready" && item.durationMs && (
              <button
                className="recording-preview"
                aria-label={`${active === item.id && status === "playing" ? "暂停" : "试听"} ${item.id}`}
                aria-busy={active === item.id && status === "loading"}
                onClick={() => listen(item)}
              >
                <span className="recording-preview-track">
                  <span
                    style={{
                      width: `${active === item.id ? progress * 100 : 0}%`,
                    }}
                  />
                </span>
                <Icon
                  name={
                    active === item.id && status === "playing"
                      ? "pause"
                      : "play"
                  }
                />
                <span className="recording-preview-track" />
              </button>
            )}
            {item.accepted !== null ? (
              <p>
                {item.accepted
                  ? `已通过 · 声音 v${item.appliedVoiceRevision}`
                  : "试听未通过"}
              </p>
            ) : (
              item.status === "ready" && (
                <button
                  className="text-button"
                  onClick={() => open(item)}
                  aria-disabled={pending}
                >
                  确认试听结果
                </button>
              )
            )}
          </article>
        ))}
      </div>
      {!items.length && <p>还没有试听任务。</p>}
      {loaderData.next && (
        <Link className="text-button" to={`/admin/voice-auditions?${next}`}>
          下一页试听
        </Link>
      )}
      <dialog
        aria-labelledby={dialogTitleId}
        className="admin-dialog"
        ref={dialog}
        onCancel={(e) => {
          if (busy.current) e.preventDefault();
        }}
        onClose={(event) => {
          if (event.target !== event.currentTarget) return;
          setMode(null);
          setError("");
          attempt.current = null;
        }}
      >
        <h2 id={dialogTitleId}>
          {mode === "create" ? "生成角色试听" : "确认试听结果"}
        </h2>
        <form className="reference-delivery-form" onSubmit={submit}>
          {mode === "create" ? (
            <>
              {loaderData.characterSource && (
                <fieldset className="audition-decision">
                  <legend>候选声音 · Flash 3.1 · 法语</legend>
                  <ChoiceDialog
                    title="法语音色"
                    value={profile.voiceId}
                    choices={voiceChoices}
                    disabled={pending || attempted}
                    onChange={(voiceId) =>
                      setProfile((p) => ({ ...p, voiceId }))
                    }
                  />
                  <label>
                    个性特点
                    <input
                      name="candidatePersonality"
                      required
                      maxLength={600}
                      value={profile.personality}
                      onChange={(e) =>
                        setProfile((p) => ({
                          ...p,
                          personality: e.target.value,
                        }))
                      }
                      readOnly={pending || attempted}
                    />
                  </label>
                  <label>
                    说话习惯
                    <input
                      name="candidateStyle"
                      required
                      maxLength={600}
                      value={profile.speakingStyle}
                      onChange={(e) =>
                        setProfile((p) => ({
                          ...p,
                          speakingStyle: e.target.value,
                        }))
                      }
                      readOnly={pending || attempted}
                    />
                  </label>
                  <label>
                    默认语气
                    <input
                      name="candidateEmotion"
                      required
                      maxLength={600}
                      value={profile.defaultEmotion}
                      onChange={(e) =>
                        setProfile((p) => ({
                          ...p,
                          defaultEmotion: e.target.value,
                        }))
                      }
                      readOnly={pending || attempted}
                    />
                  </label>
                  <label>
                    语速
                    <input
                      name="candidateRate"
                      type="number"
                      min={0.5}
                      max={2}
                      step={0.05}
                      required
                      value={profile.rate}
                      onChange={(e) =>
                        setProfile((p) => ({
                          ...p,
                          rate: e.target.valueAsNumber,
                        }))
                      }
                      readOnly={pending || attempted}
                    />
                  </label>
                </fieldset>
              )}
              <label>
                法语台词
                <textarea
                  name="auditionText"
                  required
                  maxLength={600}
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  readOnly={pending || attempted}
                />
              </label>
              <label>
                场景情绪
                <input
                  name="auditionEmotion"
                  type="text"
                  required
                  maxLength={1000}
                  value={emotion}
                  onChange={(e) => setEmotion(e.target.value)}
                  readOnly={pending || attempted}
                />
              </label>
              <p>
                会向 Qwen
                请求一次合成，可能计费。关闭页面不会撤回已发送的请求；结果不明确时先核对任务和提供方记录。
              </p>
            </>
          ) : (
            <fieldset className="audition-decision">
              <legend>试听决定</legend>
              <label>
                <input
                  type="radio"
                  name="auditionDecision"
                  checked={accepted}
                  onChange={() => setAccepted(true)}
                  disabled={pending}
                />
                通过，新增角色声音版本
              </label>
              <label>
                <input
                  type="radio"
                  name="auditionDecision"
                  checked={!accepted}
                  onChange={() => setAccepted(false)}
                  disabled={pending}
                />
                未通过，保留原声音
              </label>
            </fieldset>
          )}
          <label>
            <input
              name="auditionConsent"
              type="checkbox"
              checked={consent}
              onChange={(e) => setConsent(e.target.checked)}
              disabled={pending || attempted}
              required
            />
            {mode === "create"
              ? "确认这次合成可能计费"
              : "我已完整试听，并核对发音、情绪与角色声音一致性"}
          </label>
          <label>
            理由
            <textarea
              name="auditionReason"
              required
              maxLength={500}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              readOnly={pending || attempted}
            />
          </label>
          {error && <p role="alert">{error}</p>}
          <button
            className="primary"
            type="submit"
            disabled={!consent || !reason.trim()}
            aria-disabled={pending}
            aria-busy={pending}
          >
            {pending
              ? "正在记录…"
              : mode === "create"
                ? attempted
                  ? "核对这次请求"
                  : "提交试听任务"
                : accepted
                  ? "确认并新增声音版本"
                  : "记录试听未通过"}
          </button>
          <button
            className="text-button"
            type="button"
            aria-disabled={pending}
            onClick={() => {
              if (!busy.current) dialog.current?.close();
            }}
          >
            关闭
          </button>
        </form>
      </dialog>
    </section>
  );
}
