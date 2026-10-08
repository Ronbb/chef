import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import type { AdminSpeechPlan } from "@brioche/contracts/AdminSpeechPlan";
import type { AdminSpeechTarget } from "@brioche/contracts/AdminSpeechTarget";
import { speechTargetLocale } from "../app/lib/speech-authoring.ts";

test("speech language follows the exact immutable character and voice revision", () => {
  const seed = JSON.parse(
    readFileSync(
      new URL("../../../docs/characters/voices.json", import.meta.url),
      "utf8",
    ),
  ).items[0];
  const old = { ...seed, voiceRevision: 1 };
  const newer = {
    ...seed,
    voiceRevision: 2,
    character: { ...seed.character, speechLocale: "yue-Hant-HK" },
    profile: { ...seed.profile, locale: "yue-Hant-HK" },
  };
  const plan = { voices: [newer, old] } as AdminSpeechPlan;
  const target = {
    voice: {
      characterId: seed.character.characterId,
      characterRevision: seed.character.revision,
      voiceRevision: 1,
    },
  } as AdminSpeechTarget;
  assert.equal(speechTargetLocale(plan, target), "fr-FR");
  target.voice.voiceRevision = 2;
  assert.equal(speechTargetLocale(plan, target), "yue-Hant-HK");
  target.voice.characterRevision += 1;
  assert.throws(() => speechTargetLocale(plan, target), /固定声音语言不一致/);
  assert.throws(() => speechTargetLocale(plan, undefined), /缺少对应片段/);
  target.voice.characterRevision -= 1;
  newer.profile.locale = "fr-FR";
  assert.throws(() => speechTargetLocale(plan, target), /固定声音语言不一致/);
});
