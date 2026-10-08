import { useLayoutEffect, useRef, useState } from "react";
import type { Block } from "@brioche/contracts/Block";
import type { NeutralBlock } from "@brioche/contracts/NeutralBlock";
import { lessonLanguage, type ReadingLesson } from "../lib/reading-model";
import type { ExerciseAnswer } from "@brioche/contracts/ExerciseAnswer";
import type { GradeResult } from "@brioche/contracts/GradeResult";
import {
  ApiRequestError,
  privateRequest as requestApi,
} from "../lib/api.client";
import { ExerciseEditor } from "./exercise-editor";
import { useLearning } from "./learning";

export function PreviewExercise({
  block,
  lesson,
}: {
  block: Extract<Block | NeutralBlock, { type: "exercise" }>;
  lesson: ReadingLesson;
}) {
  const audio = useLearning();
  const owner = audio.profile?.role === "operator" ? audio.profile.id : null;
  if (!owner) return null;
  return (
    <PreviewExerciseContent
      key={JSON.stringify([owner, lesson.id, lesson.revision, block.id])}
      block={block}
      lesson={lesson}
    />
  );
}

function PreviewExerciseContent({
  block,
  lesson,
}: {
  block: Extract<Block | NeutralBlock, { type: "exercise" }>;
  lesson: ReadingLesson;
}) {
  const audio = useLearning();
  const [busy, setBusy] = useState(false),
    [hinted, setHinted] = useState(false);
  const [latest, setLatest] = useState<{
    id: string;
    answer: ExerciseAnswer;
    result: GradeResult;
  }>();
  const pending = useRef(false),
    sequence = useRef(0),
    active = useRef(true);
  const controller = useRef<AbortController | null>(null);
  useLayoutEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
      controller.current?.abort();
    };
  }, []);
  async function submit(answer: ExerciseAnswer, onConfirmed: () => void) {
    if (pending.current || !active.current) return;
    const requestController = new AbortController();
    controller.current = requestController;
    pending.current = true;
    setBusy(true);
    try {
      const result = await requestApi<GradeResult>(
        `/api/${lesson.schemaVersion === "2.0" ? "v2" : "v1"}/operator/lessons/${encodeURIComponent(lesson.id)}/revisions/${lesson.revision}/grade`,
        "POST",
        { revision: lesson.revision, exerciseId: block.id, answer },
        requestController.signal,
      );
      if (active.current && !requestController.signal.aborted) {
        setLatest({ id: `preview-${++sequence.current}`, answer, result });
        onConfirmed();
      }
    } catch (error) {
      if (active.current && !requestController.signal.aborted)
        audio.toast(
          error instanceof ApiRequestError
            ? error.message
            : "预览判分暂时无法连接，请稍后重试。",
        );
    } finally {
      pending.current = false;
      if (active.current) setBusy(false);
    }
  }
  return (
    <ExerciseEditor
      block={block}
      language={lessonLanguage(lesson)}
      latest={latest}
      hinted={hinted}
      blocked={busy}
      completed={false}
      submit={submit}
      hint={() => setHinted(true)}
    />
  );
}
