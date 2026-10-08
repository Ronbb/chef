import { useEffect, useRef, useState } from "react";
import { Link } from "react-router";
import type { ReadingSavedItem as SavedItem } from "../lib/reading-model";
import { useLearning } from "./learning";
import { useOwnedWrite } from "./owned-write";
import { ApiRequestError, privateRequest } from "../lib/api.client";
import { Icon } from "./icon";
export function Bookmark({
  knowledgeId,
  lessonId,
  revision,
  initial,
  onChange,
  onRefresh,
  onWithdrawn,
}: {
  knowledgeId: string;
  lessonId: string;
  revision: number;
  initial?: SavedItem;
  onChange?: (item: SavedItem) => void;
  onRefresh?: (item: SavedItem) => void;
  onWithdrawn?: () => void;
}) {
  const [item, setItem] = useState<SavedItem | null>(initial ?? null),
    [ready, setReady] = useState(!!initial),
    [readError, setReadError] = useState("");
  const mounted = useRef(true),
    audio = useLearning(),
    path = "/api/v2/me/saved-items/" + encodeURIComponent(knowledgeId);
  async function refresh() {
    try {
      const result = await privateRequest<SavedItem>(path, "GET");
      if (mounted.current) {
        setItem(result);
        setReady(true);
        setReadError("");
        onRefresh?.(result);
      }
    } catch (error) {
      if (!mounted.current) return;
      if (error instanceof ApiRequestError && error.status === 404) {
        setItem(null);
        setReady(true);
        setReadError("");
      } else {
        setReadError("暂时无法读取收藏状态。");
        setReady(false);
      }
    }
  }
  const write = useOwnedWrite<SavedItem>(refresh, {
    userId: audio.profile?.id,
    target: { kind: "bookmark", knowledgeId, lessonId, revision },
    accept: (saved) => {
      setItem(saved);
      onChange?.(saved);
    },
    onUnavailable: (status) => {
      if (status === 410) onWithdrawn?.();
    },
  });
  useEffect(() => {
    mounted.current = true;
    if (audio.profile && !initial) void refresh();
    return () => {
      mounted.current = false;
    };
  }, [audio.profile?.id]);
  if (!audio.profile)
    return (
      <Link
        className="text-button"
        to={"/login?next=" + encodeURIComponent("/lessons/" + lessonId)}
      >
        登录后收藏
      </Link>
    );
  return (
    <div className="knowledge-actions">
      {readError ? (
        <>
          <p role="alert">{readError}</p>
          <button className="text-button" onClick={() => void refresh()}>
            重新读取
          </button>
        </>
      ) : (
        <button
          className="text-button bookmark-action"
          aria-pressed={!!item?.saved}
          disabled={!ready || write.blocked}
          onClick={() =>
            write.write(
              path,
              {
                sourceLessonId: lessonId,
                sourceRevision: revision,
                saved: !item?.saved,
                version: item?.version ?? 0,
              },
              (saved) => {
                setItem(saved);
                onChange?.(saved);
              },
            )
          }
        >
          <Icon name="book" />
          {write.saving ? "正在保存" : item?.saved ? "已收藏" : "收藏表达"}
        </button>
      )}
      {write.error && (
        <p className="error-message" role="alert">
          {write.error}
        </p>
      )}
      {write.uncertain && (
        <button
          className="text-button"
          disabled={write.saving}
          onClick={write.retry}
        >
          重试保存
        </button>
      )}
    </div>
  );
}
