import type { CharacterVoiceProfile } from "@brioche/contracts/CharacterVoiceProfile";
import type { AdminSpeechPlan } from "@brioche/contracts/AdminSpeechPlan";
import type { AdminSpeechTarget } from "@brioche/contracts/AdminSpeechTarget";
import { QWEN_MULTILINGUAL_SYSTEM_VOICES } from "@brioche/contracts/tts-voices";

/** A plan keeps exact role/voice revisions; never infer its language from the current product. */
export function speechTargetLocale(
  plan: AdminSpeechPlan,
  target: AdminSpeechTarget | undefined,
) {
  if (!target) throw Error("配音计划缺少对应片段，请核对原计划。");
  const voice = plan.voices.find(
    (item) =>
      item.character.characterId === target.voice.characterId &&
      item.character.revision === target.voice.characterRevision &&
      item.voiceRevision === target.voice.voiceRevision,
  );
  if (
    !voice ||
    (voice.profile && voice.profile.locale !== voice.character.speechLocale)
  )
    throw Error("配音计划的固定声音语言不一致，请核对原计划。");
  speechAuthoring(voice.character.speechLocale);
  return voice.character.speechLocale;
}

/** Fixed role language, never a browser-selected provider locale. */
export function speechAuthoring(locale: string) {
  if (locale !== "fr-FR" && locale !== "yue-Hant-HK")
    throw Error("这个角色的语音语言尚未支持。");
  const cantonese = locale === "yue-Hant-HK";
  const label = cantonese ? "粤语" : "法语";
  const profile: CharacterVoiceProfile = {
    personality: "友善、自然、礼貌",
    speakingStyle: cantonese
      ? "Natural Hong Kong Cantonese, conversational, clear and unhurried."
      : "Clear natural French, conversational and unhurried.",
    defaultEmotion: "Warm, relaxed and polite.",
    provider: "qwen",
    model: "qwen-audio-3.1-tts-flash",
    voiceId: QWEN_MULTILINGUAL_SYSTEM_VOICES[0][0],
    voiceKind: "system",
    locale,
    rate: 1,
    referenceAudio: null,
  };
  return {
    label,
    profile,
    sampleText: cantonese
      ? "早晨！唔該，兩位。要紅茶，唔該。多謝，拜拜！"
      : "Bonjour ! Je voudrais une baguette, s’il vous plaît. C’est combien ? Merci, au revoir !",
    sampleEmotion:
      "Warm greeting, polite request, curious question, then a pleased farewell.",
    voices: QWEN_MULTILINGUAL_SYSTEM_VOICES.map(([value, name]) => ({
      value,
      label: name,
      detail: `${label} · Flash 3.1`,
    })),
  };
}
