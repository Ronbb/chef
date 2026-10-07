import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, redirect } from "react-router";
import type { ReviewQueue } from "@brioche/contracts/ReviewQueue";
import type { ReviewRating } from "@brioche/contracts/ReviewRating";
import type { ReviewAttemptRequest } from "@brioche/contracts/ReviewAttemptRequest";
import type { ReviewAttemptResult } from "@brioche/contracts/ReviewAttemptResult";
import { getPrivate } from "../lib/api.server";
import { knowledgeUnit } from "../lib/recording-playback";
import {
  privateRequest,
  ApiRequestError,
  definitiveWriteFailure,
} from "../lib/api.client";
import { operationKey } from "../lib/operation-key";
import {
  clearPending,
  draftScope,
  readDraft,
  saveDraft,
} from "../lib/learning-draft";
import { validOwnedPending } from "../lib/owned-draft";
import { useLearning } from "../components/learning";
import { Icon } from "../components/icon";
import { PendingNavigation } from "../components/pending-navigation";
import { usePendingOwnedWrites } from "../components/pending-owned-writes";
import type { Route } from "./+types/reviews";
export async function loader({ request }: Route.LoaderArgs) {
  try {
    return await getPrivate<ReviewQueue>(request, "/api/v1/me/reviews");
  } catch (error) {
    if (error instanceof Response && error.status === 401)
      throw redirect("/login?next=/reviews");
    throw error;
  }
}
const ratings: { value: ReviewRating; label: string }[] = [
  { value: "again", label: "还不熟" },
  { value: "remembered", label: "有印象" },
  { value: "familiar", label: "记住了" },
];
export default function Reviews({ loaderData }: Route.ComponentProps) {
  const [queue, setQueue] = useState(loaderData),
    [index, setIndex] = useState(0),
    [revealed, setRevealed] = useState(false),
    [saving, setSaving] = useState(false),
    [ready, setReady] = useState(false),
    [uncertain, setUncertain] = useState(false),
    [queueStale, setQueueStale] = useState(false),
    [error, setError] = useState(""),
    [results, setResults] = useState<ReviewAttemptResult[]>([]);
  const pending = useRef<{
      cardId: string;
      body: ReviewAttemptRequest;
      restored?: boolean;
    } | null>(null),
    busy = useRef(false),
    generation = useRef(0),
    alive = useRef(true),
    cardButton = useRef<HTMLButtonElement>(null),
    heading = useRef<HTMLHeadingElement>(null),
    uncertainHeading = useRef<HTMLHeadingElement>(null),
    staleHeading = useRef<HTMLHeadingElement>(null),
    saveFailure = useRef<HTMLParagraphElement>(null),
    focusFailure = useRef(false),
    unavailable = useRef(new Set<string>()),
    animation = useRef<Animation | null>(null);
  const audio = useLearning(),
    term = queue.items[index];
  const storageKey = audio.profile
    ? draftScope(audio.profile.id, "reviews", 1) + ":pending"
    : "";
  const hasPendingWrite = !!pending.current;
  const ownedPending = usePendingOwnedWrites(audio.profile?.id);
  useEffect(() => {
    generation.current++;
    alive.current = true;
    pending.current = null;
    unavailable.current.clear();
    busy.current = false;
    focusFailure.current = false;
    setUncertain(false);
    setQueueStale(false);
    setSaving(false);
    const stored = storageKey ? readDraft(storageKey) : null;
    if (stored && typeof stored === "object") {
      const path = (stored as { path?: unknown }).path;
      const match =
        typeof path === "string"
          ? /^\/api\/v1\/me\/reviews\/([A-Za-z0-9_-]{1,100})\/attempts$/.exec(
              path,
            )
          : null;
      if (
        match &&
        validOwnedPending(stored, { kind: "rating", cardId: match[1] })
      ) {
        pending.current = {
          cardId: match[1],
          body: stored.body as unknown as ReviewAttemptRequest,
          restored: true,
        };
        setUncertain(true);
        setError("上次复习保存尚未确认，请重试原提交。");
      } else saveDraft(storageKey, null);
    }
    setReady(!!storageKey);
    return () => {
      alive.current = false;
      animation.current?.cancel();
    };
  }, [storageKey]);
  useLayoutEffect(() => {
    if (uncertain) uncertainHeading.current?.focus();
    else if (queueStale) staleHeading.current?.focus();
  }, [uncertain, queueStale]);
  useLayoutEffect(() => {
    if (!saving && error && focusFailure.current) {
      focusFailure.current = false;
      saveFailure.current?.focus();
    }
  }, [saving, error]);
  function acceptQueue(fresh: ReviewQueue) {
    if (fresh.items.some((card) => unavailable.current.has(card.id))) {
      setQueueStale(true);
      setError("队列仍包含不可用的表达，请重新读取后继续。");
      return false;
    }
    setQueue(fresh);
    setIndex(0);
    setRevealed(false);
    setQueueStale(false);
    requestAnimationFrame(() => heading.current?.focus());
    return true;
  }
  useEffect(() => {
    if (!hasPendingWrite) return;
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [hasPendingWrite]);
  async function submit(job: NonNullable<typeof pending.current>) {
    if (busy.current) return;
    const gen = generation.current;
    const stored = {
      path: "/api/v1/me/reviews/" + job.cardId + "/attempts",
      method: "POST",
      body: job.body,
    };
    if (
      !storageKey ||
      !validOwnedPending(stored, { kind: "rating", cardId: job.cardId }) ||
      !saveDraft(storageKey, stored)
    ) {
      setError("浏览器无法保留这次提交，请允许本地存储后重试。");
      return;
    }
    busy.current = true;
    pending.current = job;
    setSaving(true);
    setError("");
    try {
      const saved = await privateRequest<ReviewAttemptResult>(
        stored.path,
        "POST",
        job.body,
      );
      clearPending(storageKey, job.body.idempotencyKey);
      if (!alive.current || gen !== generation.current) return;
      pending.current = null;
      setUncertain(false);
      setResults((old) => [...old, saved]);
      if (job.restored) {
        try {
          const fresh = await privateRequest<ReviewQueue>(
            "/api/v1/me/reviews",
            "GET",
          );
          if (!alive.current || gen !== generation.current) return;
          acceptQueue(fresh);
        } catch {
          if (!alive.current || gen !== generation.current) return;
          setQueue((old) => ({
            ...old,
            items: old.items.filter((card) => card.id !== job.cardId),
          }));
          setQueueStale(true);
          setError("复习已保存，最新队列暂时无法读取，请重新读取后继续。");
        }
        setIndex(0);
      } else setIndex((old) => old + 1);
      setRevealed(false);
      audio.stop();
      requestAnimationFrame(() => {
        heading.current?.focus();
        heading.current?.scrollIntoView({
          block: "start",
          behavior: "instant",
        });
      });
    } catch (failure) {
      if (!alive.current || gen !== generation.current) return;
      if (
        failure instanceof ApiRequestError &&
        definitiveWriteFailure(failure)
      ) {
        clearPending(storageKey, job.body.idempotencyKey);
        pending.current = null;
        setUncertain(false);
        if (
          failure.status === 409 ||
          failure.status === 410 ||
          failure.status === 404
        ) {
          if (failure.status === 410 || failure.status === 404)
            unavailable.current.add(job.cardId);
          audio.stop();
          setRevealed(false);
          setQueueStale(true);
          let accepted = false;
          try {
            const fresh = await privateRequest<ReviewQueue>(
              "/api/v1/me/reviews",
              "GET",
            );
            if (alive.current && gen === generation.current) {
              accepted = acceptQueue(fresh);
            }
          } catch {
            if (!alive.current || gen !== generation.current) return;
            setQueueStale(true);
          }
          if (!alive.current || gen !== generation.current) return;
          if (accepted) setError("复习记录已变化，请确认最新队列后继续。");
          else setError("复习记录已变化，请重新读取有效队列后继续。");
        } else {
          focusFailure.current = true;
          setError(failure.message);
        }
      } else {
        audio.stop();
        setUncertain(true);
        setError("这次保存尚未确认，请重试确认原提交。");
      }
    } finally {
      if (gen === generation.current) {
        busy.current = false;
        if (alive.current) setSaving(false);
      }
    }
  }
  async function nextBatch() {
    if (busy.current || uncertain || !ready) return;
    busy.current = true;
    const gen = generation.current;
    setSaving(true);
    setError("");
    try {
      const fresh = await privateRequest<ReviewQueue>(
        "/api/v1/me/reviews",
        "GET",
      );
      if (alive.current && gen === generation.current) {
        acceptQueue(fresh);
      }
    } catch (failure) {
      if (alive.current && gen === generation.current)
        setError(
          failure instanceof Error ? failure.message : "暂时无法读取复习队列。",
        );
    } finally {
      if (gen === generation.current) {
        busy.current = false;
        if (alive.current) setSaving(false);
      }
    }
  }
  function reveal() {
    if (busy.current || uncertain || queueStale || !ready) return;
    const height = cardButton.current?.getBoundingClientRect().height;
    animation.current?.cancel();
    setRevealed(!revealed);
    if (!revealed && term)
      audio.play([
        knowledgeUnit(
          "review-" + term.id,
          term.vocabulary.lemma,
          term.vocabulary.recording,
        ),
      ]);
    else audio.stop();
    requestAnimationFrame(() => {
      if (
        cardButton.current &&
        height &&
        !matchMedia("(prefers-reduced-motion:reduce)").matches
      )
        animation.current = cardButton.current.animate(
          [
            { height: height + "px" },
            {
              height: cardButton.current.getBoundingClientRect().height + "px",
            },
          ],
          { duration: 420, easing: "cubic-bezier(.22,.8,.25,1)" },
        );
    });
  }
  return (
    <section className="review-page page-arrive">
      <PendingNavigation
        active={hasPendingWrite || ownedPending}
        onStay={() => {
          const target = queueStale
            ? staleHeading.current
            : uncertain
              ? uncertainHeading.current
              : error
                ? saveFailure.current
                : heading.current;
          target?.focus({ preventScroll: true });
        }}
      />
      <div className="review-session-header">
        <div>
          <h1 ref={heading} tabIndex={-1}>
            复习
          </h1>
          <p>今天的表达</p>
        </div>
        {queue.items.length > 0 && (
          <span className="small">
            {Math.min(index + 1, queue.items.length)} / {queue.items.length}
          </span>
        )}
      </div>
      <div
        className="review-progress"
        role="progressbar"
        aria-label="本轮复习"
        aria-valuemin={0}
        aria-valuemax={queue.items.length || 1}
        aria-valuenow={index}
      >
        <span
          style={{
            width: queue.items.length
              ? (index / queue.items.length) * 100 + "%"
              : "100%",
          }}
        />
      </div>
      {uncertain ? (
        <div className="empty-state">
          <h2 ref={uncertainHeading} tabIndex={-1}>
            确认上次复习
          </h2>
          <p>确认原提交后，再继续今天的表达。</p>
        </div>
      ) : queueStale ? (
        <div className="empty-state">
          <h2 ref={staleHeading} tabIndex={-1}>
            需要确认复习队列
          </h2>
          <p>重新读取后，再继续今天的表达。</p>
        </div>
      ) : term ? (
        <>
          <div className="review-context">
            <Link to={"/lessons/" + term.sourceLessonId}>
              <Icon name="book" />
              回看来源课程
            </Link>
          </div>
          <button
            ref={cardButton}
            className="review-flashcard"
            aria-expanded={revealed}
            aria-disabled={saving || uncertain || queueStale || !ready}
            aria-busy={saving}
            onClick={reveal}
          >
            <span className="review-kind">
              {term.vocabulary.partOfSpeech === "phrase"
                ? "常用表达"
                : "日常词汇"}
            </span>
            <span className="review-expression" lang="fr">
              {term.vocabulary.gender === "feminine"
                ? "une "
                : term.vocabulary.gender === "masculine"
                  ? "un "
                  : ""}
              {term.vocabulary.lemma}
            </span>
            {revealed && (
              <span className="review-solution">
                <span className="review-meaning">
                  {term.vocabulary.meaningZh}
                </span>
                <span className="review-explanation">
                  {term.vocabulary.noteZh}
                </span>
              </span>
            )}
          </button>
          {revealed && !uncertain && (
            <div
              className="review-ratings review-choices-enter"
              role="group"
              aria-label="这次回想的感觉"
            >
              {ratings.map((rating, grade) => (
                <button
                  key={rating.value}
                  data-grade={grade}
                  aria-disabled={saving || queueStale || !ready}
                  aria-busy={saving}
                  onClick={() => {
                    if (busy.current || uncertain || queueStale || !ready)
                      return;
                    void submit({
                      cardId: term.id,
                      body: {
                        cardVersion: term.version,
                        idempotencyKey: operationKey(),
                        rating: rating.value,
                      },
                    });
                  }}
                >
                  <span className="rating-dot" aria-hidden="true" />
                  <span>{rating.label}</span>
                  <Icon name="chevron" />
                </button>
              ))}
            </div>
          )}
        </>
      ) : (
        <div className="review-summary">
          <h2>{results.length ? "本轮回顾" : "暂时没有到期表达"}</h2>
          <p>
            {results.length
              ? `已保存 ${results.length} 个表达的复习记录。`
              : queue.nextDueAt
                ? "下一次复习：" +
                  new Date(queue.nextDueAt).toLocaleDateString("zh-CN", {
                    timeZone: queue.timeZone,
                    month: "numeric",
                    day: "numeric",
                  })
                : "学完课程后，表达会加入这里。"}
          </p>
          {results.length > 0 && (
            <ul className="review-result-list">
              {results.map((result) => (
                <li key={result.card.id}>
                  <div>
                    <span className="result-expression" lang="fr">
                      {result.card.vocabulary.lemma}
                    </span>
                    <small>{result.card.vocabulary.meaningZh}</small>
                  </div>
                  <span className="review-result-grade">
                    下次{" "}
                    {new Date(result.card.dueAt).toLocaleDateString("zh-CN", {
                      timeZone: result.timeZone,
                      month: "numeric",
                      day: "numeric",
                    })}
                  </span>
                </li>
              ))}
            </ul>
          )}
          <div className="review-summary-actions">
            {queue.dueCount > queue.items.length && (
              <button
                className="primary summary-main"
                aria-disabled={saving}
                aria-busy={saving}
                onClick={() => void nextBatch()}
              >
                查看下一组
              </button>
            )}
            <Link className="text-button" to="/">
              回到今天
            </Link>
          </div>
        </div>
      )}
      {error && (
        <p
          ref={saveFailure}
          className="error-message"
          role="alert"
          tabIndex={-1}
        >
          {error}
        </p>
      )}
      {queueStale && (
        <button
          className="text-button"
          aria-disabled={saving}
          aria-busy={saving}
          onClick={() => void nextBatch()}
        >
          重新读取复习队列
        </button>
      )}
      {uncertain && (
        <button
          className="primary"
          aria-disabled={saving}
          aria-busy={saving}
          onClick={() => {
            if (pending.current) void submit(pending.current);
          }}
        >
          重试保存
          <Icon name="check" />
        </button>
      )}
      {saving && (
        <p className="profile-note" role="status">
          正在保存
        </p>
      )}
    </section>
  );
}
