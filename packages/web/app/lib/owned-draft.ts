import type { StoredPending } from "./learning-draft";
import { draftScope, readDraft, storedKeys } from "./learning-draft.ts";
export type OwnedTarget =
  | {
      kind: "bookmark" | "enroll";
      knowledgeId: string;
      lessonId: string;
      revision: number;
    }
  | { kind: "preference"; cardId: string }
  | { kind: "rating"; cardId: string };
export function pendingOwned(
  userId: string,
): { key: string; job: StoredPending; target: OwnedTarget }[] {
  const owner = draftScope(userId, "owned", 1) + ":",
    reviews = draftScope(userId, "reviews", 1) + ":pending";
  const found: { key: string; job: StoredPending; target: OwnedTarget }[] = [];
  for (const key of storedKeys()) {
    if (!key.startsWith(owner) && key !== reviews) continue;
    const job = readDraft(key) as StoredPending | null;
    if (!job || typeof job.path !== "string" || !job.body) continue;
    let target: OwnedTarget | null = null;
    const match =
      /^\/api\/v1\/me\/reviews\/([A-Za-z0-9_-]{1,100})\/(preferences|attempts)$/.exec(
        job.path,
      );
    if (match)
      target =
        match[2] === "preferences"
          ? { kind: "preference", cardId: match[1] }
          : { kind: "rating", cardId: match[1] };
    else {
      const body = job.body,
        saved = /^\/api\/v1\/me\/saved-items\/([A-Za-z0-9_-]{1,100})$/.exec(
          job.path,
        );
      const knowledgeId = saved?.[1] ?? body.knowledgeId;
      if (
        typeof knowledgeId === "string" &&
        /^[A-Za-z0-9_-]{1,100}$/.test(knowledgeId) &&
        typeof body.sourceLessonId === "string" &&
        /^[A-Za-z0-9_-]{1,100}$/.test(body.sourceLessonId) &&
        Number.isSafeInteger(body.sourceRevision) &&
        (body.sourceRevision as number) > 0
      )
        target = {
          kind: saved ? "bookmark" : "enroll",
          knowledgeId,
          lessonId: body.sourceLessonId,
          revision: body.sourceRevision as number,
        };
    }
    if (
      target &&
      key ===
        (target.kind === "rating" ? reviews : owner + ownedTargetKey(target)) &&
      validOwnedPending(job, target)
    )
      found.push({ key, job, target });
  }
  return found;
}
export function ownedTargetKey(target: OwnedTarget) {
  return target.kind === "preference" || target.kind === "rating"
    ? target.kind + ":" + target.cardId
    : target.kind +
        ":" +
        target.knowledgeId +
        ":" +
        target.lessonId +
        ":" +
        target.revision;
}
export function validOwnedPending(
  value: unknown,
  target: OwnedTarget,
): value is StoredPending {
  if (!value || typeof value !== "object") return false;
  const job = value as StoredPending,
    body = job.body;
  if (
    !body ||
    typeof body !== "object" ||
    Array.isArray(body) ||
    typeof body.idempotencyKey !== "string" ||
    !/^[a-zA-Z0-9_-]{16,100}$/.test(body.idempotencyKey)
  )
    return false;
  const fields = Object.keys(body);
  if (target.kind === "rating")
    return (
      job.method === "POST" &&
      job.path ===
        "/api/v1/me/reviews/" +
          encodeURIComponent(target.cardId) +
          "/attempts" &&
      Number.isSafeInteger(body.cardVersion) &&
      (body.cardVersion as number) > 0 &&
      ["again", "remembered", "familiar"].includes(body.rating as string) &&
      fields.every((key) =>
        ["cardVersion", "rating", "idempotencyKey"].includes(key),
      )
    );
  if (target.kind === "preference")
    return (
      job.method === "PUT" &&
      job.path ===
        "/api/v1/me/reviews/" +
          encodeURIComponent(target.cardId) +
          "/preferences" &&
      Number.isSafeInteger(body.cardVersion) &&
      (body.cardVersion as number) > 0 &&
      typeof body.suspended === "boolean" &&
      fields.every((key) =>
        ["cardVersion", "suspended", "idempotencyKey"].includes(key),
      )
    );
  if (
    body.sourceLessonId !== target.lessonId ||
    body.sourceRevision !== target.revision
  )
    return false;
  if (target.kind === "enroll")
    return (
      job.method === "POST" &&
      job.path === "/api/v1/me/review-enrollments" &&
      body.knowledgeId === target.knowledgeId &&
      fields.every((key) =>
        [
          "knowledgeId",
          "sourceLessonId",
          "sourceRevision",
          "idempotencyKey",
        ].includes(key),
      )
    );
  return (
    job.method === "PUT" &&
    job.path ===
      "/api/v1/me/saved-items/" + encodeURIComponent(target.knowledgeId) &&
    Number.isSafeInteger(body.version) &&
    (body.version as number) >= 0 &&
    typeof body.saved === "boolean" &&
    fields.every((key) =>
      [
        "version",
        "saved",
        "sourceLessonId",
        "sourceRevision",
        "idempotencyKey",
      ].includes(key),
    )
  );
}
