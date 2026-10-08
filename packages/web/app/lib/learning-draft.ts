import type { ExerciseAnswer } from "@brioche/contracts/ExerciseAnswer";
import type { PublicLesson } from "@brioche/contracts/PublicLesson";
import { MAX_TEXT_ANSWER_UTF16_UNITS } from "@brioche/contracts/answer-limits";

import { checkedNamespace, type SessionNamespace } from "./product-session.ts";
function prefix(namespace: SessionNamespace = "brioche") {
  return `${checkedNamespace(namespace)}.learning.v1:`;
}
export function draftsChangedEventFor(namespace: SessionNamespace = "brioche") {
  return `${checkedNamespace(namespace)}:learning-drafts`;
}
// Only a change notification; account data and request bodies stay in storage.
export const draftsChangedEvent = "brioche:learning-drafts";
function notifyDraftsChanged(namespace: SessionNamespace = "brioche") {
  if (
    typeof window !== "undefined" &&
    typeof window.dispatchEvent === "function"
  )
    window.dispatchEvent(new Event(draftsChangedEventFor(namespace)));
}
export function draftScope(
  userId: string,
  sessionId: string,
  revision: number,
  namespace: SessionNamespace = "brioche",
) {
  return (
    prefix(namespace) +
    encodeURIComponent(userId) +
    ":" +
    encodeURIComponent(sessionId) +
    ":" +
    revision
  );
}
export function readDraft(key: string): unknown {
  try {
    const raw = sessionStorage.getItem(key);
    return raw && raw.length <= 32768 ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}
export function storedKeys() {
  try {
    return Object.keys(sessionStorage);
  } catch {
    return [];
  }
}
export function saveDraft(key: string, value: unknown): boolean {
  try {
    if (value === null) sessionStorage.removeItem(key);
    else sessionStorage.setItem(key, JSON.stringify(value));
    notifyDraftsChanged(
      key.startsWith(prefix("hargow")) ? "hargow" : "brioche",
    );
    return true;
  } catch {
    return false;
  }
}
export function clearLearningDrafts(
  userId: string,
  namespace: SessionNamespace = "brioche",
) {
  const owner = prefix(namespace) + encodeURIComponent(userId) + ":";
  try {
    for (const key of Object.keys(sessionStorage))
      if (key.startsWith(owner)) sessionStorage.removeItem(key);
    notifyDraftsChanged(namespace);
  } catch {
    /* private browsing may deny storage */
  }
}
export function clearSessionDrafts(scope: string) {
  for (const key of storedKeys())
    if (key.startsWith(scope + ":")) saveDraft(key, null);
}
export function clearPending(key: string, idempotencyKey: unknown) {
  const stored = readDraft(key) as {
    body?: { idempotencyKey?: unknown };
  } | null;
  if (
    typeof idempotencyKey === "string" &&
    stored?.body?.idempotencyKey === idempotencyKey
  )
    saveDraft(key, null);
}
export function validAnswer(
  value: unknown,
  block: Extract<PublicLesson["blocks"][number], { type: "exercise" }>,
): value is ExerciseAnswer {
  if (!value || typeof value !== "object") return false;
  const answer = value as Record<string, unknown>;
  if (block.exerciseType === "single-choice")
    return (
      answer.kind === "choice" &&
      typeof answer.optionId === "string" &&
      block.options.some((option) => option.id === answer.optionId)
    );
  if (block.exerciseType === "fill-blank")
    return (
      answer.kind === "text" &&
      typeof answer.text === "string" &&
      answer.text.length <= MAX_TEXT_ANSWER_UTF16_UNITS
    );
  return (
    answer.kind === "order" &&
    Array.isArray(answer.tokenIds) &&
    answer.tokenIds.length <= block.tokens.length &&
    new Set(answer.tokenIds).size === answer.tokenIds.length &&
    answer.tokenIds.every(
      (id) =>
        typeof id === "string" && block.tokens.some((token) => token.id === id),
    )
  );
}
export type StoredPending = {
  path: string;
  method: "POST" | "PUT";
  body: Record<string, unknown>;
};
export function validPending(
  value: unknown,
  sessionId: string,
  lesson: PublicLesson,
): value is StoredPending {
  if (!value || typeof value !== "object") return false;
  const job = value as StoredPending,
    base = "/api/v1/learning-sessions/" + sessionId;
  if (
    !job.body ||
    typeof job.body !== "object" ||
    !Number.isSafeInteger(job.body.version) ||
    (job.body.version as number) < 0 ||
    typeof job.body.idempotencyKey !== "string" ||
    !/^[a-zA-Z0-9_-]{16,100}$/.test(job.body.idempotencyKey)
  )
    return false;
  const fields = Object.keys(job.body);
  if (job.path === base + "/attempts" && job.method === "POST") {
    const block = lesson.blocks.find(
      (block) => block.type === "exercise" && block.id === job.body.exerciseId,
    );
    return (
      fields.every((key) =>
        ["version", "idempotencyKey", "exerciseId", "answer"].includes(key),
      ) &&
      block?.type === "exercise" &&
      validAnswer(job.body.answer, block)
    );
  }
  if (!fields.every((key) => ["version", "idempotencyKey"].includes(key)))
    return false;
  return (
    (job.method === "POST" &&
      (job.path === base + "/complete" ||
        lesson.blocks.some(
          (block) =>
            block.type === "exercise" &&
            job.path === base + "/hints/" + encodeURIComponent(block.id),
        ))) ||
    (job.method === "PUT" &&
      lesson.steps.some(
        (step) => job.path === base + "/steps/" + encodeURIComponent(step.id),
      ))
  );
}
