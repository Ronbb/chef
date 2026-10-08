import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import type { NeutralLesson } from "@brioche/contracts/NeutralLesson";
import {
  readingTokens,
  segmentText,
  lessonLanguage,
} from "../app/lib/reading-model.ts";
import { rangeWordUnit, readingUnits } from "../app/lib/recording-playback.ts";

test("authored scalar words preserve Cantonese pronunciation, punctuation and supplementary characters", () => {
  const segment = {
    id: "s",
    vocabularyId: null,
    grammarId: null,
    reading: {
      text: "𠮷，你好！",
      words: [
        { start: 0, end: 1 },
        { start: 2, end: 4 },
      ],
      pronunciations: [
        {
          range: { start: 2, end: 4 },
          system: "jyutping" as const,
          text: "nei5 hou2",
        },
      ],
    },
  };
  assert.deepEqual(readingTokens(segment), [
    { text: "𠮷", start: 0, end: 1, word: true },
    { text: "，", start: 1, end: 2, word: false },
    { text: "你好", start: 2, end: 4, word: true, pronunciation: "nei5 hou2" },
    { text: "！", start: 4, end: 5, word: false },
  ]);
  assert.equal(
    readingTokens(segment)
      .map((t) => t.text)
      .join(""),
    segmentText(segment),
  );
  assert.throws(() =>
    readingTokens({
      ...segment,
      reading: { ...segment.reading, words: [{ start: 2, end: 6 }] },
    }),
  );
  assert.throws(() =>
    readingTokens({
      ...segment,
      reading: {
        ...segment.reading,
        words: [
          { start: 0, end: 3 },
          { start: 2, end: 4 },
        ],
      },
    }),
  );
});
test("native reading and word playback reuse measured scalar cues without guessing a recording", () => {
  const lesson = JSON.parse(
    readFileSync(
      new URL(
        "../../../crates/server/tests/fixtures/neutral-cantonese.lesson.json",
        import.meta.url,
      ),
      "utf8",
    ),
  ) as NeutralLesson;
  const block = lesson.blocks.find((b) => b.type === "article")!;
  assert.equal(block.type, "article");
  if (block.type !== "article") throw new Error("fixture");
  const hash = "a".repeat(64);
  lesson.audio = [
    {
      assetId: "audio-native",
      revision: 1,
      sha256: hash,
      mimeType: "audio/wav",
      durationMs: 1000,
      creditZh: "Protocol fixture",
      url: `/api/audio/${hash}.wav`,
    },
  ];
  const entry = block.paragraphs[0],
    segment = entry.segments[0];
  lesson.audioTracks = [
    {
      blockId: block.id,
      assetId: "audio-native",
      cues: [
        {
          entryId: entry.id,
          segmentId: null,
          wordRange: null,
          startMs: 0,
          endMs: 1000,
        },
        {
          entryId: entry.id,
          segmentId: segment.id,
          wordRange: { start: 0, end: 2 },
          startMs: 100,
          endMs: 900,
        },
      ],
    },
  ];
  const tokens = readingTokens(segment);
  assert.equal(tokens.length, 1);
  assert.equal(tokens[0].text, "你好");
  assert.equal(lessonLanguage(lesson), "yue-Hant-HK");
  const word = rangeWordUnit(
    lesson,
    block.id,
    entry.id,
    segment.id,
    tokens[0].text,
    tokens[0].start,
    tokens[0].end,
  );
  assert.equal(word.recording?.startMs, 100);
  assert.equal(word.recording?.endMs, 900);
  assert.equal(word.locale, "yue-Hant-HK");
  const units = readingUnits(lesson, block);
  assert.equal(units[0].text, "你好");
  assert.equal(units[0].recording?.cues[1].wordId, word.id);
  assert.equal(
    rangeWordUnit(lesson, block.id, entry.id, segment.id, "你", 0, 1).recording,
    undefined,
  );
});
