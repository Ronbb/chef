import { useLayoutEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router";
import type { LearningSession } from "@brioche/contracts/LearningSession";
import { ApiRequestError, privateRequest } from "../lib/api.client";
import { operationKey } from "../lib/operation-key";
type StartLearningProps = {
  lessonId: string;
  children?: React.ReactNode;
};
export function StartLearning(props: StartLearningProps) {
  return <LearningEntry key={props.lessonId} {...props} />;
}
function LearningEntry({
  lessonId,
  children = "开始学习",
}: StartLearningProps) {
  const navigate = useNavigate(),
    busy = useRef(false),
    alive = useRef(true),
    key = useRef<string | null>(null);
  const [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    [expired, setExpired] = useState(false);
  useLayoutEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  async function start() {
    if (busy.current) return;
    busy.current = true;
    setPending(true);
    setError("");
    setExpired(false);
    key.current ??= operationKey();
    try {
      const session = await privateRequest<LearningSession>(
        "/api/v1/learning-sessions",
        "POST",
        { lessonId, schemaVersion: "1.0", idempotencyKey: key.current },
      );
      if (alive.current) void navigate("/learning/" + session.progress.id);
    } catch (failure) {
      if (alive.current) {
        const status =
          failure instanceof ApiRequestError && failure.phase === "request"
            ? failure.status
            : null;
        setExpired(status === 401);
        setError(
          status === 409
            ? "课程暂时无法开始，请重新打开课程。"
            : status === 410
              ? "课程已撤回，暂时无法开始学习。"
              : status === 404
                ? "没有找到这堂课程，请重新选择。"
                : failure instanceof ApiRequestError
                  ? failure.message
                  : "学习尚未打开，请重试。",
        );
      }
    } finally {
      busy.current = false;
      if (alive.current) setPending(false);
    }
  }
  return (
    <div className="start-learning">
      <button
        className="primary"
        type="button"
        aria-disabled={pending}
        aria-busy={pending}
        onClick={() => void start()}
      >
        {pending ? "正在打开" : children}
      </button>
      {error && (
        <p className="error-message" role="alert">
          {error}
        </p>
      )}
      {expired && (
        <Link
          className="text-button"
          to={"/login?next=" + encodeURIComponent("/lessons/" + lessonId)}
        >
          重新登录
        </Link>
      )}
    </div>
  );
}
