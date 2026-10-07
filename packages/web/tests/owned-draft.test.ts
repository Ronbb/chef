import test from "node:test";
import assert from "node:assert/strict";
import {
  ApiRequestError,
  definitiveWriteFailure,
} from "../app/lib/api.client.ts";
test("authentication, CSRF, throttling and transport failures keep the original request", () => {
  for (const status of [401, 403, 429, 500, 502, 503])
    assert.equal(
      definitiveWriteFailure(new ApiRequestError(status, "test")),
      false,
    );
  for (const status of [400, 404, 409, 410, 422])
    assert.equal(
      definitiveWriteFailure(new ApiRequestError(status, "test")),
      true,
    );
  for (const status of [400, 401, 403, 404, 409, 410, 422, 429, 500])
    assert.equal(
      definitiveWriteFailure(new ApiRequestError(status, "test", "csrf")),
      false,
    );
});
import {
  ownedTargetKey,
  pendingOwned,
  validOwnedPending,
  type OwnedTarget,
} from "../app/lib/owned-draft.ts";
import { draftScope } from "../app/lib/learning-draft.ts";
test("pending list finds removed queue items and ignores other owners or mismatched keys", () => {
  const key = draftScope("a", "reviews", 1) + ":pending";
  const job = {
    path: "/api/v1/me/reviews/removed-card/attempts",
    method: "POST",
    body: {
      cardVersion: 2,
      rating: "familiar",
      idempotencyKey: "original-rating-12345",
    },
  };
  const data: Record<string, string> = {
    [key]: JSON.stringify(job),
    [draftScope("b", "reviews", 1) + ":pending"]: JSON.stringify(job),
    [draftScope("a", "owned", 1) + ":invalid"]: JSON.stringify(job),
  };
  Object.defineProperty(globalThis, "sessionStorage", {
    configurable: true,
    value: { ...data, getItem: (k: string) => data[k] ?? null },
  });
  const found = pendingOwned("a");
  assert.equal(found.length, 1);
  assert.equal(found[0].key, key);
  assert.deepEqual(found[0].job, job);
});
test("owned operation recovery fixes endpoint, source, version and mutation fields", () => {
  const base = { idempotencyKey: "owned-operation-12345" };
  const cases: { target: OwnedTarget; job: object }[] = [
    {
      target: {
        kind: "bookmark",
        knowledgeId: "word",
        lessonId: "lesson",
        revision: 2,
      },
      job: {
        path: "/api/v1/me/saved-items/word",
        method: "PUT",
        body: {
          ...base,
          sourceLessonId: "lesson",
          sourceRevision: 2,
          version: 0,
          saved: true,
        },
      },
    },
    {
      target: {
        kind: "enroll",
        knowledgeId: "word",
        lessonId: "lesson",
        revision: 2,
      },
      job: {
        path: "/api/v1/me/review-enrollments",
        method: "POST",
        body: {
          ...base,
          knowledgeId: "word",
          sourceLessonId: "lesson",
          sourceRevision: 2,
        },
      },
    },
    {
      target: { kind: "preference", cardId: "card" },
      job: {
        path: "/api/v1/me/reviews/card/preferences",
        method: "PUT",
        body: { ...base, cardVersion: 3, suspended: true },
      },
    },
    {
      target: { kind: "rating", cardId: "card" },
      job: {
        path: "/api/v1/me/reviews/card/attempts",
        method: "POST",
        body: { ...base, cardVersion: 3, rating: "again" },
      },
    },
  ];
  assert.equal(new Set(cases.map((c) => ownedTargetKey(c.target))).size, 4);
  for (const { target, job } of cases) {
    assert.ok(validOwnedPending(job, target));
    const value = job as { path: string; method: string; body: object };
    for (const changed of [
      { ...value, path: "/api/v1/me/settings" },
      { ...value, method: "GET" },
      { ...value, body: { ...value.body, idempotencyKey: "invalid" } },
      { ...value, body: { ...value.body, score: 100 } },
    ])
      assert.ok(!validOwnedPending(changed, target));
  }
  const bookmark = cases[0];
  assert.ok(
    !validOwnedPending(bookmark.job, {
      kind: "bookmark",
      knowledgeId: "word",
      lessonId: "lesson",
      revision: 3,
    }),
  );
  const rating = cases[3].job as { body: object };
  for (const ratingValue of ["excellent", null, 100])
    assert.ok(
      !validOwnedPending(
        { ...cases[3].job, body: { ...rating.body, rating: ratingValue } },
        cases[3].target,
      ),
    );
  assert.ok(
    !validOwnedPending(
      { ...cases[3].job, body: { ...rating.body, cardVersion: 0 } },
      cases[3].target,
    ),
  );
});
