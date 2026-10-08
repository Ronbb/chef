import { productNamespace } from "../lib/product-runtime";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, redirect } from "react-router";
import type { NeutralLearningSession } from "@brioche/contracts/NeutralLearningSession";
import { lessonLanguage, type ReadingSession } from "../lib/reading-model";
import type { LearningState } from "@brioche/contracts/LearningState";
import type { AttemptResult } from "@brioche/contracts/AttemptResult";
import type { HintResult } from "@brioche/contracts/HintResult";
import { getIdentity, getPrivate } from "../lib/api.server";
import { draftScope, readDraft, saveDraft } from "../lib/learning-draft";
import { useLearningSession } from "../components/learning-session";
import { ReadingBlock } from "../components/reading-block";
import { TeachingBlock } from "../components/teaching-block";
import { ExerciseEditor } from "../components/exercise-editor";
import { useLearning } from "../components/learning";
import { Icon } from "../components/icon";
import { PendingNavigation } from "../components/pending-navigation";
import { usePendingOwnedWrites } from "../components/pending-owned-writes";
import type { Route } from "./+types/learning";
export async function loader({ request, params }: Route.LoaderArgs) {
  try {
    const session = await getPrivate<NeutralLearningSession>(
      request,
      "/api/v2/learning-sessions/" + encodeURIComponent(params.sessionId),
    );
    const identity = await getIdentity(request);
    if (!identity.user)
      throw new Response("请登录后继续学习。", { status: 401 });
    return { session, ownerId: identity.user.id };
  } catch (error) {
    if (error instanceof Response && error.status === 401)
      throw redirect(
        "/login?next=" + encodeURIComponent(new URL(request.url).pathname),
      );
    throw error;
  }
}
export default function Learning({ loaderData }: Route.ComponentProps) {
  return (
    <LearningSessionContent
      key={loaderData.ownerId + loaderData.session.progress.id}
      initial={loaderData.session}
      ownerId={loaderData.ownerId}
    />
  );
}
export function LearningSessionContent({
  initial,
  ownerId,
}: {
  initial: ReadingSession;
  ownerId: string;
}) {
  const scope = draftScope(
    ownerId,
    initial.progress.id,
    initial.lesson.revision,
    productNamespace,
  );
  const session = useLearningSession(initial, scope),
    audio = useLearning(),
    lesson = initial.lesson;
  const ownedPending = usePendingOwnedWrites(ownerId);
  const [index, setIndex] = useState(
    Math.max(
      0,
      lesson.steps.findIndex((step) => step.id === initial.progress.lastStepId),
    ),
  );
  const heading = useRef<HTMLHeadingElement>(null),
    step = lesson.steps[index];
  const completionConfirmed =
      !!session.progress.completedAt && !session.uncertain,
    previouslyCompleted = useRef(!!initial.progress.completedAt);
  useEffect(() => {
    if (previouslyCompleted.current === completionConfirmed) return;
    previouslyCompleted.current = completionConfirmed;
    if (!completionConfirmed) return;
    const frame = requestAnimationFrame(() => {
      heading.current?.focus();
      heading.current?.scrollIntoView({ block: "start", behavior: "instant" });
    });
    return () => cancelAnimationFrame(frame);
  }, [completionConfirmed]);
  useEffect(() => {
    if (!session.unavailable) return;
    audio.stop();
    const frame = requestAnimationFrame(() => {
      heading.current?.focus();
      heading.current?.scrollIntoView({ block: "start", behavior: "instant" });
    });
    return () => cancelAnimationFrame(frame);
  }, [session.unavailable]);
  useEffect(() => {
    const stored = readDraft(scope + ":step");
    const restored = lesson.steps.findIndex((step) => step.id === stored);
    if (restored >= 0) setIndex(restored);
  }, [scope]);
  function move(next: number) {
    audio.stop();
    setIndex(next);
    saveDraft(scope + ":step", lesson.steps[next].id);
    requestAnimationFrame(() => {
      heading.current?.focus();
      heading.current?.scrollIntoView({ block: "start", behavior: "instant" });
    });
  }
  const consumedStepConfirmation = useRef<string | null>(null);
  useLayoutEffect(() => {
    const confirmation = session.stepConfirmation;
    if (!confirmation || consumedStepConfirmation.current === confirmation.key)
      return;
    consumedStepConfirmation.current = confirmation.key;
    if (session.progress.completedAt) return;
    const confirmedIndex = lesson.steps.findIndex(
      (step) => step.id === confirmation.id,
    );
    if (confirmedIndex >= 0 && confirmedIndex + 1 < lesson.steps.length)
      move(confirmedIndex + 1);
  }, [session.stepConfirmation]);
  const requiredInStep = lesson.completion.requiredExerciseIds.filter((id) =>
    step.blockIds.includes(id),
  );
  const canContinue = requiredInStep.every((id) =>
    session.progress.attempts.some((attempt) => attempt.exerciseId === id),
  );
  function advance() {
    if (session.blocked) return;
    void session.write<LearningState>(
      "/steps/" + encodeURIComponent(step.id),
      "PUT",
    );
  }
  const allRequired =
    lesson.completion.requiredStepIds.every((id) =>
      session.progress.confirmedStepIds.includes(id),
    ) &&
    lesson.completion.requiredExerciseIds.every((id) =>
      session.progress.attempts.some((attempt) => attempt.exerciseId === id),
    );
  const pendingNavigation = (
    <PendingNavigation
      active={session.hasPendingWrite || ownedPending}
      onStay={() => heading.current?.focus({ preventScroll: true })}
    />
  );
  if (session.unavailable)
    return (
      <section className="page-arrive learning-page">
        {pendingNavigation}
        <div className="lesson-header">
          <h1 ref={heading} tabIndex={-1}>
            {session.unavailable === 410 ? "课程已撤回" : "学习记录暂不可用"}
          </h1>
          <p className="practice-intro">
            当前无法继续这堂课，已保存的历史学习记录不会因此删除。
          </p>
          <Link className="primary" to="/">
            回到今天
          </Link>
        </div>
      </section>
    );
  return (
    <section className="page-arrive learning-page">
      {pendingNavigation}
      <div className="lesson-header">
        <div className="crumb">
          {lesson.levelId.toUpperCase()} / {lesson.title.zh}
        </div>
        <h1 lang={lessonLanguage(lesson)}>
          {"target" in lesson.title ? lesson.title.target : lesson.title.fr}
        </h1>
      </div>
      <div className="learning-stage">
        {session.progress.completedAt ? (
          <>
            {session.uncertain && (
              <div role="status">
                <p>{session.error}</p>
                <button
                  className="primary"
                  aria-disabled={session.saving}
                  aria-busy={session.saving}
                  onClick={session.retry}
                >
                  {session.saving ? "正在确认" : "确认上次保存"}
                </button>
              </div>
            )}
            <h2 ref={heading} tabIndex={-1}>
              本课已完成
            </h2>
            <p className="practice-intro">
              阅读和练习记录已保存，表达已加入复习。
            </p>
            <ul className="practice-recap">
              {lesson.objectivesZh.map((objective) => (
                <li key={objective}>{objective}</li>
              ))}
            </ul>
            <Link className="primary" to="/reviews">
              复习表达
            </Link>
            <Link className="text-button practice-back" to="/">
              回到今天
            </Link>
          </>
        ) : (
          <>
            <div className="learning-step-heading">
              <h2 ref={heading} tabIndex={-1}>
                {step.titleZh}
              </h2>
              <span>
                {index + 1} / {lesson.steps.length}
              </span>
            </div>
            <div
              className="review-progress"
              role="progressbar"
              aria-label="本课步骤"
              aria-valuemin={0}
              aria-valuemax={lesson.steps.length}
              aria-valuenow={session.progress.confirmedStepIds.length}
            >
              <span
                style={{
                  width:
                    (session.progress.confirmedStepIds.length /
                      lesson.steps.length) *
                      100 +
                    "%",
                }}
              />
            </div>
            <div className="learning-blocks" key={step.id}>
              {step.blockIds.map((id) => {
                const block = lesson.blocks.find((block) => block.id === id);
                if (!block) throw Error("Missing lesson block");
                if (block.type === "dialogue" || block.type === "article")
                  return (
                    <ReadingBlock
                      key={block.id}
                      block={block}
                      lesson={lesson}
                    />
                  );
                if (block.type === "exercise")
                  return (
                    <ExerciseEditor
                      key={block.id}
                      draftKey={scope + ":answer:" + block.id}
                      confirmedSubmission={session.confirmedAttempts[block.id]}
                      block={block}
                      language={lessonLanguage(lesson)}
                      latest={session.progress.attempts
                        .filter((attempt) => attempt.exerciseId === block.id)
                        .at(-1)}
                      hinted={session.progress.hintedExerciseIds.includes(
                        block.id,
                      )}
                      blocked={session.blocked}
                      completed={!!session.progress.completedAt}
                      submit={(answer, onSaved) =>
                        session.write<AttemptResult>(
                          "/attempts",
                          "POST",
                          {
                            exerciseId: block.id,
                            answer,
                          },
                          onSaved,
                        )
                      }
                      hint={() =>
                        void session.write<HintResult>(
                          "/hints/" + encodeURIComponent(block.id),
                          "POST",
                        )
                      }
                    />
                  );
                return (
                  <TeachingBlock key={block.id} block={block} lesson={lesson} />
                );
              })}
            </div>
            <div className="learning-actions">
              {session.error && (
                <p className="error-message" role="alert">
                  {session.error}
                </p>
              )}
              {session.readFailed ? (
                <button
                  className="primary"
                  aria-disabled={session.saving}
                  aria-busy={session.saving}
                  onClick={() => void session.refresh()}
                >
                  {session.saving ? "正在读取" : "重新读取进度"}
                </button>
              ) : session.uncertain ? (
                <button
                  className="primary"
                  aria-disabled={session.saving}
                  aria-busy={session.saving}
                  onClick={session.retry}
                >
                  {session.saving ? "正在确认" : "重试保存"}
                  <Icon name="check" />
                </button>
              ) : index === lesson.steps.length - 1 &&
                session.progress.confirmedStepIds.includes(step.id) ? (
                <button
                  className="primary"
                  disabled={!session.restored || !allRequired}
                  aria-disabled={session.blocked || !allRequired}
                  aria-busy={session.saving}
                  onClick={() => {
                    if (!session.blocked)
                      void session.write<LearningState>("/complete", "POST");
                  }}
                >
                  {session.saving ? "正在保存" : "完成本课"}
                  <Icon name="check" />
                </button>
              ) : (
                <button
                  className="primary"
                  disabled={!session.restored || !canContinue}
                  aria-disabled={session.blocked || !canContinue}
                  aria-busy={session.saving}
                  onClick={advance}
                >
                  {session.saving
                    ? "正在保存"
                    : index === lesson.steps.length - 1
                      ? "确认回顾"
                      : "继续"}
                </button>
              )}
              {index > 0 && (
                <button
                  className="text-button"
                  disabled={session.blocked}
                  onClick={() => move(index - 1)}
                >
                  回看上一步
                </button>
              )}
              <p className="profile-note" role="status">
                {session.saving
                  ? "正在保存"
                  : session.uncertain
                    ? "这次提交尚未确认保存。"
                    : session.error
                      ? "请检查当前学习记录后重试。"
                      : "学习记录已保存到账号。"}
              </p>
            </div>
          </>
        )}
      </div>
    </section>
  );
}
