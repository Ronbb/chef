import { data, Link, useLocation, useRevalidator } from "react-router";
import { useEffect, useRef, useState } from "react";
import type { AdminNeutralSpeechOptions } from "@brioche/contracts/AdminNeutralSpeechOptions";
import type { AdminSpeechPlan } from "@brioche/contracts/AdminSpeechPlan";
import type { AdminSpeechPlans } from "@brioche/contracts/AdminSpeechPlans";
import type { AdminSpeechPlanRequest } from "@brioche/contracts/AdminSpeechPlanRequest";
import type { AdminSpeechSelection } from "@brioche/contracts/AdminSpeechSelection";
import { ChoiceDialog } from "../components/choice-dialog";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite } from "../lib/admin.client";
import type { Route } from "./+types/admin-speech-plans";

export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const params = new URL(request.url).searchParams;
  const id = params.get("lessonId"),
    revision = params.get("revision"),
    planId = params.get("planId"),
    after = params.get("afterId");
  if (
    !id ||
    !/^[A-Za-z0-9_-]{1,100}$/.test(id) ||
    !revision ||
    !/^[1-9][0-9]*$/.test(revision) ||
    Number(revision) > 2147483647 ||
    [planId, after].some((v) => v !== null && !/^[a-f0-9]{32}$/.test(v))
  )
    throw new Response("课程版本无效。", { status: 400 });
  const query = new URLSearchParams({ lessonId: id, lessonRevision: revision });
  if (after) query.set("afterId", after);
  const options = await getPrivate<AdminNeutralSpeechOptions>(
    request,
    `/api/v2/operator/lessons/${id}/revisions/${revision}/speech-options`,
  );
  const plans = await getPrivate<AdminSpeechPlans>(
    request,
    `/api/v1/operator/speech-plans?${query}`,
  );
  const selected = planId
    ? await getPrivate<AdminSpeechPlan>(
        request,
        `/api/v1/operator/speech-plans/${planId}`,
      )
    : null;
  if (
    selected &&
    (selected.lessonId !== id || selected.lessonRevision !== Number(revision))
  )
    throw new Response("计划与课程版本不符。", { status: 400 });
  return data({ options, plans, selected }, { headers: headers() });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
function fixed(voice: AdminNeutralSpeechOptions["voices"][number]) {
  return {
    characterId: voice.character.characterId,
    characterRevision: voice.character.revision,
    voiceRevision: voice.voiceRevision,
  };
}
function initial(options: AdminNeutralSpeechOptions): AdminSpeechSelection {
  const needed = new Set<string>();
  for (const block of options.lesson.blocks) {
    if (block.type === "article") needed.add(block.narratorId);
    if (block.type === "dialogue")
      for (const turn of block.turns) {
        const speaker = block.speakers.find((s) => s.id === turn.speakerId);
        if (speaker) needed.add(speaker.characterId);
      }
  }
  const narrator =
    options.voices.find((v) => v.profile && v.voiceRevision > 0) ??
    options.voices[0];
  needed.add(narrator.character.characterId);
  return {
    voices: options.voices
      .filter((v) => needed.has(v.character.characterId))
      .map(fixed),
    knowledgeNarrator: fixed(narrator),
    emotions: {},
  };
}
export default function SpeechPlans({
  loaderData: { options, plans, selected },
}: Route.ComponentProps) {
  const refresh = useRevalidator();
  const location = useLocation();
  const [dirty, setDirty] = useState(true);
  const [selection, setSelection] = useState(() => initial(options));
  const [preview, setPreview] = useState<AdminSpeechPlan | null>(null),
    [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [reason, setReason] = useState("");
  const busy = useRef(false),
    controller = useRef<AbortController | null>(null),
    attempt = useRef<AdminSpeechPlanRequest | null>(null);
  const pageScope = `${options.lesson.id}:${options.lesson.revision}:${location.search}`;
  const previousScope = useRef(pageScope);
  useEffect(() => () => controller.current?.abort(), []);
  useEffect(() => {
    if (previousScope.current === pageScope) return;
    previousScope.current = pageScope;
    controller.current?.abort();
    busy.current = false;
    setPending(false);
    attempt.current = null;
    setSelection(initial(options));
    setPreview(null);
    setDirty(true);
    setReason("");
    setError("");
    setNotice("");
  }, [pageScope]);
  function change(next: AdminSpeechSelection) {
    if (busy.current || attempt.current) return;
    setSelection(next);
    setDirty(true);
    setNotice("");
  }
  async function run(save = false) {
    if (busy.current) return;
    busy.current = true;
    setPending(true);
    setError("");
    const abort = new AbortController();
    controller.current = abort;
    try {
      let result: AdminSpeechPlan;
      if (save) {
        if (!preview || dirty) return;
        attempt.current ??= {
          id: crypto.randomUUID().replaceAll("-", ""),
          preview: {
            lessonId: options.lesson.id,
            lessonRevision: options.lesson.revision,
            selection,
          },
          expectedPlanHash: preview.planHash,
          reason,
        };
        result = await adminWrite<AdminSpeechPlan>(
          "speech-plans",
          attempt.current,
          abort.signal,
        );
        if (abort.signal.aborted) return;
        setPreview(null);
        attempt.current = null;
        setReason("");
        setNotice("配音计划已保存，尚未生成音频。");
        refresh.revalidate();
      } else {
        result = await adminWrite<AdminSpeechPlan>(
          "speech-plans/preview",
          {
            lessonId: options.lesson.id,
            lessonRevision: options.lesson.revision,
            selection,
          },
          abort.signal,
        );
        if (abort.signal.aborted) return;
        setPreview(result);
        setDirty(false);
      }
    } catch (e) {
      if (!abort.signal.aborted)
        setError(e instanceof Error ? e.message : "操作未确认。");
    } finally {
      if (controller.current === abort) {
        busy.current = false;
        if (!abort.signal.aborted) setPending(false);
      }
    }
  }
  const missing = selection.voices.filter((v) => v.voiceRevision === 0);
  const current = selected ?? preview;
  const frozen = pending || attempt.current !== null;
  const validReason =
    reason.trim().length > 0 && new TextEncoder().encode(reason).length <= 1000;
  const base = `/admin/speech-plans?lessonId=${options.lesson.id}&revision=${options.lesson.revision}`;
  return (
    <section className="settings-page speech-plans-page page-arrive">
      <div className="section-heading">
        <h1>课程配音</h1>
        <Link className="text-button" to="/admin">
          返回后台
        </Link>
      </div>
      <p>
        {options.lesson.title.zh} · v{options.lesson.revision}
      </p>
      <h2>角色声音</h2>
      <div className="settings-group">
        {(selected?.voices ?? options.voices).map((v) => (
          <div className="setting-row" key={v.character.characterId}>
            <div>
              <strong>{v.character.displayName}</strong>
              <p className="admin-note">
                角色 v{v.character.revision} ·{" "}
                {v.voiceRevision ? `声音 v${v.voiceRevision}` : "尚无声音档案"}
              </p>
              <p className="admin-note">{v.profile?.speakingStyle}</p>
            </div>
            <Link
              className="text-button"
              to={`/admin/voice-auditions?characterId=${v.character.characterId}&characterRevision=${v.character.revision}`}
            >
              试听声音
            </Link>
          </div>
        ))}
      </div>
      {missing.length > 0 && (
        <p role="status">请先为本课角色补齐并确认声音档案。</p>
      )}
      {!selected && (
        <>
          <div className="setting-row">
            <span>词汇与例句讲解声音</span>
            <ChoiceDialog
              title="讲解声音"
              value={selection.knowledgeNarrator.characterId}
              disabled={frozen}
              choices={options.voices
                .filter((v) => v.profile && v.voiceRevision > 0)
                .map((v) => ({
                  value: v.character.characterId,
                  label: v.character.displayName,
                  detail: `角色 v${v.character.revision} · 声音 v${v.voiceRevision}`,
                }))}
              onChange={(id) => {
                const next = fixed(
                  options.voices.find((v) => v.character.characterId === id)!,
                );
                const defaults = initial(options);
                const ids = new Set(
                  defaults.voices
                    .filter(
                      (v) =>
                        v.characterId !==
                        defaults.knowledgeNarrator.characterId,
                    )
                    .map((v) => v.characterId),
                );
                for (const b of options.lesson.blocks) {
                  if (b.type === "article") ids.add(b.narratorId);
                  if (b.type === "dialogue")
                    for (const t of b.turns) {
                      const s = b.speakers.find((s) => s.id === t.speakerId);
                      if (s) ids.add(s.characterId);
                    }
                }
                ids.add(id);
                change({
                  ...selection,
                  knowledgeNarrator: next,
                  voices: options.voices
                    .filter((v) => ids.has(v.character.characterId))
                    .map(fixed),
                });
              }}
            />
          </div>
          <button
            className="primary"
            disabled={missing.length > 0 || attempt.current !== null}
            aria-disabled={pending}
            aria-busy={pending}
            onClick={() => run()}
          >
            核对配音计划
          </button>
        </>
      )}
      {error && <p role="alert">{error}</p>}
      {attempt.current && !pending && (
        <button
          className="text-button"
          onClick={() => {
            attempt.current = null;
            setDirty(true);
            setError("");
          }}
        >
          重新开始核对
        </button>
      )}
      {notice && <p role="status">{notice}</p>}
      {current && (
        <>
          {!selected && dirty && (
            <p role="status">设置已修改，请重新核对配音计划。</p>
          )}
          <h2>{current.id ? "已保存的计划" : "配音计划预览"}</h2>
          {current.id && (
            <Link
              className="text-button"
              to={`/admin/speech-clips?planId=${current.id}`}
            >
              生成与审听音频
            </Link>
          )}
          <p>
            {current.targets.length} 个内容片段 · {current.requestCount}{" "}
            次独立生成 · {current.totalRequestCharacters} 个法语字符
          </p>
          <p className="admin-note">
            相同配音请求已合并。此处只核对计划，保存不会触发收费合成。
          </p>
          <div className="settings-group">
            {current.targets.map((t) => (
              <div className="setting-row" key={t.pointer}>
                <div>
                  <p lang="fr">{t.text}</p>
                  <p className="admin-note">
                    {
                      options.lesson.cast.find(
                        (c) => c.characterId === t.voice.characterId,
                      )?.displayName
                    }{" "}
                    · 声音 v{t.voice.voiceRevision}
                  </p>
                  <p className="admin-note">{t.emotion}</p>
                  {!selected && (
                    <label>
                      场景情绪
                      <input
                        disabled={frozen}
                        maxLength={1000}
                        value={selection.emotions[t.pointer] ?? ""}
                        placeholder="沿用角色默认情绪"
                        onChange={(e) => {
                          const emotions = { ...selection.emotions };
                          if (e.target.value)
                            emotions[t.pointer] = e.target.value;
                          else delete emotions[t.pointer];
                          change({ ...selection, emotions });
                        }}
                      />
                    </label>
                  )}
                </div>
              </div>
            ))}
          </div>
          {!selected && (
            <div className="admin-card">
              <label>
                保存理由
                <input
                  value={reason}
                  maxLength={1000}
                  disabled={frozen}
                  onChange={(e) => setReason(e.target.value)}
                />
              </label>
              <button
                className="primary"
                disabled={!validReason || dirty}
                aria-disabled={pending}
                aria-busy={pending}
                onClick={() => run(true)}
              >
                {attempt.current ? "重试保存同一计划" : "保存配音计划"}
              </button>
            </div>
          )}
          {selected && (
            <Link className="text-button" to={base}>
              返回计划列表
            </Link>
          )}
        </>
      )}
      <h2>已保存的计划</h2>
      {plans.items.length === 0 && <p>本课程版本还没有保存的配音计划。</p>}
      <div className="settings-group">
        {plans.items.map((p) => (
          <Link
            className="setting-row"
            key={p.id}
            to={`${base}&planId=${p.id}`}
          >
            <span>
              {p.targets.length} 个片段 · {p.requestCount} 次生成
            </span>
            <span>
              {p.createdAt ? new Date(p.createdAt).toLocaleString("zh-CN") : ""}
            </span>
          </Link>
        ))}
      </div>
      {plans.next && (
        <Link className="text-button" to={`${base}&afterId=${plans.next}`}>
          下一页计划
        </Link>
      )}
    </section>
  );
}
