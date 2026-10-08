import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  draftScope,
  readDraft,
  saveDraft,
  clearLearningDrafts,
  clearSessionDrafts,
  clearPending,
  validAnswer,
  validPending,
} from "../app/lib/learning-draft.ts";
import type { PublicLesson } from "@brioche/contracts/PublicLesson";
import type { NeutralLesson } from "@brioche/contracts/NeutralLesson";
const lesson = JSON.parse(
  readFileSync(
    new URL("../../../docs/examples/a1-bakery.lesson.json", import.meta.url),
    "utf8",
  ),
) as PublicLesson;
const exercise = lesson.blocks.find(
  (block) =>
    block.type === "exercise" && block.exerciseType === "single-choice",
)!;

test("v2 native pending recovery fixes session and authored exercises while retaining exact legacy requests", () => {
  const native = JSON.parse(
    readFileSync(
      new URL(
        "../../../crates/server/tests/fixtures/neutral-cantonese.lesson.json",
        import.meta.url,
      ),
      "utf8",
    ),
  ) as NeutralLesson;
  const job = {
    path: "/api/v2/learning-sessions/native-session/attempts",
    method: "POST",
    body: {
      exerciseId: "text",
      answer: { kind: "text", text: "點心" },
      version: 2,
      idempotencyKey: "pending-native-operation",
    },
  };
  const original = structuredClone(job);
  assert.ok(validPending(job, "native-session", native));
  assert.deepEqual(job, original);
  for (const path of [
    "/api/v3/learning-sessions/native-session/attempts",
    "/api/v2/learning-sessions/other/attempts",
    "/api/v2/learning-sessions/native-session/attempts?product=brioche",
    "https://example.test" + job.path,
  ])
    assert.equal(
      validPending({ ...job, path }, "native-session", native),
      false,
    );
  for (const version of ["v1", "v2"]) {
    const old = {
      path: `/api/${version}/learning-sessions/existing-session/steps/${lesson.steps[0].id}`,
      method: "PUT",
      body: { version: 3, idempotencyKey: "existing-immutable-operation" },
    };
    const before = JSON.stringify(old);
    assert.ok(validPending(old, "existing-session", lesson));
    assert.equal(JSON.stringify(old), before);
  }
});
test("restorable text answers use the same UTF-16 units as browser maxlength", () => {
  const block = lesson.blocks.find(
    (block) => block.type === "exercise" && block.exerciseType === "fill-blank",
  )!;
  if (block.type !== "exercise") throw Error("fixture");
  for (const text of [
    "a".repeat(1024),
    "é".repeat(1024),
    "😀".repeat(512),
    "İ".repeat(1024),
  ])
    assert.ok(validAnswer({ kind: "text", text }, block));
  for (const text of [
    "a".repeat(1025),
    "😀".repeat(513),
    "e\u0301".repeat(513),
  ])
    assert.equal(validAnswer({ kind: "text", text }, block), false);
});
test("pending recovery accepts only this fixed session's valid mutations", () => {
  assert.equal(exercise.type, "exercise");
  if (exercise.type !== "exercise" || exercise.exerciseType !== "single-choice")
    throw Error("fixture");
  const job = {
    path: "/api/v1/learning-sessions/session-one/attempts",
    method: "POST",
    body: {
      exerciseId: exercise.id,
      answer: { kind: "choice", optionId: exercise.options[0].id },
      version: 1,
      idempotencyKey: "pending-operation-12345",
    },
  };
  assert.ok(validPending(job, "session-one", lesson));
  assert.ok(!validPending(job, "session-two", lesson));
  for (const changed of [
    { ...job, path: "https://elsewhere.test/steal" },
    { ...job, method: "GET" },
    { ...job, body: { ...job.body, version: -1 } },
    { ...job, body: { ...job.body, version: 1.5 } },
    { ...job, body: { ...job.body, idempotencyKey: "bad" } },
    {
      ...job,
      body: { ...job.body, answer: { kind: "choice", optionId: "unknown" } },
    },
    { ...job, body: { ...job.body, score: 100 } },
  ])
    assert.ok(!validPending(changed, "session-one", lesson));
  assert.ok(
    validPending(
      {
        ...job,
        path:
          "/api/v1/learning-sessions/session-one/steps/" + lesson.steps[0].id,
        method: "PUT",
        body: { version: 1, idempotencyKey: "pending-operation-12345" },
      },
      "session-one",
      lesson,
    ),
  );
  const order = lesson.blocks.find(
    (b) => b.type === "exercise" && b.exerciseType === "order",
  );
  if (order?.type !== "exercise" || order.exerciseType !== "order")
    throw Error("fixture order");
  assert.ok(validAnswer({ kind: "order", tokenIds: [] }, order));
  assert.ok(
    !validAnswer(
      { kind: "order", tokenIds: [order.tokens[0].id, order.tokens[0].id] },
      order,
    ),
  );
});
test("tab storage separates owners and revisions and clears only the intended owner or session", () => {
  const data: Record<string, string> = {};
  const storage = {
    getItem: (k: string) => data[k] ?? null,
    setItem: (k: string, v: string) => {
      data[k] = v;
      Object.defineProperty(storage, k, {
        configurable: true,
        enumerable: true,
        value: v,
      });
    },
    removeItem: (k: string) => {
      delete data[k];
      delete (storage as Record<string, unknown>)[k];
    },
  };
  Object.defineProperty(globalThis, "sessionStorage", {
    configurable: true,
    value: storage,
  });
  const a = draftScope("a", "session", 1) + ":pending",
    b = draftScope("b", "session", 1) + ":pending";
  assert.notEqual(a, b);
  assert.notEqual(a, draftScope("a", "session", 2) + ":pending");
  assert.ok(saveDraft(a, { idempotencyKey: "exact", answer: "é" }));
  assert.deepEqual(readDraft(a), { idempotencyKey: "exact", answer: "é" });
  saveDraft(a, { body: { idempotencyKey: "newer" } });
  clearPending(a, "older");
  assert.deepEqual(readDraft(a), { body: { idempotencyKey: "newer" } });
  clearPending(a, "newer");
  assert.equal(readDraft(a), null);
  saveDraft(b, { other: true });
  const anotherSession = draftScope("a", "session-other", 1) + ":answer:test",
    anotherRevision = draftScope("a", "session", 2) + ":pending",
    withdrawnScope = draftScope("a", "session", 1);
  saveDraft(anotherSession, { answer: "kept" });
  saveDraft(anotherRevision, { revision: 2 });
  saveDraft(withdrawnScope + ":answer:test", { answer: "removed" });
  saveDraft(withdrawnScope + ":step", "step-read");
  saveDraft(a, { body: { idempotencyKey: "pending" } });
  clearSessionDrafts(withdrawnScope);
  assert.equal(readDraft(a), null);
  assert.equal(readDraft(withdrawnScope + ":answer:test"), null);
  assert.equal(readDraft(withdrawnScope + ":step"), null);
  assert.deepEqual(readDraft(anotherSession), { answer: "kept" });
  assert.deepEqual(readDraft(anotherRevision), { revision: 2 });
  assert.deepEqual(readDraft(b), { other: true });
  clearLearningDrafts("a");
  assert.equal(readDraft(a), null);
  assert.deepEqual(readDraft(b), { other: true });
  storage.setItem(a, "{broken");
  assert.equal(readDraft(a), null);
  Object.defineProperty(globalThis, "sessionStorage", {
    configurable: true,
    get() {
      throw Error("storage denied");
    },
  });
  assert.equal(readDraft(a), null);
  assert.equal(saveDraft(a, { pending: true }), false);
  clearLearningDrafts("a");
  clearSessionDrafts(withdrawnScope);
});
