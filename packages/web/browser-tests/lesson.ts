import type { PublicLesson } from "@brioche/contracts/PublicLesson";

const speaker = {
  id: "customer",
  labelZh: "顾客",
  characterId: "camille",
  displayName: "Camille",
  avatarId: "avatar-camille-v1",
};
export const lesson: PublicLesson = {
  schemaVersion: "1.0",
  id: "reading-protocol",
  revision: 1,
  levelId: "a1",
  unitId: "protocol",
  title: { fr: "Une journée", zh: "正文交互测试" },
  summaryZh: "仅供自动化协议验收",
  estimatedMinutes: 1,
  objectivesZh: ["切换正文"],
  knowledge: { vocabulary: [], grammar: [] },
  cast: [
    {
      characterId: "camille",
      revision: 1,
      displayName: "Camille",
      avatarId: "avatar-camille-v1",
      speechLocale: "fr-FR",
    },
  ],
  media: [],
  audio: [],
  audioTracks: [],
  reviewItemIds: [],
  completion: {
    strategy: "all-required",
    requiredStepIds: [],
    requiredExerciseIds: [],
  },
  blocks: [
    {
      type: "dialogue",
      id: "morning",
      titleZh: "早晨的问候",
      speakers: [speaker],
      turns: [
        {
          id: "morning-line",
          speakerId: speaker.id,
          segments: [
            {
              id: "morning-segment",
              text: "Bonjour !",
              vocabularyId: null,
              grammarId: null,
            },
          ],
          translationZh: "早上好！",
        },
      ],
    },
    {
      type: "article",
      id: "article",
      titleZh: "一天的短文",
      narratorId: "camille",
      paragraphs: [
        {
          id: "article-line",
          segments: [
            {
              id: "article-segment",
              text: "Camille va à la boulangerie.",
              vocabularyId: null,
              grammarId: null,
            },
          ],
          translationZh: "Camille 去面包店。",
        },
      ],
    },
    {
      type: "dialogue",
      id: "evening",
      titleZh: "第二段对话：再次来到面包店",
      speakers: [speaker],
      turns: [
        {
          id: "evening-line",
          speakerId: speaker.id,
          segments: [
            {
              id: "evening-segment",
              text: "Bonsoir !",
              vocabularyId: null,
              grammarId: null,
            },
          ],
          translationZh: "晚上好！",
        },
      ],
    },
  ],
  steps: [
    {
      id: "read",
      kind: "read",
      titleZh: "阅读",
      blockIds: ["morning", "article", "evening"],
    },
  ],
};
