import { useCommittedDialog } from "../components/committed-dialog";
import { Link, data, useRevalidator } from "react-router";
import { useId, useEffect, useRef, useState } from "react";
import type { AdminCharacterVoices } from "@brioche/contracts/AdminCharacterVoices";
import type { AdminCharacterVoice } from "@brioche/contracts/AdminCharacterVoice";
import type { CharacterVoiceProfile } from "@brioche/contracts/CharacterVoiceProfile";
import { getIdentity, getPrivate } from "../lib/api.server";
import { adminWrite } from "../lib/admin.client";
import { CharacterEditor } from "../components/admin-character-editor";
import { ReferenceRecordingPicker } from "../components/reference-recording-picker";
import type { Route } from "./+types/admin-characters";
import product from "@chef/product";
import { speechAuthoring } from "../lib/speech-authoring";

export async function loader({ request }: Route.LoaderArgs) {
  const { user } = await getIdentity(request);
  if (!user) throw new Response("请先登录。", { status: 401 });
  if (user.role !== "operator")
    throw new Response("仅管理员可以进入。", { status: 403 });
  const input = new URL(request.url).searchParams;
  const query = new URLSearchParams();
  const after = input.get("afterId");
  if (after !== null) query.set("afterId", after);
  const result = await getPrivate<AdminCharacterVoices>(
    request,
    `/api/v1/operator/characters${query.size ? `?${query}` : ""}`,
  );
  let selected: AdminCharacterVoice | null = null;
  const id = input.get("characterId"),
    cr = input.get("characterRevision"),
    vr = input.get("voiceRevision");
  if (id || cr || vr) {
    if (
      !id ||
      !/^[a-z0-9][a-z0-9-]{0,99}$/.test(id) ||
      !cr ||
      !/^[1-9][0-9]{0,9}$/.test(cr) ||
      Number(cr) > 2147483647 ||
      (vr !== null &&
        (!/^[1-9][0-9]{0,9}$/.test(vr) || Number(vr) > 2147483647))
    )
      throw new Response("版本无效。", { status: 400 });
    selected = await getPrivate<AdminCharacterVoice>(
      request,
      `/api/v1/operator/characters/${id}/${cr}${vr ? `/voices/${vr}` : ""}`,
    );
  }
  return data({ ...result, selected }, { headers: headers() });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}

function defaults(characterId: string, locale: string): CharacterVoiceProfile {
  return {
    ...speechAuthoring(locale).profile,
    voiceId:
      characterId === "character-luc"
        ? "xunanchuan_v3.1"
        : "longanlingxin_v3.1",
  };
}
function exportProfile(item: AdminCharacterVoice | AdminCharacterVoice[]) {
  const items = Array.isArray(item) ? item : [item];
  const url = URL.createObjectURL(
    new Blob([JSON.stringify({ items }, null, 2)], {
      type: "application/json",
    }),
  );
  const a = document.createElement("a");
  a.href = url;
  a.download = "character-voices.json";
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export default function Characters({ loaderData }: Route.ComponentProps) {
  const dialogTitleId = useId();
  const [target, setTarget] = useState<AdminCharacterVoice | null>(null);
  const [profile, setProfile] = useState<CharacterVoiceProfile>(
    defaults("", product.targetLanguage),
  );
  const [reason, setReason] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [pending, setPending] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const openDialog = useCommittedDialog(dialog);
  const busy = useRef(false);
  const write = useRef<AbortController | null>(null);
  const refresh = useRevalidator();
  useEffect(() => () => write.current?.abort(), []);
  function open(item: AdminCharacterVoice) {
    setTarget(item);
    setProfile(
      structuredClone(
        item.profile ??
          defaults(item.character.characterId, item.character.speechLocale),
      ),
    );
    setReason("");
    setError("");
    openDialog();
  }
  function field<K extends keyof CharacterVoiceProfile>(
    key: K,
    value: CharacterVoiceProfile[K],
  ) {
    setProfile((p) => ({ ...p, [key]: value }));
  }
  async function save(event: React.FormEvent) {
    event.preventDefault();
    if (profile.referenceAudio && !profile.referenceAudio.assetId) {
      setError("请选择已登记的参考录音。");
      return;
    }
    if (busy.current || !target || !reason.trim()) {
      setError(
        busy.current
          ? "正在保存。"
          : !target
            ? "请重新选择角色。"
            : "请填写修改原因。",
      );
      return;
    }
    busy.current = true;
    setPending(true);
    setError("");
    const controller = new AbortController();
    write.current = controller;
    try {
      await adminWrite<AdminCharacterVoice>(
        "characters",
        {
          characterId: target.character.characterId,
          characterRevision: target.character.revision,
          expectedVoiceRevision: target.voiceRevision,
          profile,
          reason,
        },
        controller.signal,
      );
      if (controller.signal.aborted) return;
      dialog.current?.close();
      setNotice("声音档案已保存为新版本。");
      refresh.revalidate();
    } catch (e) {
      if (!controller.signal.aborted)
        setError(e instanceof Error ? e.message : "保存失败。");
    } finally {
      if (!controller.signal.aborted) {
        busy.current = false;
        setPending(false);
      }
    }
  }
  function card(item: AdminCharacterVoice, historical = false) {
    return (
      <article
        className="character-profile"
        key={`${item.character.characterId}:${item.voiceRevision}`}
      >
        <header className="character-profile-head">
          <img
            width="64"
            height="64"
            alt=""
            src={`/api/v1/operator/characters/${item.character.characterId}/${item.character.revision}/avatar`}
          />
          <div>
            <h2>{item.character.displayName}</h2>
            <p>
              角色 v{item.character.revision} ·{" "}
              {item.voiceRevision
                ? `声音 v${item.voiceRevision}`
                : "尚未配置声音"}
            </p>
          </div>
        </header>
        {item.profile && (
          <>
            <p>{item.profile.personality}</p>
            <dl>
              <dt>说话习惯</dt>
              <dd>{item.profile.speakingStyle}</dd>
              <dt>默认语气</dt>
              <dd>{item.profile.defaultEmotion}</dd>
              <dt>声音</dt>
              <dd>
                {item.profile.model} · {item.profile.voiceId} ·{" "}
                {item.profile.rate}×
              </dd>
              <dt>参考录音</dt>
              <dd>
                {item.profile.referenceAudio
                  ? `${item.profile.referenceAudio.assetId} v${item.profile.referenceAudio.revision}`
                  : "使用固定音色"}
              </dd>
            </dl>
          </>
        )}
        {!historical && <CharacterEditor initial={item} />}
        {!historical && item.character.revision > 1 && (
          <details>
            <summary>历史角色版本</summary>
            {Array.from(
              { length: Math.min(item.character.revision - 1, 20) },
              (_, i) => item.character.revision - i - 1,
            ).map((revision) => (
              <Link
                key={revision}
                className="text-button"
                to={`/admin/characters?characterId=${item.character.characterId}&characterRevision=${revision}`}
              >
                角色 v{revision}
              </Link>
            ))}
          </details>
        )}
        {!historical && (
          <button className="text-button" onClick={() => open(item)}>
            {item.profile ? "创建新声音版本" : "配置声音档案"}
          </button>
        )}
        <Link
          className="text-button"
          to={`/admin/voice-auditions?characterId=${item.character.characterId}&characterRevision=${item.character.revision}`}
        >
          选择音色并试听
        </Link>
        {!!item.profile && (
          <button className="text-button" onClick={() => exportProfile(item)}>
            导出固定版本
          </button>
        )}
        {!!item.profile?.referenceAudio && (
          <Link
            className="text-button"
            to={`/admin/voice-references?characterId=${item.character.characterId}&characterRevision=${item.character.revision}&voiceRevision=${item.voiceRevision}`}
          >
            参考录音交付
          </Link>
        )}
        {!historical && item.voiceRevision > 1 && (
          <details>
            <summary>历史声音版本</summary>
            {Array.from(
              { length: Math.min(item.voiceRevision - 1, 20) },
              (_, i) => item.voiceRevision - i - 1,
            ).map((v) => (
              <Link
                key={v}
                className="text-button"
                to={`/admin/characters?characterId=${item.character.characterId}&characterRevision=${item.character.revision}&voiceRevision=${v}`}
              >
                声音 v{v}
              </Link>
            ))}
          </details>
        )}
      </article>
    );
  }
  return (
    <section className="settings-page page-arrive">
      <Link className="text-button" to="/admin">
        管理员后台
      </Link>
      <h1>角色库</h1>
      <CharacterEditor />
      <button
        className="text-button"
        onClick={() =>
          exportProfile(loaderData.items.filter((item) => item.profile))
        }
      >
        导出本页声音档案
      </button>
      <p>姓名、头像与配音各自保留固定版本，已发布课程不会随修改改变。</p>
      <p role="status">{notice}</p>
      {loaderData.selected && (
        <>
          <h2>历史档案</h2>
          {card(loaderData.selected, true)}
          <Link to="/admin/characters">返回角色库</Link>
        </>
      )}
      <div className="character-profiles">
        {loaderData.items.map((item) => card(item))}
      </div>
      {!loaderData.items.length && (
        <p>尚未登记角色。选择已登记的头像，新建第一个角色。</p>
      )}
      {loaderData.nextId && (
        <Link to={`/admin/characters?afterId=${loaderData.nextId}`}>
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
          setError("");
          setReason("");
        }}
      >
        <form onSubmit={save}>
          <h2 id={dialogTitleId}>{target?.character.displayName} · 声音档案</h2>
          <label>
            个性特点
            <textarea
              required
              maxLength={600}
              value={profile.personality}
              onChange={(e) => field("personality", e.target.value)}
            />
          </label>
          <label>
            说话习惯（生成指令）
            <textarea
              required
              maxLength={600}
              value={profile.speakingStyle}
              onChange={(e) => field("speakingStyle", e.target.value)}
            />
          </label>
          <label>
            默认情绪（生成指令）
            <textarea
              required
              maxLength={600}
              value={profile.defaultEmotion}
              onChange={(e) => field("defaultEmotion", e.target.value)}
            />
          </label>
          <label>
            提供方
            <input
              required
              value={profile.provider}
              onChange={(e) => field("provider", e.target.value)}
              maxLength={200}
            />
          </label>
          <label>
            绑定模型
            <input
              required
              value={profile.model}
              onChange={(e) => field("model", e.target.value)}
              maxLength={200}
            />
          </label>
          <label>
            音色 ID
            <input
              required
              value={profile.voiceId}
              onChange={(e) => field("voiceId", e.target.value)}
              maxLength={200}
            />
          </label>
          <fieldset>
            <legend>音色类型</legend>
            {(["system", "cloned"] as const).map((kind) => (
              <label key={kind}>
                <input
                  type="radio"
                  name="voice-kind"
                  checked={profile.voiceKind === kind}
                  onChange={() => field("voiceKind", kind)}
                />
                {kind === "system" ? "固定音色" : "复刻音色"}
              </label>
            ))}
          </fieldset>
          <label>
            语速
            <input
              required
              type="number"
              min="0.5"
              max="2"
              step="0.05"
              value={profile.rate}
              onChange={(e) => field("rate", Number(e.target.value))}
            />
          </label>
          <label>
            <input
              type="checkbox"
              checked={!!profile.referenceAudio}
              onChange={(e) =>
                field(
                  "referenceAudio",
                  e.target.checked
                    ? {
                        assetId: "",
                        revision: 1,
                        transcript: "",
                        cloningPermission: "",
                      }
                    : null,
                )
              }
            />
            关联参考录音
          </label>
          {target && profile.referenceAudio && (
            <fieldset>
              <legend>已登记的参考录音</legend>
              <ReferenceRecordingPicker
                value={profile.referenceAudio}
                pending={pending}
                onChange={(value) => field("referenceAudio", value)}
              />
              <label>
                录音 ID
                <input
                  required
                  readOnly
                  value={profile.referenceAudio.assetId}
                  onChange={(e) =>
                    field("referenceAudio", {
                      ...profile.referenceAudio!,
                      assetId: e.target.value,
                    })
                  }
                />
              </label>
              <label>
                录音版本
                <input
                  type="number"
                  required
                  readOnly
                  min="1"
                  step="1"
                  value={profile.referenceAudio.revision}
                  onChange={(e) =>
                    field("referenceAudio", {
                      ...profile.referenceAudio!,
                      revision: Number(e.target.value),
                    })
                  }
                />
              </label>
              <label>
                原文
                <textarea
                  required
                  name="referenceTranscript"
                  value={profile.referenceAudio.transcript}
                  onChange={(e) =>
                    field("referenceAudio", {
                      ...profile.referenceAudio!,
                      transcript: e.target.value,
                    })
                  }
                  maxLength={1000}
                />
              </label>
              <label>
                声音复刻授权依据
                <textarea
                  required
                  name="cloningPermission"
                  value={profile.referenceAudio.cloningPermission}
                  onChange={(e) =>
                    field("referenceAudio", {
                      ...profile.referenceAudio!,
                      cloningPermission: e.target.value,
                    })
                  }
                  maxLength={1000}
                />
              </label>
            </fieldset>
          )}
          <label>
            修改原因
            <input
              required
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              maxLength={300}
            />
          </label>
          <p role="alert">{error}</p>
          <button
            type="submit"
            className="primary"
            aria-busy={pending}
            aria-disabled={pending}
          >
            保存新版本
          </button>
          <button
            type="button"
            className="text-button"
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
