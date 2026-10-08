import { productNamespace } from "../lib/product-runtime";
import { useLearning } from "../components/learning";
import product from "@chef/product";
import { Icon } from "../components/icon";
import { Link, useBlocker, useRouteLoaderData } from "react-router";
import type { loader } from "../root";
import { authRequest } from "../lib/auth.client";
import { clearLearningDrafts } from "../lib/learning-draft";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ChoiceDialog, type Choice } from "../components/choice-dialog";
const commonZones: Choice[] = [
  { value: "Asia/Shanghai", label: "中国", detail: "Asia/Shanghai" },
  { value: "Asia/Hong_Kong", label: "香港", detail: "Asia/Hong_Kong" },
  { value: "Asia/Taipei", label: "台北", detail: "Asia/Taipei" },
  { value: "Europe/Paris", label: "法国 · 巴黎", detail: "Europe/Paris" },
  { value: "Europe/London", label: "英国 · 伦敦", detail: "Europe/London" },
  { value: "America/New_York", label: "纽约", detail: "America/New_York" },
  { value: "Asia/Tokyo", label: "东京", detail: "Asia/Tokyo" },
  { value: "UTC", label: "UTC" },
];
export default function Profile() {
  const { profile } = useLearning();
  // Drafts, modal refs and pending writes belong to one identity. A late
  // completion from the previous editor must never close or lock the new one.
  return <ProfileContent key={profile?.id ?? "visitor"} />;
}
function ProfileContent() {
  const learning = useLearning();
  const identity = useRouteLoaderData<typeof loader>("root");
  const [pending, setPending] = useState(false);
  const logoutBusy = useRef(false);
  const logoutRequest = useRef<AbortController | null>(null);
  const alive = useRef(true);
  useLayoutEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      logoutRequest.current?.abort();
      logoutRequest.current = null;
    };
  }, []);
  const profile = learning.profile;
  const editor = useRef<HTMLDialogElement>(null);
  const editBusy = useRef(false);
  const baseline = useRef({ name: "", zone: "", days: 5, minutes: 10 });
  const discardHeading = useRef<HTMLHeadingElement>(null);
  const retainedName = useRef<HTMLInputElement>(null);
  const wasDiscarding = useRef(false);
  const [editorOpen, setEditorOpen] = useState(false),
    [discarding, setDiscarding] = useState(false);
  const [editMode, setEditMode] = useState<"account" | "study">("account");
  const [loadingAccount, setLoadingAccount] = useState(false);
  const accountReadBusy = useRef(false);
  const editorError =
    editMode === "account" ? learning.accountError : learning.saveError;
  const saveFailure = useRef<HTMLParagraphElement>(null);
  const [failureSequence, setFailureSequence] = useState(0);
  const [draftName, setDraftName] = useState(""),
    [zone, setZone] = useState("Asia/Shanghai"),
    [days, setDays] = useState(5),
    [minutes, setMinutes] = useState(10),
    [editing, setEditing] = useState(false),
    [zones, setZones] = useState(commonZones);
  const dirty =
    draftName !== baseline.current.name ||
    zone !== baseline.current.zone ||
    days !== baseline.current.days ||
    minutes !== baseline.current.minutes;
  const blocker = useBlocker(() => editorOpen && (dirty || editBusy.current));
  useEffect(() => {
    if (blocker.state !== "blocked" || editing) return;
    if (!editorOpen) blocker.proceed();
    else setDiscarding(true);
  }, [blocker, editorOpen, editing]);
  function keepEditing() {
    if (blocker.state === "blocked") blocker.reset();
    setDiscarding(false);
  }
  function discardChanges() {
    if (blocker.state === "blocked") blocker.proceed();
    else editor.current?.close();
  }
  useEffect(() => {
    if (!editorOpen || (!dirty && !editing)) return;
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [editorOpen, dirty, editing]);
  useLayoutEffect(() => {
    if (discarding) discardHeading.current?.focus();
    else if (wasDiscarding.current) {
      if (retainedName.current) retainedName.current.focus();
      else
        editor.current
          ?.querySelector<HTMLButtonElement>("form button")
          ?.focus();
    }
    wasDiscarding.current = discarding;
  }, [discarding]);
  function requestClose() {
    if (editBusy.current) return;
    if (discarding) keepEditing();
    else if (dirty) setDiscarding(true);
    else editor.current?.close();
  }
  useEffect(() => {
    const known = new Set(commonZones.map((choice) => choice.value));
    const all =
      typeof Intl.supportedValuesOf === "function"
        ? Intl.supportedValuesOf("timeZone")
        : [];
    setZones([
      ...commonZones,
      ...all
        .filter((value) => !known.has(value))
        .map((value) => ({ value, label: value })),
    ]);
  }, []);
  useLayoutEffect(() => {
    if (failureSequence && editor.current?.open) saveFailure.current?.focus();
  }, [failureSequence]);
  useLayoutEffect(() => {
    if (editorOpen)
      editor.current
        ?.querySelector<HTMLElement>("form input, form button")
        ?.focus();
  }, [editorOpen, editMode]);
  async function openEditor(mode: "account" | "study") {
    if (!profile) return;
    if (accountReadBusy.current) return;
    let name = profile.displayName;
    if (mode === "account") {
      accountReadBusy.current = true;
      setLoadingAccount(true);
      const account = await learning.readAccount();
      if (!alive.current) return;
      accountReadBusy.current = false;
      setLoadingAccount(false);
      if (!account) return;
      name = account.displayName;
    }
    setEditMode(mode);
    baseline.current = {
      name,
      zone: profile.settings.timeZone,
      days: profile.settings.weeklyDays,
      minutes: profile.settings.dailyMinutes,
    };
    setDiscarding(false);
    setEditorOpen(true);
    setDraftName(name);
    setZone(profile.settings.timeZone);
    setDays(profile.settings.weeklyDays);
    setMinutes(profile.settings.dailyMinutes);
    editor.current?.showModal();
  }
  async function save() {
    if (editBusy.current || !profile) return;
    const changes = {
      ...(zone !== profile.settings.timeZone ? { timeZone: zone } : {}),
      ...(days !== profile.settings.weeklyDays ? { weeklyDays: days } : {}),
      ...(minutes !== profile.settings.dailyMinutes
        ? { dailyMinutes: minutes }
        : {}),
    };
    if (
      editMode === "account"
        ? draftName.trim() === profile.displayName
        : !Object.keys(changes).length
    ) {
      editor.current?.close();
      return;
    }
    setEditing(true);
    editBusy.current = true;
    const ok =
      editMode === "account"
        ? learning.accountProfile &&
          (await learning.saveAccount(
            draftName.trim(),
            learning.accountProfile.version,
          ))
        : await learning.saveProfile(changes);
    if (!alive.current) return;
    if (ok) {
      setEditorOpen(false);
      editor.current?.close();
    } else setFailureSequence((sequence) => sequence + 1);
    setEditing(false);
    editBusy.current = false;
  }
  async function logout() {
    if (logoutBusy.current || !profile || !alive.current) return;
    logoutBusy.current = true;
    const attempt = new AbortController();
    logoutRequest.current = attempt;
    setPending(true);
    try {
      await authRequest("logout", undefined, attempt.signal);
      if (
        !alive.current ||
        logoutRequest.current !== attempt ||
        attempt.signal.aborted
      )
        return;
      clearLearningDrafts(profile.id, productNamespace);
      learning.stop();
      window.location.assign("/");
    } catch {
      if (
        !alive.current ||
        logoutRequest.current !== attempt ||
        attempt.signal.aborted
      )
        return;
      logoutRequest.current = null;
      logoutBusy.current = false;
      learning.toast("退出未完成，请重试。");
      setPending(false);
    }
  }
  return (
    <section className="settings-page page-arrive">
      <div className="settings-heading">
        <h1>我的</h1>
      </div>
      <div className="profile-summary">
        <img src={product.avatar} alt="" />
        <div>
          <h2>{profile?.displayName ?? product.learnerLabel ?? "学习者"}</h2>
          <p>{profile?.email ?? product.tagline}</p>
          {product.courseLevelLabel && (
            <span className="profile-level">{product.courseLevelLabel}</span>
          )}
        </div>
        {profile && (
          <button
            className="icon-button profile-edit"
            aria-label="编辑个人资料"
            aria-busy={loadingAccount}
            onClick={() => void openEditor("account")}
          >
            <Icon name="chevron" />
          </button>
        )}
      </div>
      {learning.accountError && !editorOpen && (
        <p className="error-message" role="alert">
          {learning.accountError}
        </p>
      )}
      {profile && (
        <>
          <h2>学习日常</h2>
          <div className="settings-group">
            {profile.role === "operator" && (
              <Link className="setting-row setting-link" to="/admin">
                <span>管理员后台</span>
                <Icon name="chevron" />
              </Link>
            )}
            <Link className="setting-row setting-link" to="/reviews">
              <span>我的复习</span>
              <Icon name="chevron" />
            </Link>
            <Link className="setting-row setting-link" to="/library">
              <span>我的表达</span>
              <Icon name="chevron" />
            </Link>
            <Link className="setting-row setting-link" to="/review-history">
              <span>复习记录</span>
              <Icon name="chevron" />
            </Link>
            <button
              className="setting-row setting-link"
              onClick={() => void openEditor("study")}
            >
              <span>学习目标</span>
              <span>
                每周 {profile.settings.weeklyDays} 天 · 每天{" "}
                {profile.settings.dailyMinutes} 分钟
                <Icon name="chevron" />
              </span>
            </button>
            <button
              className="setting-row setting-link"
              onClick={() => void openEditor("study")}
            >
              <span>学习时区</span>
              <span>
                {zones.find(
                  (choice) => choice.value === profile.settings.timeZone,
                )?.label ?? profile.settings.timeZone}
                <Icon name="chevron" />
              </span>
            </button>
          </div>
        </>
      )}
      <h2>阅读</h2>
      <div className="settings-group">
        <div className="setting-row">
          <span id="translation-label">默认显示中文译文</span>
          <button
            className="translation-switch"
            role="switch"
            aria-checked={learning.translation}
            aria-labelledby="translation-label"
            onClick={() => learning.setTranslation(!learning.translation)}
          >
            <span className="switch-track" aria-hidden="true" />
          </button>
        </div>
        <div className="setting-row">
          <span>朗读速度</span>
          <button
            className="speed-trigger"
            aria-label={`朗读速度：${learning.rate}×`}
            aria-haspopup="dialog"
            aria-controls="reading-rate-dialog"
            onClick={learning.openRate}
          >
            {learning.rate}×<Icon name="chevron" />
          </button>
        </div>
      </div>
      {profile && (
        <Link className="text-button practice-back" to="/pending-saves">
          未确认的收藏与复习保存 <Icon name="chevron" />
        </Link>
      )}
      <p className="profile-note" role="status">
        {profile
          ? learning.saveStatus === "saving"
            ? "正在保存"
            : learning.saveStatus === "error"
              ? "保存未确认，请检查当前设置后重试。"
              : "设置已保存到账号。"
          : "阅读偏好暂时仅在本次浏览中保留。"}
      </p>
      {profile ? (
        <button
          className="text-button"
          aria-disabled={pending}
          aria-busy={pending}
          onClick={() => void logout()}
        >
          {pending ? "正在退出" : "退出登录"}
        </button>
      ) : (
        identity?.enabled && (
          <Link className="primary" to="/login">
            登录账号
          </Link>
        )
      )}
      <dialog
        ref={editor}
        className="profile-dialog"
        aria-labelledby="profile-edit-title"
        onCancel={(event) => {
          event.preventDefault();
          requestClose();
        }}
        onClose={() => {
          setEditorOpen(false);
          setDiscarding(false);
        }}
      >
        <div className="rate-heading">
          <h2 id="profile-edit-title">
            {editMode === "account" ? "个人资料" : "学习日常"}
          </h2>
          <button
            type="button"
            className="icon-button"
            disabled={editing}
            aria-label="关闭个人资料"
            onClick={requestClose}
          >
            <Icon name="close" />
          </button>
        </div>
        {blocker.state === "blocked" && editing && (
          <p className="profile-leave-status" role="status">
            正在保存，完成后将离开。
          </p>
        )}
        {discarding && (
          <div className="profile-discard">
            <h3 ref={discardHeading} tabIndex={-1}>
              放弃这些修改？
            </h3>
            <p>
              {editorError
                ? "上次保存未获确认。离开将丢弃当前编辑草稿。"
                : editMode === "account"
                  ? "昵称的修改尚未保存。"
                  : "学习目标和时区的修改尚未保存。"}
            </p>
            <button type="button" className="primary" onClick={keepEditing}>
              继续编辑
            </button>
            <button
              type="button"
              className="text-button"
              onClick={discardChanges}
            >
              放弃修改
            </button>
          </div>
        )}
        <form
          hidden={discarding}
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          {editMode === "account" && (
            <label className="profile-field">
              怎么称呼你
              <input
                ref={retainedName}
                required
                maxLength={80}
                autoComplete="nickname"
                value={draftName}
                readOnly={editing}
                onChange={(event) => setDraftName(event.target.value)}
              />
            </label>
          )}
          {editMode === "study" && (
            <>
              <div className="setting-row">
                <span>学习时区</span>
                <ChoiceDialog
                  title="学习时区"
                  choices={zones}
                  value={zone}
                  onChange={setZone}
                  searchable
                  disabled={editing}
                />
              </div>
              <button
                className="text-button device-zone"
                type="button"
                disabled={editing}
                onClick={() =>
                  setZone(Intl.DateTimeFormat().resolvedOptions().timeZone)
                }
              >
                使用设备时区
              </button>
              <div className="setting-row">
                <span>每周学习</span>
                <ChoiceDialog
                  title="每周学习天数"
                  choices={[3, 5, 7].map((value) => ({
                    value: String(value),
                    label: `${value} 天`,
                  }))}
                  value={String(days)}
                  onChange={(value) => setDays(Number(value))}
                  disabled={editing}
                />
              </div>
              <div className="setting-row">
                <span>每天学习</span>
                <ChoiceDialog
                  title="每天学习时间"
                  choices={[5, 10, 15].map((value) => ({
                    value: String(value),
                    label: `${value} 分钟`,
                  }))}
                  value={String(minutes)}
                  onChange={(value) => setMinutes(Number(value))}
                  disabled={editing}
                />
              </div>
            </>
          )}
          {editorError && (
            <p
              ref={saveFailure}
              className="error-message"
              role="alert"
              tabIndex={-1}
            >
              {editorError}
            </p>
          )}
          <button
            type="submit"
            className="primary"
            aria-disabled={editing}
            aria-busy={editing}
          >
            {editing ? "正在保存" : "保存"}
            <Icon name="check" />
          </button>
        </form>
      </dialog>
    </section>
  );
}
