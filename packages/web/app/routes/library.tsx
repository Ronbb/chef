import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { Link, redirect } from "react-router";
import type { SavedItem } from "@brioche/contracts/SavedItem";
import type { SavedPage } from "@brioche/contracts/SavedPage";
import type { ReviewCard } from "@brioche/contracts/ReviewCard";
import type { ReviewCardsPage } from "@brioche/contracts/ReviewCardsPage";
import { getPrivate } from "../lib/api.server";
import { ApiRequestError, privateRequest } from "../lib/api.client";
import { useOwnedWrite } from "../components/owned-write";
import { useLearning } from "../components/learning";
import { knowledgeUnit } from "../lib/recording-playback";
import { Bookmark } from "../components/bookmark";
import { Enroll } from "../components/enroll";
import { Icon } from "../components/icon";
import { usePageCursorFocus } from "../components/page-cursor-focus";
import { PendingNavigation } from "../components/pending-navigation";
import { usePendingOwnedWrites } from "../components/pending-owned-writes";
import type { Route } from "./+types/library";
export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url),
    view = url.searchParams.get("view") === "reviews" ? "reviews" : "saved",
    cursor = url.searchParams.get("cursor"),
    query = cursor ? "?cursor=" + encodeURIComponent(cursor) : "";
  try {
    return view === "saved"
      ? {
          view: "saved" as const,
          page: await getPrivate<SavedPage>(
            request,
            "/api/v1/me/saved-items" + query,
          ),
          cursor,
        }
      : {
          view: "reviews" as const,
          page: await getPrivate<ReviewCardsPage>(
            request,
            "/api/v1/me/review-cards" + query,
          ),
          cursor,
        };
  } catch (error) {
    if (error instanceof Response && error.status === 401)
      throw redirect("/login?next=/library");
    throw error;
  }
}
export default function Library({ loaderData }: Route.ComponentProps) {
  const heading = usePageCursorFocus(loaderData.cursor);
  const pending = usePendingOwnedWrites(useLearning().profile?.id);
  return (
    <section className="settings-page page-arrive">
      <PendingNavigation
        active={pending}
        onStay={() => heading.current?.focus({ preventScroll: true })}
      />
      <div className="settings-title-row">
        <h1 ref={heading} tabIndex={-1}>
          我的表达
        </h1>
        <Link className="text-button" to="/profile">
          我的
        </Link>
      </div>
      <nav className="reader-mode" aria-label="表达分类">
        <Link
          className={loaderData.view === "saved" ? "active" : ""}
          aria-current={loaderData.view === "saved" ? "page" : undefined}
          to="/library"
        >
          收藏
        </Link>
        <Link
          className={loaderData.view === "reviews" ? "active" : ""}
          aria-current={loaderData.view === "reviews" ? "page" : undefined}
          to="/library?view=reviews"
        >
          复习
        </Link>
      </nav>
      {loaderData.view === "saved" ? (
        <SavedList
          key={"saved" + loaderData.cursor}
          page={loaderData.page}
          continuation={loaderData.cursor !== null}
        />
      ) : (
        <Cards
          key={"reviews" + loaderData.cursor}
          page={loaderData.page}
          continuation={loaderData.cursor !== null}
        />
      )}
      <div className="review-summary-actions">
        <Link className="text-button" to="/review-history">
          复习记录
          <Icon name="chevron" />
        </Link>
        <Link className="text-button" to="/reviews">
          开始复习
        </Link>
      </div>
    </section>
  );
}
function SavedList({
  page,
  continuation,
}: {
  page: SavedPage;
  continuation: boolean;
}) {
  const [items, setItems] = useState(page.items);
  const headings = useRef(new Map<string, HTMLButtonElement>()),
    pendingFocus = useRef<string | null | undefined>(undefined),
    empty = useRef<HTMLParagraphElement>(null);
  useLayoutEffect(() => {
    if (pendingFocus.current === undefined) return;
    const next = pendingFocus.current;
    pendingFocus.current = undefined;
    const target =
      (next && headings.current.get(next)) ||
      headings.current.values().next().value ||
      empty.current;
    target?.focus();
  }, [items]);
  function remove(id: string) {
    const index = items.findIndex((item) => item.id === id);
    pendingFocus.current = items[index + 1]?.id ?? items[index - 1]?.id ?? null;
    setItems((old) => old.filter((row) => row.id !== id));
  }
  return (
    <>
      {!items.length && (
        <>
          <p ref={empty} tabIndex={-1} role="status" className="profile-note">
            {continuation || page.nextCursor
              ? "这一页没有收藏了。"
              : "阅读时收藏的表达会放在这里。"}
          </p>
          {continuation && (
            <Link className="text-button" to="/library">
              返回收藏列表
            </Link>
          )}
        </>
      )}
      <div className="library-list">
        {items.map((item) => (
          <SavedRow
            key={item.id}
            item={item}
            remove={() => remove(item.id)}
            headingRef={(element) => {
              if (element) headings.current.set(item.id, element);
              else headings.current.delete(item.id);
            }}
          />
        ))}
      </div>
      {page.nextCursor && (
        <Link
          className="text-button"
          to={"/library?cursor=" + encodeURIComponent(page.nextCursor)}
        >
          下一页
          <Icon name="chevron" />
        </Link>
      )}
    </>
  );
}
function SavedRow({
  item,
  remove,
  headingRef,
}: {
  item: SavedItem;
  remove: () => void;
  headingRef: (element: HTMLButtonElement | null) => void;
}) {
  const [open, setOpen] = useState(false),
    [current, setCurrent] = useState(item),
    audio = useLearning(),
    panelId = useId();
  const heading = useRef<HTMLButtonElement>(null),
    wasWithdrawn = useRef(item.withdrawn);
  useLayoutEffect(() => {
    if (current.withdrawn && !wasWithdrawn.current) heading.current?.focus();
    wasWithdrawn.current = current.withdrawn;
  }, [current.withdrawn]);
  function withdraw() {
    audio.stop();
    setCurrent((old) => ({ ...old, withdrawn: true, vocabulary: null }));
  }
  function accept(saved: SavedItem) {
    if (saved.withdrawn) audio.stop();
    // A hard withdrawal is irreversible for this fixed source revision.
    setCurrent((old) =>
      old.withdrawn ? { ...saved, withdrawn: true, vocabulary: null } : saved,
    );
    if (!saved.saved) remove();
  }
  return (
    <article className="library-entry">
      <button
        ref={(element) => {
          heading.current = element;
          headingRef(element);
        }}
        className="library-entry-heading"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => {
          setOpen(!open);
          if (!open && current.vocabulary)
            audio.play([
              knowledgeUnit(
                "saved-" + current.id,
                current.vocabulary.lemma,
                current.vocabulary.recording,
              ),
            ]);
        }}
      >
        <span>
          <strong lang={current.withdrawn ? "zh-CN" : "fr"}>
            {current.vocabulary?.lemma ?? "来源内容已撤回"}
          </strong>
          <small>{current.vocabulary?.meaningZh}</small>
        </span>
        <Icon name="chevron" />
      </button>
      {open && (
        <div id={panelId} className="library-entry-body">
          <p>{current.vocabulary?.noteZh}</p>
          {!current.withdrawn && (
            <Link
              className="text-button"
              to={"/lessons/" + item.sourceLessonId}
            >
              回看来源课程
            </Link>
          )}
          <Bookmark
            initial={current}
            knowledgeId={item.knowledgeId}
            lessonId={item.sourceLessonId}
            revision={item.sourceRevision}
            onChange={accept}
            onWithdrawn={withdraw}
            onRefresh={(saved) => {
              accept(saved);
              if (!saved.saved) {
                audio.toast("这条表达已在其他设备取消收藏。");
              }
            }}
          />
          {!current.withdrawn && (
            <Enroll
              knowledgeId={item.knowledgeId}
              lessonId={item.sourceLessonId}
              revision={item.sourceRevision}
              onWithdrawn={withdraw}
            />
          )}
        </div>
      )}
    </article>
  );
}
function Cards({
  page,
  continuation,
}: {
  page: ReviewCardsPage;
  continuation: boolean;
}) {
  return (
    <>
      {!page.items.length && (
        <>
          <p className="profile-note">
            {continuation
              ? "这一页暂时没有复习表达。"
              : "学完课程或加入表达后，可以在这里管理复习。"}
          </p>
          {continuation && (
            <Link className="text-button" to="/library?view=reviews">
              返回复习列表
            </Link>
          )}
        </>
      )}
      <div className="library-list">
        {page.items.map((card) => (
          <ManagedCard key={card.id} initial={card} />
        ))}
      </div>
      {page.nextCursor && (
        <Link
          className="text-button"
          to={
            "/library?view=reviews&cursor=" +
            encodeURIComponent(page.nextCursor)
          }
        >
          下一页
          <Icon name="chevron" />
        </Link>
      )}
    </>
  );
}
function ManagedCard({ initial }: { initial: ReviewCard }) {
  const [card, setCard] = useState(initial),
    [open, setOpen] = useState(false),
    [readFailed, setReadFailed] = useState(false),
    [unavailable, setUnavailable] = useState<404 | 410 | null>(null),
    [refreshing, setRefreshing] = useState(false),
    audio = useLearning(),
    panelId = useId();
  const mounted = useRef(true),
    unavailableHeading = useRef<HTMLHeadingElement>(null),
    heading = useRef<HTMLButtonElement>(null),
    preference = useRef<HTMLButtonElement>(null),
    writeError = useRef<HTMLParagraphElement>(null),
    reading = useRef(false),
    focusRead = useRef(false),
    focusRetry = useRef(false);
  useLayoutEffect(() => {
    if (unavailable) unavailableHeading.current?.focus();
  }, [unavailable]);
  function removeUnavailable(status: 404 | 410) {
    if (!mounted.current) return;
    audio.stop();
    setUnavailable(status);
    setReadFailed(false);
  }
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  async function refresh() {
    if (reading.current || !mounted.current) return;
    reading.current = true;
    setRefreshing(true);
    try {
      const fresh = await privateRequest<ReviewCard>(
        "/api/v1/me/reviews/" + initial.id,
        "GET",
      );
      if (mounted.current) {
        setCard(fresh);
        setReadFailed(false);
      }
    } catch (error) {
      if (
        error instanceof ApiRequestError &&
        error.phase === "request" &&
        (error.status === 404 || error.status === 410)
      )
        removeUnavailable(error.status);
      else if (mounted.current) setReadFailed(true);
      throw error;
    } finally {
      reading.current = false;
      if (mounted.current) setRefreshing(false);
    }
  }
  const write = useOwnedWrite<ReviewCard>(refresh, {
    userId: audio.profile?.id,
    target: { kind: "preference", cardId: initial.id },
    accept: setCard,
    onUnavailable: removeUnavailable,
  });
  useLayoutEffect(() => {
    if (unavailable) {
      focusRead.current = false;
      focusRetry.current = false;
      return;
    }
    if (focusRead.current && !refreshing && !readFailed) {
      focusRead.current = false;
      heading.current?.focus({ preventScroll: true });
    }
    if (focusRetry.current && !write.saving && !write.uncertain) {
      focusRetry.current = false;
      (write.error ? writeError.current : preference.current)?.focus({
        preventScroll: true,
      });
    }
  }, [
    unavailable,
    refreshing,
    readFailed,
    write.saving,
    write.uncertain,
    write.error,
  ]);
  if (unavailable)
    return (
      <article className="library-entry">
        <h2 ref={unavailableHeading} tabIndex={-1}>
          {unavailable === 410 ? "来源内容已撤回" : "复习记录已不可用"}
        </h2>
        <p className="profile-note">这条表达暂时无法继续复习。</p>
      </article>
    );
  return (
    <article className="library-entry">
      <button
        ref={heading}
        className="library-entry-heading"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => {
          setOpen(!open);
          if (!open)
            audio.play([
              knowledgeUnit(
                "managed-" + card.id,
                card.vocabulary.lemma,
                card.vocabulary.recording,
              ),
            ]);
        }}
      >
        <span>
          <strong lang="fr">{card.vocabulary.lemma}</strong>
          <small>{card.suspended ? "已暂停" : card.vocabulary.meaningZh}</small>
        </span>
        <Icon name="chevron" />
      </button>
      {open && (
        <div id={panelId} className="library-entry-body">
          <p>{card.vocabulary.noteZh}</p>
          <button
            ref={preference}
            className="text-button"
            aria-disabled={write.blocked || readFailed || refreshing}
            aria-busy={write.saving}
            onClick={() => {
              if (write.blocked || readFailed || refreshing) return;
              write.write(
                "/api/v1/me/reviews/" + card.id + "/preferences",
                { cardVersion: card.version, suspended: !card.suspended },
                setCard,
              );
            }}
          >
            {write.saving
              ? "正在保存"
              : card.suspended
                ? "恢复复习"
                : "暂停复习"}
          </button>
          {write.error && (
            <p
              ref={writeError}
              tabIndex={-1}
              className="error-message"
              role="alert"
            >
              {write.error}
            </p>
          )}
          {readFailed && (
            <>
              <p className="error-message" role="alert">
                最新记录暂时无法读取，读取成功后再继续操作。
              </p>
              <button
                className="text-button"
                aria-disabled={refreshing}
                aria-busy={refreshing}
                onBlur={() => {
                  focusRead.current = false;
                }}
                onClick={() => {
                  if (reading.current) return;
                  focusRead.current = true;
                  void refresh().catch(() => {});
                }}
              >
                {refreshing ? "正在读取" : "重新读取记录"}
              </button>
            </>
          )}
          {write.uncertain && (
            <button
              className="text-button"
              aria-disabled={write.saving}
              aria-busy={write.saving}
              onBlur={() => {
                focusRetry.current = false;
              }}
              onClick={() => {
                if (write.saving) return;
                focusRetry.current = true;
                write.retry();
              }}
            >
              重试保存
            </button>
          )}
          <Link className="text-button" to={"/lessons/" + card.sourceLessonId}>
            回看来源课程
          </Link>
        </div>
      )}
    </article>
  );
}
