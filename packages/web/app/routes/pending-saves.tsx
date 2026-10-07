import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, redirect } from "react-router";
import { getIdentity } from "../lib/api.server";
import {
  ApiRequestError,
  definitiveWriteFailure,
  privateRequest,
} from "../lib/api.client";
import { clearPending, draftsChangedEvent } from "../lib/learning-draft";
import { pendingOwned } from "../lib/owned-draft";
import { Icon } from "../components/icon";
import { PendingNavigation } from "../components/pending-navigation";
import { usePendingOwnedWrites } from "../components/pending-owned-writes";
import type { Route } from "./+types/pending-saves";
export async function loader({ request }: Route.LoaderArgs) {
  const identity = await getIdentity(request);
  if (!identity.user) throw redirect("/login?next=/pending-saves");
  return { userId: identity.user.id };
}
export default function PendingSaves({
  loaderData: { userId },
}: Route.ComponentProps) {
  return <PendingList key={userId} userId={userId} />;
}
function PendingList({ userId }: { userId: string }) {
  const [items, setItems] = useState<ReturnType<typeof pendingOwned>>([]),
    [ready, setReady] = useState(false),
    [busy, setBusy] = useState(""),
    [error, setError] = useState("");
  const buttons = useRef(new Map<string, HTMLButtonElement>()),
    empty = useRef<HTMLParagraphElement>(null),
    heading = useRef<HTMLHeadingElement>(null),
    pendingFocus = useRef<string | null | undefined>(undefined),
    currentItems = useRef<ReturnType<typeof pendingOwned>>([]),
    alive = useRef(true),
    writing = useRef(false);
  const hasPending = usePendingOwnedWrites(userId);
  useLayoutEffect(() => {
    if (pendingFocus.current === undefined) return;
    const next = pendingFocus.current;
    pendingFocus.current = undefined;
    const target =
      (next && buttons.current.get(next)) ||
      buttons.current.values().next().value ||
      empty.current;
    target?.focus();
  }, [items]);
  function sync() {
    const previous = currentItems.current,
      next = pendingOwned(userId),
      nextKeys = new Set(next.map((entry) => entry.key)),
      focusedKey = [...buttons.current].find(
        ([, button]) => button === document.activeElement,
      )?.[0];
    if (focusedKey && !nextKeys.has(focusedKey)) {
      const index = previous.findIndex((entry) => entry.key === focusedKey);
      pendingFocus.current =
        previous.slice(index + 1).find((entry) => nextKeys.has(entry.key))
          ?.key ??
        previous
          .slice(0, index)
          .reverse()
          .find((entry) => nextKeys.has(entry.key))?.key ??
        next[0]?.key ??
        null;
    }
    currentItems.current = next;
    setItems(next);
    if (!next.length) setError("");
  }
  useEffect(() => {
    alive.current = true;
    sync();
    setReady(true);
    window.addEventListener(draftsChangedEvent, sync);
    window.addEventListener("storage", sync);
    return () => {
      alive.current = false;
      window.removeEventListener(draftsChangedEvent, sync);
      window.removeEventListener("storage", sync);
    };
  }, [userId]);
  async function confirm(item: (typeof items)[number]) {
    if (writing.current || !alive.current) return;
    // Another writer may already have confirmed this row before React commits.
    const current = pendingOwned(userId).find(
      (entry) => entry.key === item.key,
    );
    if (!current || JSON.stringify(current.job) !== JSON.stringify(item.job)) {
      sync();
      return;
    }
    writing.current = true;
    setBusy(item.key);
    setError("");
    try {
      await privateRequest(item.job.path, item.job.method, item.job.body);
      clearPending(item.key, item.job.body.idempotencyKey);
    } catch (failure) {
      if (
        failure instanceof ApiRequestError &&
        definitiveWriteFailure(failure)
      ) {
        clearPending(item.key, item.job.body.idempotencyKey);
        if (alive.current)
          setError(
            failure.status === 409
              ? "记录已在其他地方更新，请回到原页面检查后继续。"
              : failure.message,
          );
      } else if (alive.current)
        setError("保存仍未确认，可以稍后重试同一请求。");
    } finally {
      writing.current = false;
      if (alive.current) setBusy("");
    }
  }
  return (
    <section className="settings-page page-arrive">
      <PendingNavigation
        active={hasPending}
        onStay={() => heading.current?.focus({ preventScroll: true })}
      />
      <div className="section-head">
        <h1 ref={heading} tabIndex={-1}>
          未确认保存
        </h1>
        <Link className="text-button" to="/profile">
          回到我的
        </Link>
      </div>
      <p className="profile-note">此标签页中尚未确认的收藏和复习操作。</p>
      {!ready ? (
        <p role="status">正在读取</p>
      ) : !items.length ? (
        <p ref={empty} tabIndex={-1} role="status">
          没有待确认的保存。
        </p>
      ) : (
        <div className="library-list">
          {items.map((item) => (
            <div key={item.key} className="library-entry">
              <div className="section-head">
                <span>
                  {item.target.kind === "bookmark"
                    ? item.job.body.saved
                      ? "收藏表达"
                      : "取消收藏"
                    : item.target.kind === "enroll"
                      ? "加入复习"
                      : item.target.kind === "preference"
                        ? "调整复习状态"
                        : "复习自评"}
                </span>
                <button
                  ref={(element) => {
                    if (element) buttons.current.set(item.key, element);
                    else buttons.current.delete(item.key);
                  }}
                  className="text-button"
                  type="button"
                  aria-disabled={!!busy}
                  aria-busy={busy === item.key}
                  onClick={() => void confirm(item)}
                >
                  {busy === item.key ? "正在确认" : "确认原提交"}
                  <Icon name="check" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      {error && (
        <p className="error-message" role="alert">
          {error}
        </p>
      )}
      <Link className="text-button practice-back" to="/library">
        查看收藏与复习
      </Link>
    </section>
  );
}
