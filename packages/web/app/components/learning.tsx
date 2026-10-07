import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useLocation } from "react-router";
import { createPortal } from "react-dom";
import { Icon } from "./icon";
import type { UserProfile } from "@brioche/contracts/UserProfile";
import type { AccountProfile } from "@brioche/contracts/AccountProfile";
import type { UpdateProfileRequest } from "@brioche/contracts/UpdateProfileRequest";
import { ApiRequestError, privateRequest } from "../lib/api.client";
import { clearLearningDrafts } from "../lib/learning-draft";
import {
  RecordingPlayer,
  continuousRecording,
  type RecordingClip,
  type SpeechUnit,
} from "../lib/recording-playback";
export type ProfileChanges = Partial<
  Omit<UpdateProfileRequest, "version" | "displayName">
>;
type PlayerState = {
  status: "idle" | "loading" | "playing" | "paused";
  id: string | null;
  progress: number;
  owner?: string | null;
  wordId?: string | null;
};
type Learning = {
  profile: UserProfile | null;
  saveProfile: (changes: ProfileChanges) => Promise<boolean>;
  saveStatus: "idle" | "saving" | "saved" | "error";
  saveError: string;
  readAccount: () => Promise<AccountProfile | null>;
  saveAccount: (
    name: string,
    expectedAccountVersion: number,
  ) => Promise<boolean>;
  accountError: string;
  accountProfile: AccountProfile | null;
  translation: boolean;
  setTranslation: (v: boolean) => void;
  rate: number;
  setRate: (v: number) => void;
  openRate: () => void;
  play: (units: SpeechUnit[]) => void;
  toggle: (units: SpeechUnit[]) => void;
  stop: () => void;
  player: PlayerState;
  toast: (message: string) => void;
};
const Context = createContext<Learning | null>(null);
export function useLearning() {
  const value = useContext(Context);
  if (!value) throw Error("LearningProvider required");
  return value;
}
export function LearningProvider({
  children,
  user = null,
}: {
  children: ReactNode;
  user?: UserProfile | null;
}) {
  const [profile, setProfile] = useState<UserProfile | null>(user),
    [saveStatus, setSaveStatus] = useState<Learning["saveStatus"]>("idle"),
    [saveError, setSaveError] = useState(""),
    [translation, setTranslationState] = useState(
      user?.settings.showTranslation ?? false,
    ),
    [rate, setRateState] = useState(user?.settings.speechRate ?? 1),
    [player, setPlayer] = useState<PlayerState>({
      status: "idle",
      id: null,
      progress: 0,
    }),
    [message, setMessage] = useState(""),
    [messageSequence, setMessageSequence] = useState(0),
    [toastHost, setToastHost] = useState<HTMLDialogElement | null>(null);
  function notify(value: string) {
    setMessage(value);
    setMessageSequence((sequence) => sequence + 1);
  }
  const savedProfile = useRef(user),
    pendingChanges = useRef(new Map<symbol, ProfileChanges>()),
    saves = useRef<Promise<boolean>>(Promise.resolve(true)),
    saveGeneration = useRef(0);
  const account = useRef<AccountProfile | null>(null);
  const identityGeneration = useRef(0);
  const [accountProfile, setAccountProfile] = useState<AccountProfile | null>(
    null,
  );
  const requests = useRef(new Set<AbortController>());
  const [accountError, setAccountError] = useState("");
  function acceptAccount(value: AccountProfile) {
    if (value.id !== savedProfile.current?.id) throw Error("Account changed");
    account.current = value;
    setAccountProfile(value);
    // Account roles and versions must never replace product membership/settings.
    acceptProfile({
      ...savedProfile.current,
      displayName: value.displayName,
      email: value.email,
    });
  }
  async function readAccount(): Promise<AccountProfile | null> {
    if (!savedProfile.current) return null;
    const gen = identityGeneration.current;
    const request = new AbortController();
    requests.current.add(request);
    setAccountError("");
    try {
      const value = await privateRequest<AccountProfile>(
        "/api/v1/account",
        "GET",
        undefined,
        request.signal,
      );
      if (request.signal.aborted || gen !== identityGeneration.current)
        return null;
      acceptAccount(value);
      return value;
    } catch (error) {
      if (request.signal.aborted || gen !== identityGeneration.current)
        return null;
      if (
        error instanceof ApiRequestError &&
        error.phase === "request" &&
        error.status === 401
      )
        acceptProfile(null);
      setAccountError(
        error instanceof ApiRequestError
          ? error.message
          : "账号资料读取失败，请重试。",
      );
      return null;
    } finally {
      requests.current.delete(request);
    }
  }
  async function saveAccount(
    name: string,
    expectedAccountVersion: number,
  ): Promise<boolean> {
    if (!savedProfile.current) return false;
    const gen = identityGeneration.current;
    const request = new AbortController();
    requests.current.add(request);
    setAccountError("");
    try {
      const value = await privateRequest<AccountProfile>(
        "/api/v1/account",
        "PATCH",
        { displayName: name, expectedAccountVersion },
        request.signal,
      );
      if (request.signal.aborted || gen !== identityGeneration.current)
        return false;
      acceptAccount(value);
      return true;
    } catch (error) {
      if (request.signal.aborted || gen !== identityGeneration.current)
        return false;
      // Recover the authoritative account version without retrying the write.
      if (
        error instanceof ApiRequestError &&
        error.phase === "request" &&
        error.status === 401
      )
        acceptProfile(null);
      else {
        try {
          const latest = await privateRequest<AccountProfile>(
            "/api/v1/account",
            "GET",
            undefined,
            request.signal,
          );
          if (request.signal.aborted || gen !== identityGeneration.current)
            return false;
          acceptAccount(latest);
        } catch (recoveryError) {
          if (request.signal.aborted || gen !== identityGeneration.current)
            return false;
          if (
            recoveryError instanceof ApiRequestError &&
            recoveryError.phase === "request" &&
            recoveryError.status === 401
          ) {
            acceptProfile(null);
            error = recoveryError;
          }
        }
      }
      const message =
        error instanceof ApiRequestError
          ? error.message
          : "保存未确认，请检查账号资料后重试。";
      setAccountError(message);
      notify(message);
      return false;
    } finally {
      requests.current.delete(request);
    }
  }
  const rateRef = useRef(user?.settings.speechRate ?? 1),
    state = useRef(player),
    queue = useRef<SpeechUnit[]>([]),
    index = useRef(0),
    generation = useRef(0),
    dialog = useRef<HTMLDialogElement>(null);
  const location = useLocation();
  const recording = useRef<RecordingPlayer | null>(null);
  function acceptProfile(value: UserProfile | null) {
    if (value && account.current?.id === value.id)
      value = {
        ...value,
        displayName: account.current.displayName,
        email: account.current.email,
      };
    if (savedProfile.current && savedProfile.current.id !== value?.id) {
      // Losing a session also invalidates writes waiting for CSRF or a response.
      saveGeneration.current++;
      identityGeneration.current++;
      for (const request of requests.current) request.abort();
      requests.current.clear();
      pendingChanges.current.clear();
      saves.current = Promise.resolve(true);
      account.current = null;
      setAccountProfile(null);
      stop();
      clearLearningDrafts(savedProfile.current.id);
    }
    savedProfile.current = value;
    setProfile(value);
    let show = value?.settings.showTranslation ?? false,
      speed = value?.settings.speechRate ?? 1;
    for (const changes of pendingChanges.current.values()) {
      if (changes.showTranslation != null) show = changes.showTranslation;
      if (changes.speechRate != null) speed = changes.speechRate;
    }
    setTranslationState(show);
    applyRate(speed);
  }
  useLayoutEffect(() => {
    if (
      savedProfile.current?.id === user?.id &&
      (savedProfile.current?.version ?? 0) > (user?.version ?? 0)
    )
      return;
    saveGeneration.current++;
    pendingChanges.current.clear();
    // Requests from an obsolete identity/version must not block the current scope.
    saves.current = Promise.resolve(true);
    setSaveStatus("idle");
    setSaveError("");
    acceptProfile(user);
  }, [user?.id, user?.version]);
  useLayoutEffect(() => {
    identityGeneration.current++;
    for (const request of requests.current) request.abort();
    requests.current.clear();
    account.current = null;
    setAccountProfile(null);
    setAccountError("");
    return () => {
      identityGeneration.current++;
      for (const request of requests.current) request.abort();
      requests.current.clear();
    };
  }, [user?.id]);
  useLayoutEffect(
    () => () => {
      saveGeneration.current++;
      for (const request of requests.current) request.abort();
      requests.current.clear();
    },
    [],
  );
  function saveProfile(changes: ProfileChanges): Promise<boolean> {
    if (!savedProfile.current) return Promise.resolve(false);
    const gen = saveGeneration.current;
    const job = Symbol();
    pendingChanges.current.set(job, changes);
    setSaveStatus("saving");
    setSaveError("");
    const next = saves.current.then(async () => {
      if (gen !== saveGeneration.current || !savedProfile.current) return false;
      const request = new AbortController();
      requests.current.add(request);
      try {
        const value = await privateRequest<UserProfile>(
          "/api/v1/me/settings",
          "PATCH",
          { ...changes, version: savedProfile.current.version },
          request.signal,
        );
        if (gen !== saveGeneration.current) return false;
        pendingChanges.current.delete(job);
        acceptProfile(value);
        return true;
      } catch (error) {
        if (gen !== saveGeneration.current) return false;
        const recoveryGeneration = ++saveGeneration.current;
        pendingChanges.current.clear();
        if (
          error instanceof ApiRequestError &&
          error.phase === "request" &&
          error.status === 401
        )
          acceptProfile(null);
        else {
          // A timed-out response may already have saved. Read the current state; never resend a write automatically.
          try {
            const latest = await privateRequest<UserProfile>(
              "/api/v1/me",
              "GET",
              undefined,
              request.signal,
            );
            if (recoveryGeneration !== saveGeneration.current) return false;
            acceptProfile(latest);
          } catch (recoveryError) {
            if (recoveryGeneration !== saveGeneration.current) return false;
            if (
              recoveryError instanceof ApiRequestError &&
              recoveryError.phase === "request" &&
              recoveryError.status === 401
            ) {
              acceptProfile(null);
              error = recoveryError;
            } else acceptProfile(savedProfile.current);
          }
        }
        setSaveStatus("error");
        const message =
          error instanceof ApiRequestError
            ? error.message
            : "保存未确认，请检查当前设置后重试。";
        setSaveError(message);
        notify(message);
        return false;
      } finally {
        requests.current.delete(request);
      }
    });
    saves.current = next;
    void next.then((ok) => {
      if (gen !== saveGeneration.current) return;
      pendingChanges.current.delete(job);
      if (!pendingChanges.current.size && ok) setSaveStatus("saved");
    });
    return next;
  }
  function setTranslation(value: boolean) {
    setTranslationState(value);
    if (savedProfile.current) void saveProfile({ showTranslation: value });
  }
  function update(value: PlayerState) {
    value = {
      ...value,
      owner: queue.current[0]?.id ?? null,
      wordId: value.wordId ?? (value.id?.includes(":word:") ? value.id : null),
    };
    state.current = value;
    setPlayer(value);
  }
  function stop(keepRecording = false) {
    generation.current++;
    recording.current?.stop(!keepRecording);
    queue.current = [];
    update({ status: "idle", id: null, progress: 0 });
  }
  function playCurrent() {
    const unit = queue.current[index.current];
    if (!unit) {
      update({ status: "idle", id: null, progress: 1 });
      return;
    }
    if (unit.recording) {
      playRecording(unit.recording, false);
      return;
    }
    stop();
    notify("这段录音还在准备中。");
  }
  function playRecording(clip: RecordingClip, whole: boolean) {
    const gen = generation.current;
    recording.current ??= new RecordingPlayer();
    update({
      status: "loading",
      id: queue.current[index.current]?.id ?? null,
      progress: whole ? 0 : index.current / queue.current.length,
    });
    recording.current.play(clip, rateRef.current, {
      status: (status) => {
        if (gen === generation.current) update({ ...state.current, status });
      },
      progress: (fraction, id, wordId) => {
        if (gen !== generation.current) return;
        if (whole && id) {
          const activeIndex = queue.current.findIndex((unit) => unit.id === id);
          if (activeIndex >= 0) index.current = activeIndex;
        }
        update({
          status: state.current.status === "loading" ? "loading" : "playing",
          id,
          wordId,
          progress: whole
            ? fraction
            : (index.current + fraction) / queue.current.length,
        });
      },
      end: () => {
        if (gen !== generation.current) return;
        index.current = whole ? queue.current.length : index.current + 1;
        playCurrent();
      },
      error: (blocked) => {
        if (gen !== generation.current) return;
        stop();
        notify(
          blocked
            ? "请再次点击播放，允许浏览器播放录音。"
            : "录音暂时无法播放，请重试。",
        );
      },
    });
  }
  function play(units: SpeechUnit[]) {
    stop(true);
    // Do not play a partial sequence or fall back to a different synthesized voice.
    if (units.some((unit) => !unit.recording)) {
      stop();
      notify("这段录音还在准备中。");
      return;
    }
    queue.current = units;
    index.current = 0;
    const clip = units.length > 1 ? continuousRecording(units) : null;
    if (clip) playRecording(clip, true);
    else playCurrent();
  }
  function toggle(units: SpeechUnit[]) {
    if (
      (state.current.owner || state.current.id) &&
      !units.some(
        (unit) => unit.id === (state.current.owner || state.current.id),
      )
    ) {
      play(units);
      return;
    }
    if (
      state.current.status === "playing" ||
      state.current.status === "loading"
    ) {
      recording.current?.pause();
      update({ ...state.current, status: "paused" });
    } else if (state.current.status === "paused") {
      recording.current?.resume();
    } else play(units);
  }
  function applyRate(value: number) {
    if (rateRef.current === value) return;
    rateRef.current = value;
    setRateState(value);
    recording.current?.setRate(value);
  }
  function setRate(value: number) {
    applyRate(value);
    if (savedProfile.current) void saveProfile({ speechRate: value });
  }
  useEffect(() => {
    stop();
    dialog.current?.close();
    setMessage("");
    setToastHost(null);
  }, [location.pathname, location.search, user?.id]);
  useEffect(() => {
    const leave = () => stop();
    window.addEventListener("pagehide", leave);
    return () => {
      window.removeEventListener("pagehide", leave);
      generation.current++;
      recording.current?.stop();
    };
  }, []);
  useEffect(() => {
    if (!message) {
      setToastHost(null);
      return;
    }
    const updateHost = () =>
      setToastHost(
        Array.from(
          document.querySelectorAll<HTMLDialogElement>("dialog[open]"),
        ).at(-1) ?? null,
      );
    updateHost();
    const observer = new MutationObserver(updateHost);
    observer.observe(document.body, {
      subtree: true,
      attributes: true,
      attributeFilter: ["open"],
    });
    const timer = setTimeout(() => setMessage(""), 5500);
    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, [message, messageSequence]);
  const toast = (
    <div className="toast" hidden={!message}>
      <span role="status" aria-live="polite">
        {message}
      </span>
      <button
        className="toast-close"
        aria-label="关闭提示"
        onClick={() => setMessage("")}
      >
        <Icon name="close" />
      </button>
    </div>
  );
  return (
    <Context.Provider
      value={{
        profile,
        saveProfile,
        saveStatus,
        saveError,
        readAccount,
        saveAccount,
        accountError,
        accountProfile,
        translation,
        setTranslation,
        rate,
        setRate,
        openRate: () => {
          dialog.current?.showModal();
          dialog.current
            ?.querySelector<HTMLButtonElement>('[aria-checked="true"]')
            ?.focus();
        },
        play,
        toggle,
        stop,
        player,
        toast: notify,
      }}
    >
      {children}
      <dialog
        ref={dialog}
        id="reading-rate-dialog"
        className="rate-dialog"
        aria-labelledby="speed-title"
        onClick={(e) => {
          if (e.target === dialog.current) {
            const r = dialog.current.getBoundingClientRect();
            if (
              e.clientX < r.left ||
              e.clientX > r.right ||
              e.clientY < r.top ||
              e.clientY > r.bottom
            )
              dialog.current.close();
          }
        }}
      >
        <div className="rate-heading">
          <h2 id="speed-title">朗读速度</h2>
          <button
            className="icon-button"
            aria-label="关闭速度设置"
            onClick={() => dialog.current?.close()}
          >
            <Icon name="close" />
          </button>
        </div>
        <div className="rate-options" role="radiogroup" aria-label="朗读速度">
          {[0.75, 1, 1.25, 1.5].map((value) => (
            <button
              key={value}
              className="rate-option"
              role="radio"
              aria-checked={rate === value}
              tabIndex={rate === value ? 0 : -1}
              onKeyDown={(event) => {
                const values = [0.75, 1, 1.25, 1.5];
                let next: number | undefined;
                if (event.key === "ArrowDown" || event.key === "ArrowRight")
                  next = values[(values.indexOf(value) + 1) % values.length];
                if (event.key === "ArrowUp" || event.key === "ArrowLeft")
                  next =
                    values[
                      (values.indexOf(value) + values.length - 1) %
                        values.length
                    ];
                if (event.key === "Home") next = values[0];
                if (event.key === "End") next = values.at(-1);
                if (next !== undefined) {
                  event.preventDefault();
                  setRate(next);
                  const buttons =
                    dialog.current?.querySelectorAll<HTMLButtonElement>(
                      '[role="radio"]',
                    );
                  buttons?.[values.indexOf(next)]?.focus();
                }
              }}
              onClick={() => {
                setRate(value);
                dialog.current?.close();
              }}
            >
              {value}×<Icon name="check" />
            </button>
          ))}
        </div>
      </dialog>
      {toastHost ? createPortal(toast, toastHost) : toast}
    </Context.Provider>
  );
}
export function Player({ units }: { units: SpeechUnit[] }) {
  const learning = useLearning(),
    hold = useRef<ReturnType<typeof setTimeout> | null>(null),
    long = useRef(false),
    start = useRef({ x: 0, y: 0 });
  const ownsPlayback = units.some(
    (unit) =>
      unit.id === learning.player.id || unit.id === learning.player.owner,
  );
  const progress = ownsPlayback ? learning.player.progress : 0;
  const playing =
    ownsPlayback &&
    (learning.player.status === "playing" ||
      learning.player.status === "loading");
  const cancel = () => {
    if (hold.current) clearTimeout(hold.current);
    hold.current = null;
  };
  useEffect(() => cancel, []);
  return (
    <div className="reader-player">
      <button
        className="playback-line"
        aria-label={playing ? "暂停朗读" : "播放全文"}
        onPointerDown={(e) => {
          long.current = false;
          start.current = { x: e.clientX, y: e.clientY };
          cancel();
          hold.current = setTimeout(() => {
            long.current = true;
            learning.openRate();
          }, 550);
        }}
        onPointerMove={(e) => {
          if (
            Math.hypot(
              e.clientX - start.current.x,
              e.clientY - start.current.y,
            ) > 10
          )
            cancel();
        }}
        onPointerUp={cancel}
        onPointerCancel={cancel}
        onPointerLeave={cancel}
        onContextMenu={(e) => {
          e.preventDefault();
          cancel();
          long.current = true;
          learning.openRate();
        }}
        onKeyDown={(e) => {
          if (e.shiftKey && e.key === "F10") {
            e.preventDefault();
            learning.openRate();
          }
        }}
        onClick={(event) => {
          cancel();
          const held = long.current;
          long.current = false;
          if (held && event.detail > 0) return;
          learning.toggle(units);
        }}
      >
        <span className="playback-track">
          <span
            style={{
              display: "block",
              height: 2,
              background: "var(--accent)",
              width: progress * 100 + "%",
            }}
          />
          <span
            className="play-marker"
            style={{
              left: `${Math.max(2, Math.min(98, progress ? progress * 100 : 50))}%`,
            }}
          >
            <Icon name={playing ? "pause" : "play"} />
          </span>
        </span>
      </button>
    </div>
  );
}
