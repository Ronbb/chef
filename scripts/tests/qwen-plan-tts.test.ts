import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { validatePlan, rustJson } from "../qwen-plan-tts.ts";
const hash = (value) =>
  createHash("sha256").update(rustJson(value)).digest("hex");
function fixture() {
  const profile = {
    personality: "Patient",
    speakingStyle: "Natural",
    defaultEmotion: "Friendly",
    provider: "qwen",
    model: "qwen-audio-3.1-tts-flash",
    voiceId: "longanlingxin_v3.1",
    voiceKind: "system",
    locale: "yue-Hant-HK",
    rate: 1,
    referenceAudio: null,
  };
  const voice = {
    characterId: "fixture",
    characterRevision: 1,
    voiceRevision: 1,
  };
  const text = "兩位，唔該！",
    emotion = "Friendly";
  const parameters = {
    model: profile.model,
    input: {
      text,
      voice: profile.voiceId,
      format: "wav",
      sample_rate: 24000,
      rate: 1,
      seed: 0,
      enable_aigc_tag: true,
      instruction: `请用自然的香港粤语朗读提供的原文，保持粤语声调和口语节奏，适合粤语初学者。不要用普通话，不要翻译，不要添加内容或朗读这些指令。 Character: Patient Speaking style: Natural Default emotion: Friendly Scene emotion: ${emotion}`,
    },
  };
  const words = [
    {
      segmentId: "segment",
      text: "兩位",
      segmentStart: 0,
      segmentEnd: 2,
      entryStart: 0,
      entryEnd: 2,
    },
    {
      segmentId: "segment",
      text: "唔該",
      segmentStart: 3,
      segmentEnd: 5,
      entryStart: 3,
      entryEnd: 5,
    },
  ];
  const request = {
    compilerVersion: "speech-plan-2/author-scalar-1",
    parameters,
    profile,
    voice,
    wordUnits: words.map((w) => ({
      text: w.text,
      start: w.entryStart,
      end: w.entryEnd,
    })),
  };
  const plan = {
    compilerVersion: request.compilerVersion,
    lessonId: "fixture",
    lessonRevision: 1,
    sourceHash: "0".repeat(64),
    planHash: "",
    targets: [
      {
        pointer: "/blocks/0/turns/0",
        blockId: "dialogue",
        entryId: "turn",
        text,
        voice,
        emotion,
        generationKey: "",
        words,
      },
    ],
    requests: {},
    totalRequestCharacters: 6,
  };
  return { plan, request };
}
function seal(plan, request) {
  const key = hash(request);
  plan.targets[0].generationKey = key;
  plan.requests = { [key]: request };
  plan.planHash = "";
  plan.planHash = hash(plan);
  return plan;
}
test("native paid plan binds source text, authored phrase units and dialect directions", () => {
  const { plan, request } = fixture();
  assert.equal(validatePlan(seal(plan, request)).length, 1);
  plan.totalRequestCharacters = 7;
  assert.throws(() => validatePlan(plan), /hash mismatch/);
});
test("rehashed plans still reject Mandarin voices, French hints and detached targets", () => {
  for (const change of [
    (p, r) => {
      r.profile.voiceId = r.parameters.input.voice = "yuxiaoyun_v3.1";
    },
    (p, r) => {
      r.parameters.input.language_hints = ["fr"];
    },
    (p) => {
      p.targets[0].text = "wrong text";
    },
    (p, r) => {
      r.wordUnits[0].end = 1;
    },
    (p, r) => {
      r.parameters.input.instruction = "Read in Mandarin";
    },
  ]) {
    const { plan, request } = fixture();
    change(plan, request);
    assert.throws(() => validatePlan(seal(plan, request)));
  }
});
