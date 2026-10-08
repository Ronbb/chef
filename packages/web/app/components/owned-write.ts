import { productNamespace } from "../lib/product-runtime";
import { useEffect, useRef, useState } from "react";
import {
  ApiRequestError,
  definitiveWriteFailure,
  privateRequest,
} from "../lib/api.client";
import { operationKey } from "../lib/operation-key";
import {
  clearPending,
  draftScope,
  readDraft,
  saveDraft,
} from "../lib/learning-draft";
import {
  ownedTargetKey,
  validOwnedPending,
  type OwnedTarget,
} from "../lib/owned-draft";
type Job<T> = {
  path: string;
  body: object;
  method: "PUT" | "POST";
  accept: (result: T) => void;
};
export function useOwnedWrite<T>(
  refresh: (() => Promise<void>) | undefined,
  recovery: {
    userId?: string;
    target: OwnedTarget;
    accept: (result: T) => void;
    onUnavailable?: (status: 404 | 410) => void;
  },
) {
  const storageKey = recovery.userId
    ? draftScope(recovery.userId, "owned", 1, productNamespace) +
      ":" +
      ownedTargetKey(recovery.target)
    : "";
  const [saving, setSaving] = useState(false),
    [ready, setReady] = useState(false),
    [uncertain, setUncertain] = useState(false),
    [error, setError] = useState("");
  const pending = useRef<Job<T> | null>(null),
    busy = useRef(false),
    alive = useRef(true),
    generation = useRef(0),
    recoveryRef = useRef(recovery);
  recoveryRef.current = recovery;
  useEffect(() => {
    generation.current++;
    alive.current = true;
    busy.current = false;
    pending.current = null;
    setSaving(false);
    setUncertain(false);
    setError("");
    const stored = storageKey ? readDraft(storageKey) : null;
    if (validOwnedPending(stored, recoveryRef.current.target)) {
      pending.current = {
        ...stored,
        accept: (result) => recoveryRef.current.accept(result),
      };
      setUncertain(true);
      setError("上次保存尚未确认，请重试原提交。");
    } else if (storageKey) saveDraft(storageKey, null);
    setReady(!!storageKey);
    return () => {
      alive.current = false;
    };
  }, [storageKey]);
  useEffect(() => {
    if (!saving && !uncertain) return;
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [saving, uncertain]);
  async function send(job: Job<T>) {
    if (busy.current || !alive.current) return;
    const gen = generation.current,
      key = storageKey;
    if (
      !key ||
      !validOwnedPending(job, recoveryRef.current.target) ||
      !saveDraft(key, { path: job.path, method: job.method, body: job.body })
    ) {
      setError("浏览器无法保留这次提交，请允许本地存储后重试。");
      return;
    }
    busy.current = true;
    pending.current = job;
    setSaving(true);
    setError("");
    try {
      const result = await privateRequest<T>(job.path, job.method, job.body);
      clearPending(key, (job.body as Record<string, unknown>).idempotencyKey);
      if (alive.current && gen === generation.current) {
        pending.current = null;
        setUncertain(false);
        job.accept(result);
      }
    } catch (failure) {
      if (!alive.current || gen !== generation.current) return;
      if (
        failure instanceof ApiRequestError &&
        definitiveWriteFailure(failure)
      ) {
        clearPending(key, (job.body as Record<string, unknown>).idempotencyKey);
        pending.current = null;
        setUncertain(false);
        if (failure.status === 404 || failure.status === 410)
          recoveryRef.current.onUnavailable?.(failure.status);
        if (failure.status === 409) {
          try {
            await refresh?.();
          } catch {
            /* retain last known state */
          }
        }
        setError(
          failure.status === 409
            ? "其他设备已更新，请确认最新记录后重试。"
            : failure.status === 410
              ? "来源内容已撤回，暂时无法操作。"
              : failure.message,
        );
      } else {
        setUncertain(true);
        setError("保存尚未确认，请重试原提交。");
      }
    } finally {
      if (gen === generation.current) {
        busy.current = false;
        if (alive.current) setSaving(false);
      }
    }
  }
  return {
    saving,
    uncertain,
    error,
    blocked: saving || uncertain || !ready,
    write: (
      path: string,
      body: object,
      accept: Job<T>["accept"],
      method: "PUT" | "POST" = "PUT",
    ) => {
      if (ready && !pending.current)
        void send({
          path,
          method,
          body: { ...body, idempotencyKey: operationKey() },
          accept,
        });
    },
    retry: () => {
      if (pending.current) void send(pending.current);
    },
  };
}
