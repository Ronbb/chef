import { useEffect, useId, useRef, useState } from "react";
import { MAX_TEXT_ANSWER_UTF16_UNITS } from "@brioche/contracts/answer-limits";
import { readDraft, saveDraft, validAnswer } from "../lib/learning-draft";
import type { Block } from "@brioche/contracts/Block";
import type { NeutralBlock } from "@brioche/contracts/NeutralBlock";
import type { AttemptRecord } from "@brioche/contracts/AttemptRecord";
import type { ExerciseAnswer } from "@brioche/contracts/ExerciseAnswer";
import { OrderEditor } from "./order-editor";
import { Icon } from "./icon";
import { useLearning } from "./learning";
export function ExerciseEditor({
  block,
  latest,
  hinted,
  blocked,
  completed,
  submit,
  hint,
  draftKey,
  confirmedSubmission,
  language = "fr-FR",
}: {
  block: Extract<Block | NeutralBlock, { type: "exercise" }>;
  language?: string;
  latest?: Pick<AttemptRecord, "id" | "answer" | "result">;
  hinted: boolean;
  blocked: boolean;
  completed: boolean;
  submit: (answer: ExerciseAnswer, onSaved: () => void) => Promise<unknown>;
  hint: () => void;
  draftKey?: string;
  confirmedSubmission?: string;
}) {
  const [choice, setChoice] = useState(
      latest?.answer.kind === "choice" ? latest.answer.optionId : "",
    ),
    [text, setText] = useState(
      latest?.answer.kind === "text" ? latest.answer.text : "",
    ),
    [order, setOrder] = useState<string[]>(
      latest?.answer.kind === "order" ? latest.answer.tokenIds : [],
    ),
    [editing, setEditing] = useState(!latest),
    [draftConflict, setDraftConflict] = useState(false);
  const previous = useRef(latest?.id);
  const previousConfirmation = useRef(confirmedSubmission);
  const form = useRef<HTMLFormElement>(null),
    feedback = useRef<HTMLDivElement>(null),
    focusFeedback = useRef(false),
    hintContent = useRef<HTMLParagraphElement>(null),
    focusHint = useRef(false),
    answerId = useId();
  const audio = useLearning(),
    storageWarning = useRef(false);
  useEffect(() => {
    const changed = latest?.id !== previous.current;
    previous.current = latest?.id;
    if (!draftKey) {
      if (changed) setEditing(false);
      return;
    }
    if (
      confirmedSubmission &&
      confirmedSubmission !== previousConfirmation.current
    ) {
      previousConfirmation.current = confirmedSubmission;
      focusFeedback.current = true;
      saveDraft(draftKey, null);
      setEditing(false);
      setDraftConflict(false);
      return;
    }
    const draft = readDraft(draftKey) as {
      answer?: unknown;
      baseline?: unknown;
    } | null;
    if (draft && validAnswer(draft.answer, block)) {
      if (draft.answer.kind === "choice") setChoice(draft.answer.optionId);
      if (draft.answer.kind === "text") setText(draft.answer.text);
      if (draft.answer.kind === "order") setOrder(draft.answer.tokenIds);
      setEditing(true);
      setDraftConflict(draft.baseline !== (latest?.id ?? null));
    } else {
      saveDraft(draftKey, null);
      setDraftConflict(false);
      if (changed) setEditing(false);
    }
  }, [draftKey, latest?.id, confirmedSubmission]);
  function keep(answer: ExerciseAnswer) {
    if (!draftKey) return;
    if (
      !saveDraft(draftKey, { answer, baseline: latest?.id ?? null }) &&
      !storageWarning.current
    ) {
      storageWarning.current = true;
      audio.toast("浏览器无法保存草稿，离开前请保留答案。");
    }
  }
  const shownChoice =
    !editing && latest?.answer.kind === "choice"
      ? latest.answer.optionId
      : choice;
  const shownText =
    !editing && latest?.answer.kind === "text" ? latest.answer.text : text;
  const shownOrder =
    !editing && latest?.answer.kind === "order"
      ? latest.answer.tokenIds
      : order;
  const result = !editing ? latest?.result : null;
  useEffect(() => {
    if (!hinted || !focusHint.current || !hintContent.current) return;
    focusHint.current = false;
    const frame = requestAnimationFrame(() => hintContent.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [hinted]);
  useEffect(() => {
    if (!result || !focusFeedback.current) return;
    focusFeedback.current = false;
    const frame = requestAnimationFrame(() => feedback.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [result, confirmedSubmission]);
  const ready =
    block.exerciseType === "single-choice"
      ? !!choice
      : block.exerciseType === "fill-blank"
        ? !!text.trim()
        : order.length === block.tokens.length;
  return (
    <form
      ref={form}
      className="exercise-sheet"
      onSubmit={(event) => {
        event.preventDefault();
        if (!ready || blocked || completed) return;
        void submit(
          block.exerciseType === "single-choice"
            ? { kind: "choice", optionId: choice }
            : block.exerciseType === "fill-blank"
              ? { kind: "text", text }
              : { kind: "order", tokenIds: order },
          () => {
            focusFeedback.current = true;
            if (draftKey) saveDraft(draftKey, null);
            setEditing(false);
          },
        );
      }}
    >
      <h2>{block.promptZh}</h2>
      {draftConflict && editing && (
        <p className="profile-note" role="status">
          这道题已有新的提交。你的草稿仍保留，确认后会记录一次新尝试。
        </p>
      )}
      <fieldset disabled={blocked || !!result || completed}>
        <legend className="sr-only">你的答案</legend>
        {block.exerciseType === "single-choice" && (
          <div className="practice-options">
            {block.options.map((option) => (
              <label
                key={option.id}
                className={
                  "practice-option" +
                  (shownChoice === option.id ? " is-selected" : "")
                }
              >
                <input
                  type="radio"
                  name={block.id + "-answer"}
                  checked={shownChoice === option.id}
                  value={option.id}
                  onChange={() => {
                    setChoice(option.id);
                    keep({ kind: "choice", optionId: option.id });
                  }}
                />
                <span>{option.text}</span>
                <Icon name="check" />
              </label>
            ))}
          </div>
        )}
        {block.exerciseType === "fill-blank" && (
          <>
            <p className="practice-sentence" lang={language}>
              {"templateTarget" in block
                ? block.templateTarget
                : block.templateFr}
            </p>
            <label className="answer-label" htmlFor={answerId}>
              你的答案
            </label>
            <input
              className="practice-input"
              id={answerId}
              lang={language}
              autoComplete="off"
              autoCapitalize="none"
              spellCheck={false}
              maxLength={MAX_TEXT_ANSWER_UTF16_UNITS}
              value={shownText}
              onChange={(event) => {
                setText(event.target.value);
                keep({ kind: "text", text: event.target.value });
              }}
            />
          </>
        )}
        {block.exerciseType === "order" && (
          <OrderEditor
            language={language}
            tokens={block.tokens}
            order={shownOrder}
            onChange={(next) => {
              setOrder(next);
              keep({ kind: "order", tokenIds: next });
            }}
          />
        )}
      </fieldset>
      {block.exerciseType === "fill-blank" &&
        !!block.hintZh.trim() &&
        (hinted ? (
          <p className="profile-note" ref={hintContent} tabIndex={-1}>
            {block.hintZh}
          </p>
        ) : (
          <button
            className="text-button practice-hint"
            type="button"
            disabled={blocked || completed}
            onClick={() => {
              focusHint.current = true;
              hint();
            }}
          >
            提示
          </button>
        ))}
      {result && (
        <div
          className={
            "practice-feedback" + (result.correct ? " is-correct" : "")
          }
          role="status"
          ref={feedback}
          tabIndex={-1}
        >
          <strong>{result.correct ? "答对了" : "再看看这个表达"}</strong>
          <p>{result.feedbackZh}</p>
        </div>
      )}
      {!completed &&
        (result ? (
          <button
            key="retry"
            type="button"
            className="text-button"
            disabled={blocked}
            onClick={() => {
              setEditing(true);
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
        ) : (
          <button
            key="submit"
            type="submit"
            className="primary"
            disabled={!ready}
            aria-disabled={blocked || !ready}
            aria-busy={blocked}
          >
            确认答案
            <Icon name="check" />
          </button>
        ))}
    </form>
  );
}
