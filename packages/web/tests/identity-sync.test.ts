import test from "node:test";
import assert from "node:assert/strict";
import {
  identityNoticeKey,
  watchIdentity,
  type Identity,
} from "../app/lib/identity-sync.ts";

const operator = { id: "one", role: "operator" };
const settle = () => new Promise<void>((resolve) => setImmediate(resolve));
function harness(read: (signal: AbortSignal) => Promise<Identity>) {
  const window = new EventTarget(),
    document = new EventTarget();
  const actions: string[] = [];
  let visible = true,
    invalidations = 0,
    stops = 0,
    canceled = false;
  let tick = () => {};
  const dispose = watchIdentity({
    identity: operator,
    window,
    document,
    visible: () => visible,
    read,
    invalidate: () => {
      actions.push("invalidate");
      invalidations++;
    },
    stopPlayback: () => {
      actions.push("stop");
      stops++;
    },
    every: (callback) => {
      tick = callback;
      return () => {
        canceled = true;
      };
    },
  });
  return {
    window,
    document,
    dispose,
    tick: () => tick(),
    hide: () => {
      visible = false;
    },
    show: () => {
      visible = true;
    },
    counts: () => ({ invalidations, stops, canceled }),
    actions,
    notice: (
      key = identityNoticeKey,
      newValue: string | null = "opaque-notice",
    ) => {
      const event = new Event("storage");
      Object.assign(event, { key, newValue });
      window.dispatchEvent(event);
    },
  };
}
test("another tab's auth notice stops playback before invalidation, exactly once", async () => {
  let signal: AbortSignal | undefined;
  const h = harness((value) => {
    signal = value;
    return new Promise(() => {});
  });
  h.notice("unrelated");
  h.notice(identityNoticeKey, null);
  assert.equal(h.counts().invalidations, 0);
  h.notice();
  h.notice();
  h.window.dispatchEvent(new Event("focus"));
  assert.equal(signal?.aborted, true);
  assert.deepEqual(h.actions, ["stop", "invalidate"]);
  assert.deepEqual(h.counts(), { invalidations: 1, stops: 1, canceled: false });
  h.dispose();
  assert.equal(h.counts().canceled, true);
});
test("same account remains; demotion, logout and account switch invalidate", async () => {
  for (const next of [
    null,
    { id: "two", role: "operator" },
    { id: "one", role: "learner" },
  ]) {
    let identity: Identity = operator;
    const h = harness(async () => identity);
    await settle();
    assert.equal(h.counts().invalidations, 0);
    identity = next;
    h.window.dispatchEvent(new Event("focus"));
    await settle();
    assert.equal(h.counts().invalidations, 1);
    assert.equal(h.counts().stops, 1);
    h.dispose();
  }
});
test("hidden tabs do not poll; visible checks recover from transport errors without overlapping", async () => {
  let calls = 0,
    resolve: ((value: Identity) => void) | undefined;
  const h = harness(async () => {
    calls++;
    if (calls === 1) throw Error("offline");
    return new Promise<Identity>((done) => {
      resolve = done;
    });
  });
  await settle();
  assert.equal(h.counts().invalidations, 0);
  h.hide();
  h.tick();
  assert.equal(calls, 1);
  h.show();
  h.document.dispatchEvent(new Event("visibilitychange"));
  h.tick();
  h.window.dispatchEvent(new Event("focus"));
  assert.equal(calls, 2);
  resolve!(null);
  await settle();
  assert.equal(h.counts().invalidations, 1);
  h.dispose();
});
test("disposed listeners and late identity responses cannot reload a newer page", async () => {
  let resolve: ((value: Identity) => void) | undefined;
  const h = harness(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  h.dispose();
  resolve!(null);
  await settle();
  h.notice();
  h.tick();
  h.window.dispatchEvent(new Event("focus"));
  assert.deepEqual(h.counts(), { invalidations: 0, stops: 0, canceled: true });
});
test("pagehide releases playback and BFCache restoration reauthorizes the whole route", async () => {
  const h = harness(async () => operator);
  await settle();
  h.window.dispatchEvent(new Event("pagehide"));
  assert.equal(h.counts().stops, 1);
  const event = new Event("pageshow");
  Object.assign(event, { persisted: true });
  h.window.dispatchEvent(event);
  assert.deepEqual(h.counts(), { invalidations: 1, stops: 2, canceled: false });
  h.dispose();
});
