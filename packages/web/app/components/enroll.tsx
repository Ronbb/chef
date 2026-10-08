import { useState } from "react";
import { Link } from "react-router";
import type { ReadingReviewCard as ReviewCard } from "../lib/reading-model";
import { useLearning } from "./learning";
import { useOwnedWrite } from "./owned-write";
export function Enroll({
  knowledgeId,
  lessonId,
  revision,
  onWithdrawn,
}: {
  knowledgeId: string;
  lessonId: string;
  revision: number;
  onWithdrawn?: () => void;
}) {
  const learning = useLearning(),
    [card, setCard] = useState<ReviewCard | null>(null),
    write = useOwnedWrite<ReviewCard>(undefined, {
      userId: learning.profile?.id,
      target: { kind: "enroll", knowledgeId, lessonId, revision },
      accept: setCard,
      onUnavailable: (status) => {
        if (status === 410) onWithdrawn?.();
      },
    });
  if (!learning.profile) return null;
  return (
    <div className="knowledge-actions">
      {card ? (
        <Link className="text-button" to="/library?view=reviews">
          {card.suspended ? "已加入复习 · 已暂停" : "已加入复习"}
        </Link>
      ) : (
        <button
          className="text-button"
          disabled={write.blocked}
          onClick={() =>
            write.write(
              "/api/v2/me/review-enrollments",
              {
                knowledgeId,
                sourceLessonId: lessonId,
                sourceRevision: revision,
              },
              setCard,
              "POST",
            )
          }
        >
          {write.saving ? "正在保存" : "加入复习"}
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
