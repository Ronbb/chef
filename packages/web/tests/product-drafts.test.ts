import test from "node:test";
import assert from "node:assert/strict";
import {
  draftScope,
  saveDraft,
  readDraft,
  clearLearningDrafts,
  clearSessionDrafts,
  clearPending,
  draftsChangedEventFor,
} from "../app/lib/learning-draft.ts";
import { pendingOwned, ownedTargetKey } from "../app/lib/owned-draft.ts";
import { checkedNamespace } from "../app/lib/product-session.ts";
import {
  identityNoticeKeyFor,
  announceIdentityChange,
  watchIdentity,
} from "../app/lib/identity-sync.ts";

test("same account and identifiers keep pending operations, cleanup and notifications product-local", () => {
  const storage = Object.create(null) as Record<string, string>;
  Object.defineProperties(storage, {
    getItem: { value: (key: string) => storage[key] ?? null },
    setItem: {
      value: (key: string, value: string) => {
        storage[key] = value;
      },
    },
    removeItem: {
      value: (key: string) => {
        delete storage[key];
      },
    },
  });
  const window = new EventTarget();
  const priorStorage = Object.getOwnPropertyDescriptor(
    globalThis,
    "sessionStorage",
  );
  const priorWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  Object.defineProperty(globalThis, "sessionStorage", {
    configurable: true,
    value: storage,
  });
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: window,
  });
  try {
    const events: string[] = [];
    for (const product of ["brioche", "hargow"] as const)
      window.addEventListener(draftsChangedEventFor(product), () =>
        events.push(product),
      );
    const b = draftScope("same", "session", 1, "brioche"),
      h = draftScope("same", "session", 1, "hargow");
    assert.equal(b, "brioche.learning.v1:same:session:1");
    assert.notEqual(b, h);
    const original = {
      path: "/api/v1/learning-sessions/session/complete",
      method: "POST",
      body: { version: 1, idempotencyKey: "original-operation-12345" },
    };
    saveDraft(b + ":pending", original);
    saveDraft(h + ":pending", {
      ...original,
      body: { ...original.body, idempotencyKey: "hargow-operation-12345" },
    });
    assert.deepEqual(events, ["brioche", "hargow"]);
    clearPending(h + ":pending", original.body.idempotencyKey);
    assert.deepEqual(readDraft(b + ":pending"), original);
    assert.equal(
      (readDraft(h + ":pending") as typeof original).body.idempotencyKey,
      "hargow-operation-12345",
    );
    const target = {
      kind: "bookmark" as const,
      knowledgeId: "word",
      lessonId: "lesson",
      revision: 2,
    };
    const owned = {
      path: "/api/v1/me/saved-items/word",
      method: "PUT",
      body: {
        idempotencyKey: "owned-operation-12345",
        sourceLessonId: "lesson",
        sourceRevision: 2,
        version: 0,
        saved: true,
      },
    };
    for (const product of ["brioche", "hargow"] as const)
      saveDraft(
        draftScope("same", "owned", 1, product) + ":" + ownedTargetKey(target),
        owned,
      );
    assert.equal(pendingOwned("same", "brioche").length, 1);
    assert.equal(pendingOwned("same", "hargow").length, 1);
    assert.notEqual(
      pendingOwned("same", "brioche")[0].key,
      pendingOwned("same", "hargow")[0].key,
    );
    saveDraft(
      draftScope("other", "session", 1, "hargow") + ":pending",
      original,
    );
    clearSessionDrafts(h);
    assert.equal(readDraft(h + ":pending"), null);
    assert.deepEqual(readDraft(b + ":pending"), original);
    clearLearningDrafts("same", "hargow");
    assert.equal(pendingOwned("same", "hargow").length, 0);
    assert.equal(pendingOwned("same", "brioche").length, 1);
    assert.deepEqual(
      readDraft(draftScope("other", "session", 1, "hargow") + ":pending"),
      original,
    );
    assert.deepEqual(readDraft(b + ":pending"), original);
    clearLearningDrafts("same");
    assert.equal(readDraft(b + ":pending"), null);
  } finally {
    if (priorStorage)
      Object.defineProperty(globalThis, "sessionStorage", priorStorage);
    else Reflect.deleteProperty(globalThis, "sessionStorage");
    if (priorWindow) Object.defineProperty(globalThis, "window", priorWindow);
    else Reflect.deleteProperty(globalThis, "window");
  }
});

test("shared-origin identity notices only invalidate the configured product, once", () => {
  const window = new EventTarget(),
    document = new EventTarget();
  const counts = { brioche: 0, hargow: 0 },
    stopped = { brioche: 0, hargow: 0 };
  const disposers = (["brioche", "hargow"] as const).map((namespace) =>
    watchIdentity({
      namespace,
      identity: { id: "same", role: "learner" },
      window,
      document,
      visible: () => true,
      read: () => new Promise(() => {}),
      invalidate: () => {
        counts[namespace]++;
      },
      stopPlayback: () => {
        stopped[namespace]++;
      },
      every: () => () => {},
    }),
  );
  try {
    const notice = (namespace: "brioche" | "hargow") => {
      const event = new Event("storage");
      Object.assign(event, {
        key: identityNoticeKeyFor(namespace),
        newValue: "opaque-event",
      });
      window.dispatchEvent(event);
    };
    notice("hargow");
    notice("hargow");
    assert.deepEqual(counts, { brioche: 0, hargow: 1 });
    assert.deepEqual(stopped, { brioche: 0, hargow: 1 });
    notice("brioche");
    assert.deepEqual(counts, { brioche: 1, hargow: 1 });
  } finally {
    disposers.forEach((dispose) => dispose());
  }
});

test("trusted namespace validation rejects aliases; announcements contain only opaque values", () => {
  for (const value of ["", "Hargow", " hargow", "secret-invalid", null])
    assert.throws(() => checkedNamespace(value), {
      message: "Invalid product session namespace",
    });
  const values = new Map<string, string>();
  const prior = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: { setItem: (key: string, value: string) => values.set(key, value) },
  });
  try {
    announceIdentityChange("brioche");
    announceIdentityChange("hargow");
    assert.equal(values.size, 2);
    assert.equal(identityNoticeKeyFor(), "brioche.identity-change.v1");
    for (const value of values.values()) assert.match(value, /^[a-f0-9-]{36}$/);
  } finally {
    if (prior) Object.defineProperty(globalThis, "localStorage", prior);
    else Reflect.deleteProperty(globalThis, "localStorage");
  }
});
