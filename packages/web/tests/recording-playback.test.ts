import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import type { PublicLesson } from "@brioche/contracts/PublicLesson";
import {
  RecordingPlayer,
  readingUnits,
  readingScope,
  wordUnit,
  continuousRecording,
  knowledgeUnit,
  type RecordingClip,
} from "../app/lib/recording-playback.ts";
const url = "/api/audio/" + "a".repeat(64) + ".mp3";
const clip: RecordingClip = {
  url,
  startMs: 1000,
  endMs: 2000,
  cues: [
    { startMs: 1000, endMs: 2000, id: "sentence" },
    { startMs: 1200, endMs: 1500, id: "sentence", wordId: "word" },
  ],
};
test("knowledge snapshots preserve fixed intervals and reject unsafe media references", () => {
  const recording = {
    asset: {
      assetId: "audio-term",
      revision: 2,
      sha256: "a".repeat(64),
      mimeType: "audio/mpeg",
      durationMs: 1000,
      creditZh: "Synthetic protocol fixture",
      url,
    },
    startMs: 50,
    endMs: 900,
  };
  const unit = knowledgeUnit("saved-term", "une baguette", recording);
  assert.equal(unit.recording?.url, url);
  assert.equal(unit.recording?.startMs, 50);
  assert.equal(unit.recording?.endMs, 900);
  assert.equal(unit.recording?.cues[0].id, "saved-term");
  assert.equal(knowledgeUnit("old", "bonjour").recording, undefined);
  for (const altered of [
    { ...recording, endMs: 1001 },
    { ...recording, startMs: 900 },
    { ...recording, startMs: -1 },
    {
      ...recording,
      asset: { ...recording.asset, url: "https://example.test/audio.mp3" },
    },
    { ...recording, asset: { ...recording.asset, sha256: "b".repeat(64) } },
  ])
    assert.equal(
      knowledgeUnit("unsafe", "bonjour", altered).recording,
      undefined,
    );
});
class FakeMedia {
  src = "";
  preload = "";
  currentTime = 0;
  duration = 10;
  readyState = 1;
  paused = true;
  playbackRate = 1;
  preservesPitch = false;
  loads = 0;
  plays = 0;
  pauses = 0;
  listeners = new Map<string, Set<() => void>>();
  pending: { resolve: () => void; reject: (error: Error) => void }[] = [];
  play() {
    this.plays++;
    this.paused = false;
    return new Promise<void>((resolve, reject) =>
      this.pending.push({ resolve, reject }),
    );
  }
  pause() {
    this.pauses++;
    this.paused = true;
    this.emit("pause");
  }
  load() {
    this.loads++;
  }
  removeAttribute(name: string) {
    if (name === "src") this.src = "";
  }
  addEventListener(name: string, cb: () => void) {
    if (!this.listeners.has(name)) this.listeners.set(name, new Set());
    this.listeners.get(name)!.add(cb);
  }
  removeEventListener(name: string, cb: () => void) {
    this.listeners.get(name)?.delete(cb);
  }
  emit(name: string) {
    for (const cb of [...(this.listeners.get(name) ?? [])]) cb();
  }
}
function setup() {
  const media = new FakeMedia();
  const frames = new Map<number, FrameRequestCallback>();
  let next = 0;
  const player = new RecordingPlayer(
    () => media as unknown as HTMLAudioElement,
    {
      request: (cb) => {
        frames.set(++next, cb);
        return next;
      },
      cancel: (id) => {
        frames.delete(id);
      },
    },
  );
  const statuses: string[] = [];
  const progress: [number, string | null, string | null][] = [];
  const errors: boolean[] = [];
  let ended = 0;
  const callbacks = {
    status: (value: string) => statuses.push(value),
    progress: (fraction: number, id: string | null, word: string | null) =>
      progress.push([fraction, id, word]),
    end: () => ended++,
    error: (blocked: boolean) => errors.push(blocked),
  };
  return {
    media,
    player,
    frames,
    statuses,
    progress,
    errors,
    callbacks,
    get ended() {
      return ended;
    },
  };
}
test("actual media time controls progress, word highlight and clip completion", async () => {
  const s = setup();
  s.player.play(clip, 1.25, s.callbacks);
  assert.equal(s.media.currentTime, 1);
  assert.equal(s.media.playbackRate, 1.25);
  assert.equal(s.media.preservesPitch, true);
  s.media.pending[0].resolve();
  await Promise.resolve();
  s.media.currentTime = 1.25;
  s.media.emit("timeupdate");
  assert.deepEqual(s.progress.at(-1), [0.25, "sentence", "word"]);
  s.player.pause();
  const count = s.progress.length;
  s.media.currentTime = 1.4;
  s.media.emit("timeupdate");
  assert.equal(s.progress.length, count);
  s.player.setRate(0.75);
  assert.equal(s.media.currentTime, 1.4);
  assert.equal(s.media.playbackRate, 0.75);
  s.player.resume();
  s.media.pending[1].resolve();
  await Promise.resolve();
  assert.equal(s.media.currentTime, 1.4);
  s.media.currentTime = 2.01;
  s.media.emit("timeupdate");
  assert.equal(s.ended, 1);
  assert.equal(s.player.isActive, false);
  assert.equal(s.frames.size, 0);
  s.media.emit("ended");
  assert.equal(s.ended, 1);
});
test("old listeners, frame callbacks and rejected play promises cannot interrupt a newer clip", async () => {
  const s = setup();
  s.player.play(clip, 1, s.callbacks);
  s.media.emit("playing");
  const oldError = [...s.media.listeners.get("error")!][0];
  const oldMetadata = [...s.media.listeners.get("loadedmetadata")!][0];
  const oldFrame = [...s.frames.values()][0];
  s.player.play({ ...clip, startMs: 3000, endMs: 4000 }, 1, s.callbacks);
  s.media.emit("playing");
  s.media.emit("pause"); // A previously queued native pause must not pause the new playing element.
  assert.notEqual(s.statuses.at(-1), "paused");
  const currentFrames = s.frames.size;
  oldError();
  oldMetadata();
  oldFrame(0);
  s.media.pending[0].reject(new Error("old failure"));
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(s.errors.length, 0);
  assert.equal(s.media.currentTime, 3);
  assert.equal(s.player.isActive, true);
  assert.equal(s.frames.size, currentFrames);
  assert.equal(s.media.loads, 1, "same file reuses the media element");
  s.player.stop();
  assert.equal(s.media.src, "");
  assert.equal(s.frames.size, 0);
  s.media.pending[1].resolve();
  await Promise.resolve();
  assert.equal(s.player.isActive, false);
});
test("pending play remains paused and blocked playback does not masquerade as success", async () => {
  const s = setup();
  s.player.play(clip, 1, s.callbacks);
  s.player.pause();
  s.media.pending[0].resolve();
  await Promise.resolve();
  assert.equal(s.media.paused, true);
  assert.equal(s.statuses.includes("playing"), false);
  s.player.resume();
  const error = new Error("gesture required");
  error.name = "NotAllowedError";
  s.media.pending[1].reject(error);
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(s.errors, [true]);
  assert.equal(s.player.isActive, false);
  assert.equal(s.media.src, "");
  s.player.play(clip, 1, s.callbacks);
  s.media.emit("error");
  assert.deepEqual(s.errors, [true, false]);
});
test("pause and resume ignore settlement of the superseded play attempt in the same clip", async () => {
  const s = setup();
  s.player.play(clip, 1, s.callbacks);
  s.player.pause();
  s.player.resume();
  const interrupted = new Error("the earlier play was interrupted by pause");
  interrupted.name = "AbortError";
  s.media.pending[0].reject(interrupted);
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(s.errors, []);
  assert.equal(s.player.isActive, true);
  assert.equal(s.statuses.at(-1), "loading");
  s.media.pending[1].resolve();
  await Promise.resolve();
  assert.equal(s.statuses.at(-1), "playing");
  assert.equal(s.media.currentTime, 1);
  s.player.stop();

  const other = setup();
  other.player.play(clip, 1, other.callbacks);
  other.player.pause();
  other.player.resume();
  other.media.pending[0].resolve();
  await Promise.resolve();
  assert.equal(other.statuses.at(-1), "loading");
  assert.equal(other.frames.size, 0);
  other.media.pending[1].resolve();
  await Promise.resolve();
  assert.equal(other.statuses.at(-1), "playing");
  other.player.stop();
});
test("late metadata seeks correctly; short media and early EOF fail; external pause is tracked", async () => {
  const s = setup();
  s.media.readyState = 0;
  s.player.play(clip, 1, s.callbacks);
  s.media.currentTime = 0;
  s.media.emit("loadedmetadata");
  assert.equal(s.media.currentTime, 1);
  s.media.pending[0].resolve();
  await Promise.resolve();
  s.media.pause();
  assert.equal(s.statuses.at(-1), "paused");
  assert.equal(s.frames.size, 0);
  s.player.stop();
  s.media.duration = 0.5;
  s.player.play(clip, 1, s.callbacks);
  s.media.emit("loadedmetadata");
  assert.deepEqual(s.errors, [false]);
  s.media.duration = 10;
  s.player.play(clip, 1, s.callbacks);
  s.media.currentTime = 1.2;
  s.media.emit("ended");
  assert.deepEqual(s.errors, [false, false]);
});
test("Unicode word targets select exact clips; absent annotations use only the clicked word", () => {
  const lesson = JSON.parse(
    readFileSync(
      new URL("../../../docs/examples/a1-bakery.lesson.json", import.meta.url),
      "utf8",
    ),
  ) as PublicLesson;
  const block = lesson.blocks.find((block) => block.type === "dialogue")!;
  if (block.type !== "dialogue") throw Error("fixture");
  const entry = block.turns[0];
  const segment = entry.segments[0];
  segment.text = "🥐 Bonjour";
  lesson.audio = [
    {
      assetId: "audio",
      revision: 1,
      sha256: "a".repeat(64),
      mimeType: "audio/mpeg",
      durationMs: 10000,
      creditZh: "协议音",
      url,
    },
  ];
  lesson.audioTracks = [
    {
      blockId: block.id,
      assetId: "audio",
      cues: [
        ...block.turns.map((turn, i) => ({
          entryId: turn.id,
          startMs: i * 1000,
          endMs: (i + 1) * 1000,
        })),
        {
          entryId: entry.id,
          segmentId: segment.id,
          wordRange: { start: 2, end: 9 },
          startMs: 100,
          endMs: 700,
        },
      ],
    },
  ];
  const word = wordUnit(
    lesson,
    block.id,
    entry.id,
    segment.id,
    "Bonjour",
    segment.text,
    3,
  );
  assert.equal(word.recording?.startMs, 100);
  assert.equal(word.recording?.endMs, 700);
  assert.ok(word.id.endsWith(":2:9"));
  assert.notEqual(
    word.id,
    wordUnit(
      lesson,
      block.id,
      block.turns[1].id,
      segment.id,
      "Bonjour",
      segment.text,
      3,
    ).id,
    "segment IDs may repeat in different entries",
  );
  const missing = wordUnit(
    lesson,
    block.id,
    entry.id,
    segment.id,
    "bonjour",
    segment.text,
    4,
  );
  assert.equal(missing.recording, undefined);
  assert.equal(missing.text, "bonjour");
  const units = readingUnits(lesson, block);
  const continuous = continuousRecording(units)!;
  assert.equal(continuous.startMs, 0);
  assert.equal(continuous.endMs, block.turns.length * 1000);
  assert.equal(continuous.url, url);
  assert.equal(continuous.cues.find((cue) => cue.wordId)?.wordId, word.id);
  assert.ok(units[0].id.startsWith(readingScope(lesson, block.id)));
  lesson.audio[0].url = "https://untrusted.example/audio.mp3";
  assert.equal(readingUnits(lesson, block)[0].recording, undefined);
});
