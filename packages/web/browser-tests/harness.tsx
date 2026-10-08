import { productNamespace } from "../app/lib/product-runtime";
import product from "@chef/product";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import {
  createMemoryRouter,
  RouterProvider,
  Navigate,
  useLoaderData,
  Link,
} from "react-router";
import { StartLearning } from "../app/components/start-learning";
import { ExerciseEditor } from "../app/components/exercise-editor";
import type { ExerciseAnswer } from "@brioche/contracts/ExerciseAnswer";
import type { GradeResult } from "@brioche/contracts/GradeResult";
import { LearningProvider, useLearning } from "../app/components/learning";
import { LessonContent } from "../app/routes/lesson";
import nativeSource from "../../../crates/server/tests/fixtures/neutral-cantonese.lesson.json";
import type { NeutralLesson } from "@brioche/contracts/NeutralLesson";
import { lesson } from "./lesson";
import { ChoiceDialog } from "../app/components/choice-dialog";
import Profile from "../app/routes/profile";
import { LearningSessionContent } from "../app/routes/learning";
import type { ReadingLesson } from "../app/lib/reading-model";
import Reviews from "../app/routes/reviews";
import Library from "../app/routes/library";
import PendingSaves from "../app/routes/pending-saves";
import { HomeContent as Home } from "../app/routes/home";
import { CoursesContent as Courses } from "../app/routes/courses";
import History from "../app/routes/review-history";
import { Account } from "../app/components/account";
import AuthorPreview from "../app/routes/author-preview";
import { LessonAudioReview } from "../app/components/admin-lesson-audio-review";
import type { AdminLessonAudioStatus } from "@brioche/contracts/AdminLessonAudioStatus";
import Practice from "../app/routes/practice";
import type { Block } from "@brioche/contracts/Block";
import { Scrollbar } from "../app/components/scrollbar";
import type { PreviewRelease } from "@brioche/contracts/PreviewRelease";
import type { ReviewHistoryPage } from "@brioche/contracts/ReviewHistoryPage";
import type { Catalog } from "@brioche/contracts/Catalog";
import type { StudyDashboard } from "@brioche/contracts/StudyDashboard";
import {
  clearPending,
  clearSessionDrafts,
  draftScope,
  saveDraft,
} from "../app/lib/learning-draft";
import { ownedTargetKey } from "../app/lib/owned-draft";
import type { ReadingSavedItem as SavedItem } from "../app/lib/reading-model";
import type { ReviewCard } from "@brioche/contracts/ReviewCard";
import type { ReviewQueue } from "@brioche/contracts/ReviewQueue";
import type { ReviewAttemptResult } from "@brioche/contracts/ReviewAttemptResult";
import type { LearningState } from "@brioche/contracts/LearningState";
import type { UserProfile } from "@brioche/contracts/UserProfile";
import type { AccountProfile } from "@brioche/contracts/AccountProfile";
import "../app/styles/app.css";

const stress = new URL(location.href).searchParams.has("stress");
if (new URL(location.href).searchParams.has("hold-animation")) {
  const animate = Element.prototype.animate;
  Element.prototype.animate = function (frames, options) {
    const animation = animate.call(this, frames, options);
    if (this.matches(".home-review .review")) animation.pause();
    return animation;
  };
}
if (stress) {
  lesson.title.fr = "Une conversation autour du mot anticonstitutionnellement";
  for (const block of lesson.blocks) {
    if (block.type === "dialogue")
      block.turns[0].segments[0].text = "anticonstitutionnellement";
    if (block.type === "article")
      block.paragraphs[0].segments[0].text = "anticonstitutionnellement";
  }
}

const qa = {
  demoWrites: [] as {
    body: unknown;
    signal?: AbortSignal | null;
    release: (value: GradeResult | number) => void;
  }[],
  previewWrites: [] as {
    path: string;
    body: unknown;
    signal?: AbortSignal | null;
    release: (value: GradeResult | AdminLessonAudioStatus | number) => void;
  }[],
  ready: false,
  deferAuthBootstrap: false,
  authBootstraps: [] as {
    signal?: AbortSignal | null;
    release: (status: number) => void;
  }[],
  authRequests: [] as {
    path: string;
    body: unknown;
    signal?: AbortSignal | null;
    release: (status: number) => void;
  }[],
  writes: [] as { lessonId: string; idempotencyKey: string }[],
  release: [] as ((status: number) => void)[],
  spoken: [] as string[],
  media: [] as HTMLAudioElement[],
  mediaEvents: [] as { name: string; callback: EventListener }[],
  mediaPlays: [] as { url: string; time: number }[],
  playback: "idle",
  playbackId: null as string | null,
  route: "/",
  search: "",
  changeUser: null as (() => void) | null,
  profileWrites: [] as Record<string, unknown>[],
  profileRelease: [] as ((profile: UserProfile | number) => void)[],
  profileReads: [] as ((profile: UserProfile | number) => void)[],
  accountWrites: [] as Record<string, unknown>[],
  accountRelease: [] as ((account: AccountProfile | number) => void)[],
  accountReads: [] as ((account: AccountProfile | number) => void)[],
  accountState: {
    id: "account-a",
    email: "a@example.test",
    displayName: "Alice",
    role: "operator",
    version: 1,
  } as AccountProfile,
  deferAccountRead: false,
  navigate: null as ((destination: string | number) => void) | null,
  learningWrites: [] as Record<string, unknown>[],
  learningRelease: [] as ((value: LearningState | number) => void)[],
  learningPaths: [] as { path: string; method?: string }[],
  learningReads: [] as ((value: LearningState | number) => void)[],
  sessionLesson: lesson as ReadingLesson,
  reviewPaths: [] as string[],
  reviewWrites: [] as Record<string, unknown>[],
  reviewRelease: [] as ((value: ReviewAttemptResult | number) => void)[],
  queueReads: [] as ((value: ReviewQueue | number) => void)[],
  reviewFixture: null as ReviewQueue | null,
  ownedWrites: [] as { path: string; body: Record<string, unknown> }[],
  ownedRelease: [] as ((value: SavedItem | ReviewCard | number) => void)[],
  savedFixture: null as SavedItem | null,
  confirmExternal: null as ((index: number) => void) | null,
  textAnswers: [] as ExerciseAnswer[],
  hintRequests: 0,
  cardReads: [] as ((value: ReviewCard | number) => void)[],
  catalogReads: [] as { query: string; release: (catalog: Catalog) => void }[],
  catalogFixture: null as Catalog | null,
};
Object.assign(window, { qa });
// A browser speech spy remains available: recording failures must never invoke it.
Object.defineProperty(window, "speechSynthesis", {
  configurable: true,
  value: {
    speak: (utterance: { text: string }) => qa.spoken.push(utterance.text),
    getVoices: () => [{ lang: "fr-FR", name: "must-not-be-used" }],
    cancel() {},
    pause() {},
    resume() {},
    addEventListener() {},
    removeEventListener() {},
  },
});
const NativeAudio = window.Audio;
Object.defineProperty(window, "Audio", {
  configurable: true,
  value: function () {
    const media = new NativeAudio();
    qa.media.push(media);
    const listen = media.addEventListener.bind(media);
    media.addEventListener = ((
      name: string,
      callback: EventListener,
      options?: boolean | AddEventListenerOptions,
    ) => {
      qa.mediaEvents.push({ name, callback });
      listen(name, callback, options);
    }) as typeof media.addEventListener;
    const play = media.play.bind(media);
    media.play = () => {
      qa.mediaPlays.push({ url: media.src, time: media.currentTime });
      return play();
    };
    return media;
  },
});
if (!new URL(location.href).searchParams.has("missing-recording")) {
  for (const [index, block] of lesson.blocks.entries()) {
    if (block.type !== "dialogue" && block.type !== "article") continue;
    const sha = String(index + 1).repeat(64);
    const assetId = "qa-audio-" + block.id;
    lesson.audio!.push({
      assetId,
      revision: 1,
      sha256: sha,
      mimeType: "audio/wav",
      durationMs: 30000,
      creditZh: "Synthetic protocol fixture",
      url: `/api/audio/${sha}.wav`,
    });
    const entries = block.type === "dialogue" ? block.turns : block.paragraphs;
    lesson.audioTracks!.push({
      blockId: block.id,
      assetId,
      cues: entries.flatMap((entry) => [
        {
          entryId: entry.id,
          segmentId: null,
          wordRange: null,
          startMs: 0,
          endMs: 30000,
        },
        ...entry.segments.flatMap((segment) => [
          {
            entryId: entry.id,
            segmentId: segment.id,
            wordRange: null,
            startMs: 0,
            endMs: 30000,
          },
          ...[
            ...new Intl.Segmenter("fr", { granularity: "word" }).segment(
              segment.text,
            ),
          ]
            .filter((token) => token.isWordLike)
            .map((token) => ({
              entryId: entry.id,
              segmentId: segment.id,
              wordRange: {
                start: Array.from(segment.text.slice(0, token.index)).length,
                end:
                  Array.from(segment.text.slice(0, token.index)).length +
                  Array.from(token.segment).length,
              },
              startMs: 0,
              endMs: 30000,
            })),
        ]),
      ]),
    });
  }
}
if (new URL(location.href).searchParams.has("partial-recording")) {
  const block = lesson.blocks.find((block) => block.type === "dialogue");
  if (block?.type === "dialogue")
    block.turns.push({
      ...block.turns[0],
      id: "missing-line",
      segments: [{ ...block.turns[0].segments[0], id: "missing-segment" }],
    });
}
const originalFetch = window.fetch;
function controlledAuth(
  signal: AbortSignal | null | undefined,
  register: (release: (status: number) => void) => void,
) {
  return new Promise<Response>((resolve, reject) => {
    const abort = () =>
      reject(signal?.reason ?? new DOMException("Canceled", "AbortError"));
    if (signal?.aborted) {
      abort();
      return;
    }
    signal?.addEventListener("abort", abort, { once: true });
    register((status) => {
      signal?.removeEventListener("abort", abort);
      resolve(
        status === 200
          ? Response.json({ csrfToken: "controlled" })
          : new Response("", { status }),
      );
    });
  });
}
window.fetch = async (input, init) => {
  const personalInput = String(input).replace(
    /^\/api\/v2\/me\//,
    "/api/v1/me/",
  );
  if (
    String(input).startsWith("/api/demo/lessons/") &&
    init?.method === "POST"
  ) {
    return new Promise<Response>((resolve) =>
      qa.demoWrites.push({
        body: JSON.parse(String(init.body)),
        signal: init.signal,
        release: (value) =>
          resolve(
            typeof value === "number"
              ? new Response("", { status: value })
              : Response.json(value),
          ),
      }),
    );
  }
  if (
    String(input).startsWith("/api/v1/operator/lessons/") &&
    init?.method === "POST"
  ) {
    return new Promise<Response>((resolve) => {
      qa.previewWrites.push({
        path: String(input),
        body: JSON.parse(String(init.body)),
        signal: init.signal,
        release: (value) =>
          resolve(
            typeof value === "number"
              ? new Response("", { status: value })
              : Response.json(value),
          ),
      });
    });
  }
  if (String(input) === "/api/v1/auth/csrf") {
    if (qa.deferAuthBootstrap)
      return controlledAuth(init?.signal, (release) =>
        qa.authBootstraps.push({ signal: init?.signal, release }),
      );
    return Response.json({ csrfToken: "controlled" });
  }
  if (
    [
      "/api/v1/auth/login",
      "/api/v1/auth/accept-invite",
      "/api/v1/auth/reset-password",
      "/api/v1/auth/logout",
    ].includes(String(input))
  )
    return controlledAuth(init?.signal, (release) =>
      qa.authRequests.push({
        path: String(input),
        body: init?.body ? JSON.parse(String(init.body)) : null,
        signal: init?.signal,
        release,
      }),
    );
  if (
    (personalInput.startsWith("/api/v1/me/saved-items/") &&
      init?.method === "PUT") ||
    personalInput === "/api/v1/me/review-enrollments" ||
    personalInput === "/api/v1/me/reviews/qa-card/preferences"
  ) {
    const index =
      qa.ownedWrites.push({
        path: String(input),
        body: JSON.parse(String(init?.body)),
      }) - 1;
    return new Promise<Response>((resolve) => {
      qa.ownedRelease[index] = (value) =>
        resolve(
          typeof value === "number"
            ? new Response("", { status: value })
            : Response.json(value),
        );
    });
  }
  if (
    personalInput === "/api/v1/me/reviews/qa-card" &&
    init?.method === "GET"
  ) {
    return new Promise<Response>((resolve) => {
      qa.cardReads.push((value) =>
        resolve(
          typeof value === "number"
            ? new Response("", { status: value })
            : Response.json(value),
        ),
      );
    });
  }
  if (personalInput === "/api/v1/me/reviews/qa-card/attempts") {
    qa.reviewPaths.push(String(input));
    const index = qa.reviewWrites.push(JSON.parse(String(init?.body))) - 1;
    return new Promise<Response>((resolve) => {
      qa.reviewRelease[index] = (value) =>
        resolve(
          typeof value === "number"
            ? new Response("", { status: value })
            : Response.json(value),
        );
    });
  }
  if (personalInput === "/api/v1/me/reviews") {
    return new Promise<Response>((resolve) => {
      qa.queueReads.push((value) =>
        resolve(
          typeof value === "number"
            ? new Response("", { status: value })
            : Response.json(value),
        ),
      );
    });
  }
  if (
    /^\/api\/v[12]\/learning-sessions\/qa-session$/.test(String(input)) &&
    init?.method === "GET"
  ) {
    return new Promise<Response>((resolve) => {
      qa.learningReads.push((value) =>
        resolve(
          typeof value === "number"
            ? new Response("", { status: value })
            : Response.json({ lesson: qa.sessionLesson, progress: value }),
        ),
      );
    });
  }
  if (/^\/api\/v[12]\/learning-sessions\/qa-session\//.test(String(input))) {
    qa.learningPaths.push({ path: String(input), method: init?.method });
    const index = qa.learningWrites.push(JSON.parse(String(init?.body))) - 1;
    return new Promise<Response>((resolve) => {
      qa.learningRelease[index] = (value) =>
        resolve(
          typeof value === "number"
            ? new Response("", { status: value })
            : Response.json(value),
        );
    });
  }
  if (String(input) === "/api/v1/account") {
    if (init?.method === "PATCH") {
      qa.deferAccountRead = true;
      const index = qa.accountWrites.push(JSON.parse(String(init.body))) - 1;
      return new Promise<Response>((resolve) => {
        qa.accountRelease[index] = (value) => {
          if (typeof value !== "number") {
            qa.accountState = value;
            qa.deferAccountRead = false;
          }
          resolve(
            typeof value === "number"
              ? new Response("", { status: value })
              : Response.json(value),
          );
        };
      });
    }
    if (!qa.deferAccountRead) return Response.json(qa.accountState);
    return new Promise<Response>((resolve) => {
      qa.accountReads.push((value) => {
        qa.deferAccountRead = false;
        if (typeof value !== "number") qa.accountState = value;
        resolve(
          typeof value === "number"
            ? new Response("", { status: value })
            : Response.json(value),
        );
      });
    });
  }
  if (String(input) === "/api/v1/me/settings") {
    const index = qa.profileWrites.push(JSON.parse(String(init?.body))) - 1;
    return new Promise<Response>((resolve) => {
      qa.profileRelease[index] = (profile) =>
        resolve(
          typeof profile === "number"
            ? new Response("", { status: profile })
            : Response.json(profile),
        );
    });
  }
  if (String(input) === "/api/v1/me") {
    return new Promise<Response>((resolve) => {
      qa.profileReads.push((profile) =>
        resolve(
          typeof profile === "number"
            ? new Response("", { status: profile })
            : Response.json(profile),
        ),
      );
    });
  }
  if (String(input) !== "/api/v2/learning-sessions")
    return originalFetch(input, init);
  const body = JSON.parse(String(init?.body)) as {
    lessonId: string;
    idempotencyKey: string;
  };
  const index = qa.writes.push(body) - 1;
  return new Promise<Response>((resolve) => {
    qa.release[index] = (status) =>
      resolve(
        status === 200
          ? Response.json({ progress: { id: "session-" + body.lessonId } })
          : new Response("", { status }),
      );
  });
};
function StartHarness() {
  const [lessonId, setLessonId] = useState("course-a");
  return (
    <main>
      <h1>{lessonId}</h1>
      <StartLearning lessonId={lessonId}>学习 {lessonId}</StartLearning>
      <button
        id="switch-course"
        onClick={() =>
          setLessonId(lessonId === "course-a" ? "course-b" : "course-a")
        }
      >
        切换推荐课程
      </button>
    </main>
  );
}
const neutralReading: NeutralLesson = {
  schemaVersion: nativeSource.schemaVersion,
  targetLanguage: "yue-Hant-HK",
  explanationLanguage: "zh-CN",
  id: nativeSource.id,
  revision: nativeSource.revision,
  levelId: nativeSource.levelId,
  unitId: nativeSource.unitId,
  title: nativeSource.title,
  summaryZh: nativeSource.summaryZh,
  estimatedMinutes: nativeSource.estimatedMinutes,
  objectivesZh: nativeSource.objectivesZh,
  knowledge: nativeSource.knowledge as NeutralLesson["knowledge"],
  blocks: nativeSource.blocks as NeutralLesson["blocks"],
  steps: nativeSource.steps as NeutralLesson["steps"],
  completion: nativeSource.completion,
  reviewItemIds: nativeSource.reviewItemIds,
  cast: [],
  media: [],
  audio: [
    {
      assetId: "qa-native",
      revision: 1,
      sha256: "1".repeat(64),
      mimeType: "audio/wav",
      durationMs: 30000,
      creditZh: "Silent protocol fixture",
      url: "/api/audio/" + "1".repeat(64) + ".wav",
    },
  ],
  audioTracks: [
    {
      blockId: "reading",
      assetId: "qa-native",
      cues: [
        {
          entryId: "paragraph-greeting",
          segmentId: null,
          wordRange: null,
          startMs: 0,
          endMs: 30000,
        },
        {
          entryId: "paragraph-greeting",
          segmentId: "segment-greeting",
          wordRange: { start: 0, end: 2 },
          startMs: 0,
          endMs: 30000,
        },
      ],
    },
  ],
};
function ReadingHarness() {
  const { player } = useLearning();
  qa.playback = player.status;
  qa.playbackId = player.id;
  return (
    <div className="app">
      <main>
        <LessonContent
          lesson={kind === "reading-neutral" ? neutralReading : lesson}
          demo={kind !== "reading-neutral"}
        />
      </main>
    </div>
  );
}
function ChoicesHarness() {
  const [rate, setRate] = useState("1"),
    [zone, setZone] = useState("Asia/Shanghai");
  return (
    <main>
      <h1>个人设置</h1>
      <ChoiceDialog
        title="朗读速度"
        value={rate}
        onChange={setRate}
        choices={["0.75", "1", "1.25", "1.5"].map((value) => ({
          value,
          label: value + "×",
        }))}
      />
      <ChoiceDialog
        title="时区"
        value={zone}
        onChange={setZone}
        searchable
        choices={[
          { value: "Asia/Shanghai", label: "上海" },
          { value: "Europe/Paris", label: "巴黎" },
        ]}
      />
    </main>
  );
}
function ProfileHarness() {
  const [user, setUser] = useState<UserProfile>({
    id: "account-a",
    email: "a@example.test",
    displayName: stress ? "LearnerWithASingleLongNameWithoutSpaces" : "Alice",
    role: "learner",
    version: 1,
    settings: {
      timeZone: "Asia/Shanghai",
      weeklyDays: 5,
      dailyMinutes: 10,
      showTranslation: false,
      speechRate: 1,
    },
  });
  qa.changeUser = () => {
    qa.accountState = {
      id: "account-b",
      email: "b@example.test",
      displayName: "Bob",
      role: "operator",
      version: 3,
    };
    qa.deferAccountRead = false;
    setUser({
      ...user,
      id: "account-b",
      email: "b@example.test",
      displayName: "Bob",
      version: 10,
    });
  };
  return (
    <LearningProvider user={user}>
      <main>
        <Profile />
      </main>
    </LearningProvider>
  );
}
const progress: LearningState = {
  id: "qa-session",
  lessonId: lesson.id,
  revision: 1,
  version: 1,
  lastStepId: "read",
  confirmedStepIds: [],
  hintedExerciseIds: [],
  attempts: [],
  completedAt: null,
  firstCompletedAt: null,
};
function SessionHarness() {
  const session = {
    lesson:
      kind === "session-neutral"
        ? neutralReading
        : kind === "session-multi"
          ? {
              ...lesson,
              steps: [
                ...lesson.steps,
                {
                  id: "recap",
                  kind: "recap",
                  titleZh: "回顾",
                  blockIds: ["evening"],
                },
              ],
              completion: {
                ...lesson.completion,
                requiredStepIds: ["read", "recap"],
              },
            }
          : lesson,
    progress:
      kind === "session-neutral"
        ? {
            ...progress,
            lessonId: neutralReading.id,
            lastStepId: neutralReading.steps[0].id,
          }
        : progress,
  };
  qa.sessionLesson = session.lesson;
  return (
    <LearningProvider user={kind === "session-revoked" ? reviewUser : null}>
      <main>
        <LearningSessionContent initial={session} ownerId="qa-account" />
      </main>
    </LearningProvider>
  );
}
const kind = new URL(location.href).searchParams.get("case");
const reviewQueue: ReviewQueue = {
  items: [
    {
      id: "qa-card",
      knowledgeId: "qa-word",
      sourceLessonId: lesson.id,
      sourceRevision: 1,
      vocabulary: {
        id: "qa-word",
        lemma: "bonjour",
        partOfSpeech: "phrase",
        gender: null,
        meaningZh: "你好",
        noteZh: "日常问候",
      },
      stage: 0,
      dueAt: "2026-10-06T00:00:00Z",
      version: 1,
      suspended: false,
    },
  ],
  dueCount: 1,
  nextDueAt: null,
  localDate: "2026-10-06",
  timeZone: "Asia/Shanghai",
};
qa.reviewFixture = reviewQueue;
if (new URL(location.href).searchParams.has("knowledge-recording")) {
  const recording = { asset: lesson.audio![0], startMs: 1200, endMs: 1800 };
  reviewQueue.items[0].vocabulary.recording = recording;
  lesson.knowledge.vocabulary = [reviewQueue.items[0].vocabulary];
  lesson.knowledge.grammar = [
    {
      id: "qa-grammar",
      titleZh: "日常问候",
      bodyZh: "自动化录音协议样例",
      examples: [{ fr: "Bonjour !", zh: "你好！", recording }],
    },
  ];
  lesson.blocks.push(
    { type: "vocabulary", id: "qa-vocabulary-block", entryIds: ["qa-word"] },
    { type: "grammar", id: "qa-grammar-block", entryIds: ["qa-grammar"] },
  );
  lesson.steps.push({
    id: "qa-explore",
    kind: "explore",
    titleZh: "表达",
    blockIds: ["qa-vocabulary-block", "qa-grammar-block"],
  });
}
if (stress) reviewQueue.items[0].vocabulary.lemma = "anticonstitutionnellement";
const reviewUser: UserProfile = {
  id: "qa-account",
  email: "qa@example.test",
  displayName: "QA",
  role: "learner",
  version: 1,
  settings: {
    timeZone: "Asia/Shanghai",
    weeklyDays: 5,
    dailyMinutes: 10,
    showTranslation: false,
    speechRate: 1,
  },
};
const nativeReviewQueue = {
  ...reviewQueue,
  items: [
    {
      ...reviewQueue.items[0],
      sourceLessonId: neutralReading.id,
      knowledgeId: neutralReading.knowledge.vocabulary[0].id,
      vocabulary: {
        ...neutralReading.knowledge.vocabulary[0],
        recording: { asset: lesson.audio![0], startMs: 1200, endMs: 1800 },
      },
    },
  ],
};
Object.assign(qa, { nativeReviewQueue });
function ReviewsHarness() {
  const currentQueue =
    kind === "reviews-neutral" ? nativeReviewQueue : reviewQueue;
  return (
    <LearningProvider user={reviewUser}>
      <main>
        <Reviews
          loaderData={currentQueue}
          params={{}}
          matches={[
            {
              id: "root",
              params: {},
              pathname: "/",
              loaderData: { user: reviewUser, enabled: true },
              handle: undefined,
            },
            {
              id: "routes/reviews",
              params: {},
              pathname: "/",
              loaderData: currentQueue,
              handle: undefined,
            },
          ]}
        />
      </main>
    </LearningProvider>
  );
}
const summary = {
  id: lesson.id,
  revision: lesson.revision,
  levelId: lesson.levelId,
  unitId: lesson.unitId,
  title: {
    ...lesson.title,
    zh: stress ? "anticonstitutionnellement" : lesson.title.zh,
  },
  summaryZh: lesson.summaryZh,
  estimatedMinutes: lesson.estimatedMinutes,
};
const catalogFixture: Catalog = {
  developmentFixture: false,
  levels: [
    {
      id: lesson.levelId,
      label: "A1 入门",
      units: [{ id: lesson.unitId, titleZh: "日常问候", lessons: [summary] }],
    },
  ],
};
function DemoHarness() {
  const blocks: Extract<Block, { type: "exercise" }>[] = [
    {
      type: "exercise",
      id: "demo-choice",
      exerciseType: "single-choice",
      promptZh: "选择问候",
      options: [
        { id: "bonjour", text: "Bonjour !" },
        { id: "merci", text: "Merci !" },
      ],
    },
    {
      type: "exercise",
      id: "demo-text",
      exerciseType: "fill-blank",
      promptZh: "填入冠词",
      templateFr: "___ baguette",
      hintZh: "阴性名词",
    },
    {
      type: "exercise",
      id: "demo-order",
      exerciseType: "order",
      promptZh: "组成问候",
      tokens: [
        { id: "bonjour", text: "Bonjour" },
        { id: "luc", text: "Luc !" },
      ],
    },
  ];
  const loaderData = {
    lesson: { ...lesson, blocks: [...lesson.blocks, ...blocks] },
  };
  return (
    <LearningProvider>
      <main>
        <Practice
          loaderData={loaderData}
          params={{ lessonId: lesson.id }}
          matches={[
            {
              id: "root",
              params: {},
              pathname: "/",
              loaderData: { user: null, enabled: false },
              handle: undefined,
            },
            {
              id: "routes/practice",
              params: { lessonId: lesson.id },
              pathname: "/",
              loaderData,
              handle: undefined,
            },
          ]}
        />
      </main>
    </LearningProvider>
  );
}
function ScrollHarness() {
  const [long, setLong] = useState(true);
  return (
    <>
      <main
        id="page-content"
        style={{ height: long ? 2400 : 200, padding: 20 }}
      >
        <h1>滚动条检查</h1>
        <button className="scroll-toggle" onClick={() => setLong(!long)}>
          切换内容高度
        </button>
      </main>
      <Scrollbar />
    </>
  );
}
function AuthorHarness() {
  const loaderData = useLoaderData() as {
    lesson: typeof lesson | null;
    id: string;
    revision: string;
    release: PreviewRelease | null;
    releaseId: string;
  };
  const [user, setUser] = useState({
    ...reviewUser,
    role: "operator" as const,
  });
  qa.changeUser = () =>
    setUser({
      ...reviewUser,
      id: "operator-b",
      displayName: "Bob",
      role: "operator",
    });
  return (
    <LearningProvider user={user}>
      <main>
        <AuthorPreview
          loaderData={{ ...loaderData, audioReview: null }}
          params={{}}
          matches={[
            {
              id: "root",
              params: {},
              pathname: "/",
              loaderData: { user, enabled: true },
              handle: undefined,
            },
            {
              id: "routes/author-preview",
              params: {},
              pathname: "/author-preview",
              loaderData: { ...loaderData, audioReview: null },
              handle: undefined,
            },
          ]}
        />
      </main>
    </LearningProvider>
  );
}
qa.catalogFixture = catalogFixture;
function HomeHarness() {
  const resume = {
    sessionId: "qa-home-session",
    lessonId: lesson.id,
    revision: lesson.revision,
    title: summary.title,
    lastStepId: "read",
    completedAt: null,
    firstCompletedAt: null,
    updatedAt: "2026-10-06T00:00:00Z",
  };
  const dashboard: StudyDashboard = {
    localDate: "2026-10-06",
    timeZone: "Asia/Shanghai",
    weekStart: "2026-10-05",
    days: Array.from({ length: 7 }, (_, i) => ({
      localDate: "2026-10-" + String(i + 5).padStart(2, "0"),
      confirmedSteps: i === 1 ? 1 : 0,
      exerciseAttempts: 0,
      reviewAttempts: 0,
      completedLessons: 0,
      active: i === 1,
    })),
    activeDays: 1,
    weeklyGoalDays: 5,
    dailyGoalMinutes: 10,
    dueReviews: 3,
    nextReviewAt: null,
    completedLessons: 0,
    resume,
    recommendedLesson: null,
    allAvailableCompleted: false,
    courseStates: [resume],
    catalog: catalogFixture,
  };
  const homeLesson = {
    ...lesson,
    knowledge: {
      ...lesson.knowledge,
      vocabulary:
        kind === "home-no-expression" ? [] : [reviewQueue.items[0].vocabulary],
    },
    reviewItemIds:
      kind === "home-no-expression" ? [] : [reviewQueue.items[0].knowledgeId],
  };
  const loaderData = {
    catalog: catalogFixture,
    lesson: homeLesson,
    learning: dashboard,
  };
  return (
    <LearningProvider user={reviewUser}>
      <main>
        <Home {...loaderData} />
      </main>
    </LearningProvider>
  );
}
function CoursesHarness() {
  const loaderData = useLoaderData() as { catalog: Catalog; query: string };
  return (
    <main>
      <Courses {...loaderData} />
    </main>
  );
}
const historyItem = {
  id: "qa-history",
  cardId: "qa-card",
  vocabulary: reviewQueue.items[0].vocabulary,
  withdrawn: false,
  rating: "familiar" as const,
  oldStage: 1,
  newStage: 2,
  reviewedAt: "2026-10-05T23:30:00Z",
  dueAt: "2026-10-08T23:30:00Z",
  timeZone: "Asia/Shanghai",
  algorithmVersion: "qa-schedule",
};
function HistoryHarness() {
  const loaderData = useLoaderData() as ReviewHistoryPage;
  return (
    <main>
      <History
        loaderData={loaderData}
        params={{}}
        matches={[
          {
            id: "root",
            params: {},
            pathname: "/",
            loaderData: { user: reviewUser, enabled: true },
            handle: undefined,
          },
          {
            id: "routes/review-history",
            params: {},
            pathname: "/review-history",
            loaderData,
            handle: undefined,
          },
        ]}
      />
    </main>
  );
}
const savedItem: SavedItem = {
  id: "qa-saved",
  knowledgeId: "qa-word",
  sourceLessonId: lesson.id,
  sourceRevision: 1,
  vocabulary: reviewQueue.items[0].vocabulary,
  saved: true,
  withdrawn: false,
  version: 1,
  createdAt: "2026-10-06T00:00:00Z",
};
qa.savedFixture = savedItem;
function LibraryHarness() {
  const loaderData =
    kind === "managed-library"
      ? {
          view: "reviews" as const,
          page: { items: reviewQueue.items, nextCursor: null },
          cursor: null,
        }
      : {
          view: "saved" as const,
          page: {
            items: [
              kind === "library-neutral"
                ? {
                    ...savedItem,
                    vocabulary: nativeReviewQueue.items[0].vocabulary,
                  }
                : savedItem,
            ],
            nextCursor: null,
          },
          cursor: null,
        };
  return (
    <LearningProvider user={reviewUser}>
      <main>
        <Library
          loaderData={loaderData}
          params={{}}
          matches={[
            {
              id: "root",
              params: {},
              pathname: "/",
              loaderData: { user: reviewUser, enabled: true },
              handle: undefined,
            },
            {
              id: "routes/library",
              params: {},
              pathname: "/",
              loaderData,
              handle: undefined,
            },
          ]}
        />
      </main>
    </LearningProvider>
  );
}
const pendingJobs = ["qa-account", "qa-account", "qa-next"].map(
  (userId, index) => {
    const knowledgeId = "pending-word-" + index;
    return {
      key:
        draftScope(userId, "owned", 1, productNamespace) +
        ":" +
        ownedTargetKey({
          kind: "bookmark",
          knowledgeId,
          lessonId: lesson.id,
          revision: 1,
        }),
      job: {
        path: "/api/v1/me/saved-items/" + knowledgeId,
        method: "PUT" as const,
        body: {
          sourceLessonId: lesson.id,
          sourceRevision: 1,
          saved: true,
          version: 0,
          idempotencyKey: "pending-recovery-operation-" + index,
        },
      },
    };
  },
);
if (kind === "pending" || kind === "session-revoked") {
  if (kind === "session-revoked")
    clearSessionDrafts(
      draftScope("qa-account", "qa-session", 1, productNamespace),
    );
  for (const entry of pendingJobs) saveDraft(entry.key, entry.job);
  qa.confirmExternal = (index) =>
    clearPending(
      pendingJobs[index].key,
      pendingJobs[index].job.body.idempotencyKey,
    );
}
function PendingHarness() {
  const [user, setUser] = useState(reviewUser);
  qa.changeUser = () =>
    setUser({ ...reviewUser, id: "qa-next", displayName: "Next" });
  const loaderData = { userId: user.id };
  return (
    <LearningProvider user={user}>
      <main>
        <PendingSaves
          loaderData={loaderData}
          params={{}}
          matches={[
            {
              id: "root",
              params: {},
              pathname: "/",
              loaderData: { user, enabled: true },
              handle: undefined,
            },
            {
              id: "routes/pending-saves",
              params: {},
              pathname: "/",
              loaderData,
              handle: undefined,
            },
          ]}
        />
      </main>
    </LearningProvider>
  );
}
function TextLimitHarness({ hintText = "边界测试" }: { hintText?: string }) {
  const [hinted, setHinted] = useState(false);
  return (
    <LearningProvider>
      <main>
        <h1>填空输入边界</h1>
        <ExerciseEditor
          block={{
            type: "exercise",
            id: "text-limit",
            exerciseType: "fill-blank",
            promptZh: "输入表达",
            templateFr: stress ? "anticonstitutionnellement ___" : "___",
            hintZh: hintText,
          }}
          hinted={hinted}
          blocked={false}
          completed={false}
          hint={() => {
            qa.hintRequests++;
            setHinted(true);
          }}
          submit={async (answer) => {
            qa.textAnswers.push(answer);
          }}
        />
        {stress && (
          <ExerciseEditor
            block={{
              type: "exercise",
              id: "long-choice",
              exerciseType: "single-choice",
              promptZh: "选择表达",
              options: [
                { id: "long", text: "anticonstitutionnellement" },
                { id: "short", text: "bonjour" },
              ],
            }}
            hinted={false}
            blocked={false}
            completed={false}
            hint={() => {}}
            submit={async (answer) => {
              qa.textAnswers.push(answer);
            }}
          />
        )}
        {stress && (
          <ExerciseEditor
            block={{
              type: "exercise",
              id: "long-order",
              exerciseType: "order",
              promptZh: "排列表达",
              tokens: [
                { id: "long", text: "anticonstitutionnellement" },
                { id: "short", text: "bonjour" },
              ],
            }}
            hinted={false}
            blocked={false}
            completed={false}
            hint={() => {}}
            submit={async (answer) => {
              qa.textAnswers.push(answer);
            }}
          />
        )}
      </main>
    </LearningProvider>
  );
}
const reading = kind === "reading" || kind === "reading-neutral";
function AccountHarness() {
  qa.deferAuthBootstrap = kind === "account-abort";
  return (
    <LearningProvider>
      <main>
        <Account
          mode={
            kind === "account-reset"
              ? "reset-password"
              : kind === "account-invite"
                ? "invite"
                : "login"
          }
        />
        <Link className="account-exit" to="/previous">
          离开账号入口
        </Link>
      </main>
    </LearningProvider>
  );
}
const router = createMemoryRouter(
  [
    {
      id: "root",
      path: "/",
      loader: () => ({
        user: null,
        enabled: kind === "profile" || !!kind?.startsWith("account-"),
      }),
      element: kind?.startsWith("account-") ? (
        <AccountHarness />
      ) : kind === "scrollbar" ? (
        <ScrollHarness />
      ) : kind === "demo" ? (
        <DemoHarness />
      ) : reading ? (
        <LearningProvider>
          <ReadingHarness />
        </LearningProvider>
      ) : kind === "choices" ? (
        <ChoicesHarness />
      ) : kind === "profile" ? (
        <ProfileHarness />
      ) : kind === "session" ||
        kind === "session-revoked" ||
        kind === "session-multi" ||
        kind === "session-neutral" ? (
        <SessionHarness />
      ) : kind === "reviews" || kind === "reviews-neutral" ? (
        <ReviewsHarness />
      ) : kind === "library" ||
        kind === "library-neutral" ||
        kind === "managed-library" ? (
        <LibraryHarness />
      ) : kind === "pending" ? (
        <PendingHarness />
      ) : kind === "home" || kind === "home-no-expression" ? (
        <HomeHarness />
      ) : kind === "courses" ? (
        <Navigate to="/courses" replace />
      ) : kind === "audio-publication" ? (
        <main>
          <LessonAudioReview
            id="qa-audio"
            revision={7}
            initial={{
              required: true,
              published: false,
              lessonHash: "a".repeat(64),
              version: 2,
              accepted: false,
              directAuthorized: false,
              reason: "",
              actor: null,
            }}
          />
          <Link className="audio-exit" to="/previous">
            回到后台
          </Link>
        </main>
      ) : kind === "author" ? (
        <Navigate
          to={
            "/author-preview?" +
            new URLSearchParams({
              releaseId: "qa-release-one",
              lessonId: lesson.id,
              revision: "1",
            })
          }
          replace
        />
      ) : kind === "history" || kind === "history-empty" ? (
        <Navigate to="/review-history" replace />
      ) : kind === "text-limit" || kind === "text-no-hint" ? (
        <TextLimitHarness
          hintText={kind === "text-no-hint" ? "\u00a0\u202f" : "边界测试"}
        />
      ) : (
        <StartHarness />
      ),
    },
    { path: "/learning/:id", element: <h1>已进入学习</h1> },
    { path: "/login", element: <h1>登录入口</h1> },
    { path: "/previous", element: <h1>上一页</h1> },
    { path: "/reviews", element: <h1>账号复习入口</h1> },
    {
      path: "/author-preview",
      loader: ({ request }) => {
        const query = new URL(request.url).searchParams;
        const releaseId = query.get("releaseId") ?? "",
          id = query.get("lessonId") ?? "",
          revision = query.get("revision") ?? "";
        const second = {
          ...summary,
          id: "qa-second",
          revision: releaseId === "qa-release-two" ? 3 : 2,
          title: { zh: "另一段日常对话", fr: "Une autre conversation" },
        };
        return {
          releaseId,
          id,
          revision,
          release: releaseId
            ? {
                id: releaseId,
                catalog: {
                  developmentFixture: false,
                  levels: [
                    {
                      id: lesson.levelId,
                      label: "A1",
                      units: [
                        {
                          id: lesson.unitId,
                          titleZh: "日常对话",
                          lessons: [summary, second],
                        },
                      ],
                    },
                  ],
                },
                withdrawnLessonIds: [],
              }
            : null,
          lesson: id
            ? {
                ...lesson,
                id,
                revision: Number(revision),
                title: id === "qa-second" ? second.title : lesson.title,
                blocks: [
                  ...lesson.blocks,
                  {
                    type: "exercise",
                    id: "preview-choice",
                    exerciseType: "single-choice",
                    promptZh: "选择问候",
                    options: [
                      { id: "bonjour", text: "Bonjour !" },
                      { id: "merci", text: "Merci !" },
                    ],
                  },
                  {
                    type: "exercise",
                    id: "preview-text",
                    exerciseType: "fill-blank",
                    promptZh: "填入冠词",
                    templateFr: "___ baguette",
                    hintZh: "阴性名词",
                  },
                  {
                    type: "exercise",
                    id: "preview-order",
                    exerciseType: "order",
                    promptZh: "组成问候",
                    tokens: [
                      { id: "bonjour", text: "Bonjour" },
                      { id: "luc", text: "Luc !" },
                    ],
                  },
                ],
                steps: [
                  ...lesson.steps,
                  {
                    id: "preview-practice",
                    kind: "practice",
                    titleZh: "练习",
                    blockIds: [
                      "preview-choice",
                      "preview-text",
                      "preview-order",
                    ],
                  },
                ],
              }
            : null,
        };
      },
      element: <AuthorHarness />,
    },
    {
      path: "/review-history",
      loader: ({ request }): ReviewHistoryPage => {
        const cursor = new URL(request.url).searchParams.get("cursor");
        if (kind === "history-empty" || cursor === "end")
          return { items: [], nextCursor: null };
        if (cursor)
          return {
            items: [{ ...historyItem, id: "qa-older", rating: "again" }],
            nextCursor: "end",
          };
        return {
          items: [
            historyItem,
            {
              ...historyItem,
              id: "qa-withdrawn",
              vocabulary: null,
              withdrawn: true,
              rating: "remembered",
            },
          ],
          nextCursor: "older/qa?+",
        };
      },
      element: <HistoryHarness />,
    },
    {
      path: "/courses",
      loader: ({ request }) => {
        const query = new URL(request.url).searchParams.get("q") ?? "";
        if (!query) return { catalog: catalogFixture, query };
        return new Promise<{ catalog: Catalog; query: string }>((resolve) => {
          qa.catalogReads.push({
            query,
            release: (catalog) => resolve({ catalog, query }),
          });
        });
      },
      element: <CoursesHarness />,
    },
  ],
  { initialEntries: ["/previous", "/"], initialIndex: 1 },
);
qa.navigate = (destination) => {
  if (typeof destination === "number") void router.navigate(destination);
  else void router.navigate(destination);
};
router.subscribe((state) => {
  qa.route = state.location.pathname;
  qa.search = state.location.search;
});
if (kind === "scrollbar")
  document.documentElement.classList.add("overlay-scroll");
createRoot(document.getElementById("root")!).render(
  <RouterProvider router={router} />,
);
qa.ready = true;

for (const [name, value] of Object.entries(product.theme))
  document.documentElement.style.setProperty(name, value);
