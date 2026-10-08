import { useEffect, useRef, useState } from "react";
import type { ReadingSession } from "../lib/reading-model";
import type { NeutralLearningSession } from "@brioche/contracts/NeutralLearningSession";
import type { LearningState } from "@brioche/contracts/LearningState";
import type { AttemptResult } from "@brioche/contracts/AttemptResult";
import type { HintResult } from "@brioche/contracts/HintResult";
import {
  ApiRequestError,
  definitiveWriteFailure,
  privateRequest,
} from "../lib/api.client";
import { operationKey } from "../lib/operation-key";
import {
  clearPending,
  clearSessionDrafts,
  readDraft,
  saveDraft,
  validPending,
  sessionMutationSuffix,
} from "../lib/learning-draft";
type Result = LearningState | AttemptResult | HintResult;
type Pending = {
  path: string;
  method: "POST" | "PUT";
  body: object;
  onSaved?: () => void;
};
export function useLearningSession(initial: ReadingSession, scope: string) {
  const [progress, setProgress] = useState(initial.progress),
    [saving, setSaving] = useState(false),
    [error, setError] = useState(""),
    [restored, setRestored] = useState(false),
    [confirmedAttempts, setConfirmedAttempts] = useState<
      Record<string, string>
    >({}),
    [uncertain, setUncertain] = useState(false),
    [readFailed, setReadFailed] = useState(false),
    [stepConfirmation, setStepConfirmation] = useState<{
      id: string;
      key: string;
    } | null>(null),
    [unavailable, setUnavailable] = useState<404 | 410 | null>(null);
  const latest = useRef(initial.progress),
    busy = useRef(false),
    pending = useRef<Pending | null>(null),
    alive = useRef(true),
    stale = useRef(false),
    removed = useRef(false);
  const hasPendingWrite = !!pending.current;
  function removeUnavailable(status: 404 | 410) {
    removed.current = true;
    stale.current = true;
    pending.current = null;
    clearSessionDrafts(scope);
    setUncertain(false);
    setReadFailed(false);
    setUnavailable(status);
    setError("");
  }
  useEffect(() => {
    alive.current = true;
    const restored = readDraft(scope + ":pending");
    if (validPending(restored, initial.progress.id, initial.lesson)) {
      pending.current = restored;
      setUncertain(true);
      setError("上次提交尚未确认，请重试原提交。");
    } else saveDraft(scope + ":pending", null);
    setRestored(true);
    return () => {
      alive.current = false;
    };
  }, [scope]);
  useEffect(() => {
    if (!hasPendingWrite) return;
    const beforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", beforeUnload);
    return () => window.removeEventListener("beforeunload", beforeUnload);
  }, [hasPendingWrite]);
  function accept(value: LearningState) {
    if (value.version < latest.current.version) return;
    latest.current = value;
    setProgress(value);
  }
  async function readLatest() {
    let fresh: NeutralLearningSession;
    try {
      fresh = await privateRequest<NeutralLearningSession>(
        "/api/v2/learning-sessions/" + initial.progress.id,
        "GET",
      );
    } catch (failure) {
      if (
        alive.current &&
        failure instanceof ApiRequestError &&
        failure.phase === "request" &&
        (failure.status === 404 || failure.status === 410)
      ) {
        removeUnavailable(failure.status);
        return false;
      }
      throw failure;
    }
    if (!alive.current) return false;
    accept(fresh.progress);
    stale.current = false;
    setReadFailed(false);
    return true;
  }
  async function refresh() {
    if (busy.current || pending.current || removed.current || !alive.current)
      return;
    busy.current = true;
    setSaving(true);
    try {
      if ((await readLatest()) && alive.current)
        setError("最新进度已读取，请检查当前记录后再确认。");
    } catch {
      if (alive.current)
        setError("最新进度暂时无法读取。答案仍保留，请重新读取后继续。");
    } finally {
      busy.current = false;
      if (alive.current) setSaving(false);
    }
  }
  async function send<T extends Result>(job: Pending): Promise<T | null> {
    if (busy.current || removed.current || !alive.current) return null;
    busy.current = true;
    pending.current = job;
    if (
      !saveDraft(scope + ":pending", {
        path: job.path,
        method: job.method,
        body: job.body,
      })
    ) {
      busy.current = false;
      pending.current = null;
      setError("浏览器无法保留这次提交，请允许本地存储后重试。");
      return null;
    }
    setSaving(true);
    setError("");
    try {
      const result = await privateRequest<T>(job.path, job.method, job.body);
      clearPending(
        scope + ":pending",
        (job.body as Record<string, unknown>).idempotencyKey,
      );
      if (!alive.current) return null;
      const confirmedProgress: LearningState =
        "progress" in result ? result.progress : result;
      accept(confirmedProgress);
      const fields = job.body as Record<string, unknown>;
      const confirmedStep =
        job.method === "PUT"
          ? initial.lesson.steps.find(
              (step) =>
                sessionMutationSuffix(job.path, initial.progress.id) ===
                "/steps/" + encodeURIComponent(step.id),
            )
          : undefined;
      if (
        confirmedStep &&
        typeof fields.idempotencyKey === "string" &&
        confirmedProgress.confirmedStepIds.includes(confirmedStep.id)
      ) {
        setStepConfirmation({
          id: confirmedStep.id,
          key: fields.idempotencyKey,
        });
      }
      if (
        typeof fields.exerciseId === "string" &&
        typeof fields.idempotencyKey === "string"
      ) {
        const exerciseId = fields.exerciseId,
          key = fields.idempotencyKey;
        saveDraft(scope + ":answer:" + exerciseId, null);
        setConfirmedAttempts((old) => ({ ...old, [exerciseId]: key }));
      }
      pending.current = null;
      setUncertain(false);
      job.onSaved?.();
      return result;
    } catch (failure) {
      if (!alive.current) return null;
      if (
        failure instanceof ApiRequestError &&
        failure.phase === "request" &&
        (failure.status === 404 || failure.status === 410)
      ) {
        removeUnavailable(failure.status);
        return null;
      }
      if (
        failure instanceof ApiRequestError &&
        definitiveWriteFailure(failure)
      ) {
        clearPending(
          scope + ":pending",
          (job.body as Record<string, unknown>).idempotencyKey,
        );
        pending.current = null;
        setUncertain(false);
        if (failure.status === 409) {
          stale.current = true;
          setReadFailed(true);
          try {
            if (!(await readLatest())) return null;
          } catch {
            if (alive.current)
              setError("最新进度暂时无法读取。答案仍保留，请重新读取后继续。");
            return null;
          }
        }
        if (!alive.current) return null;
        setError(
          failure.status === 409
            ? "另一处学习进度已更新，请检查当前记录后再确认。"
            : failure.status === 400
              ? "请检查答案后再确认。"
              : failure.message,
        );
      } else {
        // Keep the exact body/key. A server commit may have happened before the connection failed.
        setUncertain(true);
        setError("保存尚未确认。重试会确认原提交，答案已保留在此标签页。");
      }
      return null;
    } finally {
      busy.current = false;
      if (alive.current) setSaving(false);
    }
  }
  function write<T extends Result>(
    suffix: string,
    method: "POST" | "PUT",
    fields: object = {},
    onSaved?: () => void,
  ): Promise<T | null> {
    if (pending.current || busy.current || stale.current)
      return Promise.resolve(null);
    return send<T>({
      path: "/api/v2/learning-sessions/" + initial.progress.id + suffix,
      method,
      body: {
        ...fields,
        version: latest.current.version,
        idempotencyKey: operationKey(),
      },
      onSaved,
    });
  }
  function retry() {
    if (pending.current) void send(pending.current);
  }
  return {
    progress,
    confirmedAttempts,
    stepConfirmation,
    restored,
    saving,
    error,
    uncertain,
    readFailed,
    unavailable,
    hasPendingWrite,
    blocked: saving || uncertain || readFailed || !!unavailable || !restored,
    write,
    retry,
    refresh,
  };
}
