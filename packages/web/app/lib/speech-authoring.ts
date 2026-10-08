import type { CharacterVoiceProfile } from "@brioche/contracts/CharacterVoiceProfile";
import { QWEN_MULTILINGUAL_SYSTEM_VOICES } from "@brioche/contracts/tts-voices";

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
