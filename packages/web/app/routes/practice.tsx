import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { MAX_TEXT_ANSWER_UTF16_UNITS } from "@brioche/contracts/answer-limits";
import { Link, redirect } from "react-router";
import type { ExerciseAnswer } from "@brioche/contracts/ExerciseAnswer";
import type { GradeRequest } from "@brioche/contracts/GradeRequest";
import type { GradeResult } from "@brioche/contracts/GradeResult";
import { getCatalog, getLesson } from "../lib/api.server";
import { useLearning } from "../components/learning";
import { OrderEditor } from "../components/order-editor";
import { Icon } from "../components/icon";
import { LessonNote } from "../components/lesson-note";
import type { Route } from "./+types/practice";

export async function loader({ params }: Route.LoaderArgs) {
  const catalog = await getCatalog();
  if (!catalog.developmentFixture)
    throw redirect("/lessons/" + encodeURIComponent(params.lessonId));
  return { lesson: await getLesson(params.lessonId) };
}
export default function Practice({ loaderData }: Route.ComponentProps) {
  return (
    <PracticeSession
      key={loaderData.lesson.id + ":" + loaderData.lesson.revision}
      {...loaderData}
    />
  );
}
function PracticeSession({ lesson }: Route.ComponentProps["loaderData"]) {
  const learning = useLearning();
  const exercises = lesson.blocks.filter((b) => b.type === "exercise");
  const [index, setIndex] = useState(0),
    [choice, setChoice] = useState(""),
    [text, setText] = useState(""),
    [order, setOrder] = useState<string[]>([]),
    [result, setResult] = useState<GradeResult | null>(null),
    [results, setResults] = useState<Record<string, GradeResult>>({}),
    [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const busy = useRef(false),
    controller = useRef<AbortController | null>(null),
    heading = useRef<HTMLHeadingElement>(null),
    feedback = useRef<HTMLDivElement>(null),
    form = useRef<HTMLFormElement>(null),
    active = useRef(true);
  useLayoutEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
      controller.current?.abort();
    };
  }, []);
  useEffect(() => {
    if (!result) return;
    const frame = requestAnimationFrame(() => feedback.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [result]);
  const current = exercises[index];
  const ready =
    current &&
    (current.exerciseType === "single-choice"
      ? !!choice
      : current.exerciseType === "fill-blank"
        ? !!text.trim()
        : order.length === current.tokens.length);
  async function submit() {
    if (!current || !ready || busy.current || !active.current) return;
    busy.current = true;
    setPending(true);
    setError("");
    const answer: ExerciseAnswer =
      current.exerciseType === "single-choice"
        ? { kind: "choice", optionId: choice }
        : current.exerciseType === "fill-blank"
          ? { kind: "text", text }
          : { kind: "order", tokenIds: order };
    const body: GradeRequest = {
      revision: lesson.revision,
      exerciseId: current.id,
      answer,
    };
    const request = new AbortController();
    controller.current = request;
    let failure = "答案未提交成功，可以重试。";
    try {
      const response = await fetch(
        "/api/demo/lessons/" + encodeURIComponent(lesson.id) + "/grade",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
          signal: AbortSignal.any([request.signal, AbortSignal.timeout(10000)]),
        },
      );
      if (!response.ok) {
        if (response.status === 409)
          failure = "课程版本已变化，请重新打开课程。";
        throw Error(failure);
      }
      const feedback = (await response.json()) as GradeResult;
      if (!active.current || request.signal.aborted) return;
      setResult(feedback);
      setResults((old) => ({ ...old, [current.id]: feedback }));
    } catch {
      if (active.current && !request.signal.aborted) {
        setError(failure);
        learning.toast(failure);
      }
    } finally {
      busy.current = false;
      if (active.current) setPending(false);
    }
  }
  function next() {
    learning.stop();
    setIndex((i) => i + 1);
    setChoice("");
    setText("");
    setOrder([]);
    setResult(null);
    setError("");
    requestAnimationFrame(() => {
      heading.current?.focus();
      heading.current?.scrollIntoView({ block: "start", behavior: "instant" });
    });
  }
  if (!current)
    return (
      <section className="page-arrive practice-page">
        <h1 tabIndex={-1} ref={heading}>
          本次练习
        </h1>
        <p className="practice-intro">
          完成了 {Object.keys(results).length} 道题，其中{" "}
          {Object.values(results).filter((r) => r.correct).length} 道答对。
        </p>
        <ul className="practice-recap">
          {exercises.map((e) => (
            <li key={e.id}>
              <span>{e.promptZh}</span>
              <span>{results[e.id]?.correct ? "答对了" : "再巩固"}</span>
            </li>
          ))}
        </ul>
        {lesson.blocks
          .filter((b) => b.type === "habit")
          .map((b) => (
            <LessonNote kind="habit" title="带进日常" key={b.id}>
              <p>{b.taskZh}</p>
              <p className="profile-note">{b.alternativeZh}</p>
            </LessonNote>
          ))}
        {lesson.blocks
          .filter((b) => b.type === "summary")
          .map((b) => (
            <div className="lesson-note" key={b.id}>
              <h2>今天能做到</h2>
              <ul>
                {b.takeawaysZh.map((t) => (
                  <li key={t}>{t}</li>
                ))}
              </ul>
            </div>
          ))}
        <Link className="primary" to={"/review/" + lesson.id}>
          复习表达
        </Link>
        <Link
          className="text-button practice-back"
          to={"/lessons/" + lesson.id}
        >
          回看课程
        </Link>
        <p className="profile-note">演示结果尚未保存到账号。</p>
      </section>
    );
  return (
    <section className="page-arrive practice-page">
      <div className="review-session-header">
        <div>
          <h1>练习</h1>
          <p>{lesson.title.zh}</p>
        </div>
        <span>
          {index + 1} / {exercises.length}
        </span>
      </div>
      <div
        className="review-progress"
        role="progressbar"
        aria-label="练习进度"
        aria-valuemin={0}
        aria-valuemax={exercises.length}
        aria-valuenow={index}
      >
        <span
          style={{ width: (index / Math.max(1, exercises.length)) * 100 + "%" }}
        />
      </div>
      <form
        ref={form}
        className="exercise-sheet"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <h2 ref={heading} tabIndex={-1}>
          {current.promptZh}
        </h2>
        <fieldset disabled={pending || !!result}>
          <legend className="sr-only">你的答案</legend>
          {current.exerciseType === "single-choice" && (
            <div className="practice-options">
              {current.options.map((o) => (
                <label
                  key={o.id}
                  className={
                    "practice-option" + (choice === o.id ? " is-selected" : "")
                  }
                >
                  <input
                    type="radio"
                    name="answer"
                    value={o.id}
                    checked={choice === o.id}
                    onChange={() => setChoice(o.id)}
                  />
                  <span>{o.text}</span>
                  <Icon name="check" />
                </label>
              ))}
            </div>
          )}
          {current.exerciseType === "fill-blank" && (
            <>
              <p className="practice-sentence" lang="fr">
                {current.templateFr}
              </p>
              <label className="answer-label" htmlFor="blank-answer">
                填写冠词或表达
              </label>
              <input
                className="practice-input"
                id="blank-answer"
                lang="fr"
                autoComplete="off"
                autoCapitalize="none"
                spellCheck={false}
                maxLength={MAX_TEXT_ANSWER_UTF16_UNITS}
                value={text}
                onChange={(e) => setText(e.target.value)}
              />
              {!!current.hintZh.trim() && (
                <details className="practice-hint">
                  <summary>提示</summary>
                  <p>{current.hintZh}</p>
                </details>
              )}
            </>
          )}
          {current.exerciseType === "order" && (
            <OrderEditor
              tokens={current.tokens}
              order={order}
              onChange={setOrder}
            />
          )}
        </fieldset>
        {result && (
          <div
            ref={feedback}
            tabIndex={-1}
            className={
              "practice-feedback" + (result.correct ? " is-correct" : "")
            }
            role="status"
          >
            <strong>{result.correct ? "答对了" : "再看看这个表达"}</strong>
            <p>{result.feedbackZh}</p>
          </div>
        )}
        {error && (
          <p className="error-message" role="status">
            {error}
          </p>
        )}
        {result ? (
          <div className="practice-next">
            <button type="button" className="primary" onClick={next}>
              {index + 1 === exercises.length ? "查看回顾" : "下一题"}
            </button>
            {!result.correct && (
              <button
                type="button"
                className="text-button"
                onClick={() => {
                  setResult(null);
                  requestAnimationFrame(() =>
                    form.current
                      ?.querySelector<HTMLElement>(
                        "fieldset input:not(:disabled), fieldset button:not(:disabled)",
                      )
                      ?.focus(),
                  );
                }}
              >
                再试一次
              </button>
            )}
          </div>
        ) : (
          <button
            type="submit"
            className="primary"
            disabled={!ready}
            aria-disabled={!ready || pending}
            aria-busy={pending}
          >
            {pending ? "正在确认" : error ? "重新提交" : "确认答案"}
            <Icon name="check" />
          </button>
        )}
      </form>
      <Link className="text-button practice-back" to={"/lessons/" + lesson.id}>
        回看课程
      </Link>
    </section>
  );
}
