import { before, after, test } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import {
  readFile,
  writeFile,
  mkdtemp,
  unlink,
  rmdir,
  mkdir,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, sep, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import { createRequestHandler } from "react-router";
import { productWebUrl, browserCliUrl } from "../test-product.mjs";
const build = await import(productWebUrl("build/server/index.js"));

const execute = promisify(execFile);
const cli = fileURLToPath(
  browserCliUrl(),
);
const client = fileURLToPath(productWebUrl("build/client/"));
const session = "brioche-ssr-layout-" + randomUUID();
const uploadDirectory = await mkdtemp(
  resolve(tmpdir(), "brioche-admin-upload-"),
);
const lessonUpload = resolve(uploadDirectory, "lesson.json");
const releaseUpload = resolve(uploadDirectory, "release.json");
const source = JSON.parse(
  await readFile(
    new URL("../../../docs/examples/a1-bakery.lesson.json", import.meta.url),
    "utf8",
  ),
);
// Public projection only: the browser never receives grading rules or editorial data.
const publicFields = [
  "schemaVersion",
  "id",
  "revision",
  "levelId",
  "unitId",
  "title",
  "summaryZh",
  "estimatedMinutes",
  "objectivesZh",
  "knowledge",
  "blocks",
  "steps",
  "completion",
  "reviewItemIds",
  "cast",
];
const lesson = {
  ...Object.fromEntries(publicFields.map((key) => [key, source[key]])),
  media: [],
  audio: [],
  audioTracks: [],
};
const catalog = {
  developmentFixture: true,
  levels: [
    {
      id: "a1",
      label: "A1",
      units: [
        { id: lesson.unitId, titleZh: "早餐与面包店", lessons: [lesson] },
      ],
    },
  ],
};
const handler = createRequestHandler(build, "production");
const originalBase = process.env.INTERNAL_API_URL;
const serverErrors = [];
let origin,
  opened = false;
let accounts = false;
let operatorAccount = false;
let managedRole = "learner";
let managedSessionRevoked = false;
let pendingTokenRevoked = false;
let referenceGrant = null;
let voiceJob = null;
const voiceSeed = JSON.parse(
  await readFile(
    new URL("../../../docs/characters/voices.json", import.meta.url),
    "utf8",
  ),
);
let characterVoice = { ...voiceSeed.items[0], voiceRevision: 0, profile: null };
const characterAvatar = await readFile(
  new URL("../../../test-fixtures/visuals/avatars/camille.svg", import.meta.url),
);
let finalListening = false;
let finalLostReply = false;
let finalDecision = null;
let finalStatus = {
  required: true,
  published: false,
  lessonHash: "a".repeat(64),
  version: 0,
  accepted: false,
  reason: "",
  actor: null,
};
let adminApproved = false;
let adminAudio = { required: false, accepted: false };
let adminPaged = false;
let overviewReads = [];
let adminWrites = [];
let voiceAuditions = [],
  auditionSynthCalls = 0,
  auditionLostReply = false;
let lessonStatus = 200;
let speechClips = [],
  clipLostReply = false;
let alignmentResult = null,
  alignmentLostImport = false,
  alignmentLostReview = false;
let packageReceipts = [],
  packageLostReply = false;
let speechPlans = [],
  speechLostReply = false;
const speechVoices = lesson.cast.map((character) => ({
  ...voiceSeed.items[0],
  character,
  voiceRevision: 1,
}));
const identityReads = [];
let identityProof = null;
const profile = (id) => ({
  id,
  email: `${id}@example.test`,
  displayName: id === "shell-a" ? "Alice" : "Bob",
  role: operatorAccount ? "operator" : "learner",
  version: 1,
  settings: {
    timeZone: "Asia/Shanghai",
    weeklyDays: 3,
    dailyMinutes: 10,
    showTranslation: false,
    speechRate: 1,
  },
});
const api = createServer((request, response) => {
  response.setHeader("Content-Type", "application/json");
  if (request.url.match(/speech-plans\/[a-f0-9]{32}\/alignments/)) {
    response.end(JSON.stringify({ items: [], next: null }));
    return;
  }
  if (request.url.startsWith("/api/v1/operator/speech-alignments")) {
    if (request.url.includes("/packages")) {
      response.end(JSON.stringify({ items: packageReceipts, next: null }));
      return;
    }
    if (request.method === "POST") {
      let body = "";
      request.on("data", (c) => (body += c));
      request.on("end", () => {
        const payload = JSON.parse(body);
        adminWrites.push({ operation: request.url, ...payload });
        if (request.url.endsWith("/package/import")) {
          const result = {
            id: payload.id,
            lessonId: lesson.id,
            revision: payload.package.lessonRevision,
            recordingCount: 3,
          };
          if (!packageReceipts.some((item) => item.id === payload.id))
            packageReceipts.push(result);
          if (packageLostReply) {
            packageLostReply = false;
            response.statusCode = 503;
            response.end("{}");
            return;
          }
          response.end(JSON.stringify(result));
          return;
        } else if (request.url.endsWith("/package")) {
          response.setHeader("Content-Type", "application/x-tar");
          response.end(Buffer.alloc(1024));
          return;
        } else if (request.url.endsWith("/review")) {
          alignmentResult.clips[0].accepted = payload.accepted;
          alignmentResult.clips[0].words = payload.words;
          if (alignmentLostReview) {
            alignmentLostReview = false;
            response.statusCode = 503;
            response.end("{}");
            return;
          }
        } else {
          alignmentResult.id = payload.id;
          if (alignmentLostImport) {
            alignmentLostImport = false;
            response.statusCode = 503;
            response.end("{}");
            return;
          }
        }
        response.end(JSON.stringify(alignmentResult));
      });
      return;
    }
    response.end(JSON.stringify(alignmentResult));
    return;
  }
  if (request.url.match(/speech-plans\/[a-f0-9]{32}\/clips$/)) {
    response.end(JSON.stringify({ items: speechClips, configured: true }));
    return;
  }
  if (request.url.startsWith("/api/v1/operator/speech-clips")) {
    if (request.url.endsWith("/file")) {
      const bytes = Buffer.alloc(44 + 24000 * 4 * 2);
      bytes.write("RIFF");
      bytes.writeUInt32LE(bytes.length - 8, 4);
      bytes.write("WAVEfmt ", 8);
      bytes.writeUInt32LE(16, 16);
      bytes.writeUInt16LE(1, 20);
      bytes.writeUInt16LE(1, 22);
      bytes.writeUInt32LE(24000, 24);
      bytes.writeUInt32LE(48000, 28);
      bytes.writeUInt16LE(2, 32);
      bytes.writeUInt16LE(16, 34);
      bytes.write("data", 36);
      bytes.writeUInt32LE(bytes.length - 44, 40);
      response.setHeader("Content-Type", "audio/wav");
      response.end(bytes);
      return;
    }
    if (request.method === "POST") {
      let body = "";
      request.on("data", (chunk) => (body += chunk));
      request.on("end", () => {
        const payload = JSON.parse(body);
        adminWrites.push({ operation: request.url, ...payload });
        let clip = speechClips.find((c) => c.id === payload.id);
        if (!clip) {
          clip = {
            id: payload.id,
            planId: payload.planId,
            generationKey: payload.generationKey,
            reusedFrom: null,
            status: "ready",
            durationMs: 100,
            requestId: "controlled-request",
            accepted: null,
            createdAt: "2026-10-07T00:00:00.000000Z",
          };
          speechClips.push(clip);
        }
        if (clipLostReply) {
          clipLostReply = false;
          response.writeHead(503).end("{}");
          return;
        }
        response.end(JSON.stringify(clip));
      });
      return;
    }
    const id = request.url.match(/speech-clips\/([a-f0-9]{32})$/)?.[1];
    response.end(JSON.stringify(speechClips.find((c) => c.id === id)));
    return;
  }
  if (request.url.endsWith("/speech-options")) {
    response.end(JSON.stringify({ lesson, voices: speechVoices }));
    return;
  }
  if (request.url.startsWith("/api/v1/operator/speech-plans")) {
    if (request.method === "POST") {
      let body = "";
      request.on("data", (chunk) => (body += chunk));
      request.on("end", () => {
        const payload = JSON.parse(body);
        adminWrites.push({ operation: request.url, ...payload });
        const input = payload.preview ?? payload;
        const plan = {
          id: null,
          lessonId: lesson.id,
          lessonRevision: 1,
          sourceHash: "b".repeat(64),
          planHash: "a".repeat(64),
          requestCount: 2,
          totalRequestCharacters: 20,
          selection: input.selection,
          voices: speechVoices,
          createdAt: null,
          targets: [
            {
              pointer: "/blocks/1/turns/0",
              entryId: "qa-0",
              text: "Bonjour !",
              voice: input.selection.voices[0],
              emotion: input.selection.emotions["/blocks/1/turns/0"] ?? "Calm",
              generationKey: "c".repeat(64),
              wordCount: 1,
            },
            {
              pointer: "/knowledge/vocabulary/0/lemma",
              entryId: "qa-1",
              text: "une baguette",
              voice: input.selection.knowledgeNarrator,
              emotion: "Calm",
              generationKey: "d".repeat(64),
              wordCount: 0,
            },
          ],
        };
        if (request.url.endsWith("/preview")) {
          response.end(JSON.stringify(plan));
          return;
        }
        let saved = speechPlans.find((p) => p.id === payload.id);
        if (!saved) {
          saved = {
            ...plan,
            id: payload.id,
            createdAt: "2026-10-07T01:02:03.123456Z",
          };
          speechPlans.push(saved);
        }
        if (speechLostReply) {
          speechLostReply = false;
          response.writeHead(503).end("{}");
          return;
        }
        response.end(JSON.stringify(saved));
      });
      return;
    }
    const id = request.url.match(/speech-plans\/([a-f0-9]{32})$/)?.[1];
    response.end(
      JSON.stringify(
        id
          ? speechPlans.find((p) => p.id === id)
          : { items: speechPlans, next: null },
      ),
    );
    return;
  }
  if (request.url.startsWith("/api/v1/operator/recordings")) {
    if (request.method === "POST") {
      const chunks = [];
      request.on("data", (chunk) => chunks.push(chunk));
      request.on("end", async () => {
        try {
          const payload = await new Request("http://test", {
            method: "POST",
            headers: { "content-type": request.headers["content-type"] },
            body: Buffer.concat(chunks),
          }).formData();
          const document = JSON.parse(payload.get("document")),
            file = payload.get("file");
          adminWrites.push({
            operation: request.url,
            ...document,
            fileBytes: file.size,
            csrf: request.headers["x-csrf-token"],
          });
          response.end(
            JSON.stringify({
              assetId: document.assetId,
              revision: document.revision,
            }),
          );
        } catch (e) {
          serverErrors.push(String(e));
          response.writeHead(400).end("{}");
        }
      });
      return;
    }
    if (!accounts || !operatorAccount) {
      response.statusCode = 401;
      response.end("{}");
      return;
    }
    if (request.url.endsWith("/file")) {
      const bytes = Buffer.alloc(44 + 8000 * 4 * 2);
      bytes.write("RIFF");
      bytes.writeUInt32LE(bytes.length - 8, 4);
      bytes.write("WAVEfmt ", 8);
      bytes.writeUInt32LE(16, 16);
      bytes.writeUInt16LE(1, 20);
      bytes.writeUInt16LE(1, 22);
      bytes.writeUInt32LE(8000, 24);
      bytes.writeUInt32LE(16000, 28);
      bytes.writeUInt16LE(2, 32);
      bytes.writeUInt16LE(16, 34);
      bytes.write("data", 36);
      bytes.writeUInt32LE(bytes.length - 44, 40);
      for (let i = 0; i < 32000; i++)
        bytes.writeInt16LE(
          Math.round(Math.sin((i * Math.PI * 2 * 220) / 8000) * 1200),
          44 + i * 2,
        );
      response.setHeader("Content-Type", "audio/wav");
      response.end(bytes);
      return;
    }
    const query = new URL(request.url, "http://fixture").searchParams.get("q");
    if (query === "reference-fixture") {
      response.end(
        JSON.stringify({
          items: [1, 2, 3].map((revision) => ({
            asset: {
              assetId: "qa-reference",
              revision,
              sha256: "a".repeat(64),
              mimeType: "audio/wav",
              durationMs: revision === 3 ? 31000 : 4000,
              creditZh: "仅测试",
              url: "/api/v1/operator/recordings/qa-reference/1/file",
            },
            source: "test:synthetic",
            license: "LicenseRef-TestOnly",
            creator: "protocol fixture",
            rightsConfirmed: true,
            byteSize: 64044,
            sampleRate: 8000,
            channels: 1,
          })),
          next: null,
        }),
      );
      return;
    }
    response.end(
      JSON.stringify({
        items: query
          ? []
          : [
              {
                asset: {
                  assetId: "qa-recording-" + "a".repeat(88),
                  revision: 1,
                  sha256: "a".repeat(64),
                  mimeType: "audio/wav",
                  durationMs: 4000,
                  creditZh: "隔离合成测试",
                  url: "/api/v1/operator/recordings/qa-recording/1/file",
                },
                source: "test:synthetic/" + "long-source-".repeat(20),
                license: "LicenseRef-TestOnly",
                creator: "protocol fixture",
                rightsConfirmed: true,
                byteSize: 64044,
                sampleRate: 8000,
                channels: 1,
              },
            ],
        next: null,
      }),
    );
    return;
  }
  if (request.url.startsWith("/api/v1/operator/assets")) {
    if (request.method === "POST") {
      const chunks = [];
      request.on("data", (chunk) => chunks.push(chunk));
      request.on("end", async () => {
        try {
          const payload = await new Request("http://test", {
            method: "POST",
            headers: { "content-type": request.headers["content-type"] },
            body: Buffer.concat(chunks),
          }).formData();
          const document = JSON.parse(payload.get("document"));
          const file = payload.get("file");
          adminWrites.push({
            operation: request.url,
            ...document,
            fileBytes: file.size,
            csrf: request.headers["x-csrf-token"],
          });
          response.end(
            JSON.stringify({
              assetId: document.assetId,
              revision: document.revision,
            }),
          );
        } catch (e) {
          serverErrors.push(String(e));
          response.writeHead(400).end("{}");
        }
      });
      return;
    }
    if (request.url.endsWith("/file")) {
      response.setHeader("content-type", "image/svg+xml");
      response.end(characterAvatar);
    } else {
      const query = new URL(request.url, "http://test").searchParams;
      response.end(
        JSON.stringify({
          items:
            query.get("q") === "missing"
              ? []
              : [
                  {
                    asset: {
                      assetId: "avatar-camille-v1",
                      revision: 1,
                      sha256: "a".repeat(64),
                      mimeType: "image/svg+xml",
                      width: 96,
                      height: 96,
                      altZh: "Camille 头像",
                      creditZh: "隔离测试",
                      url: "/api/v1/operator/assets/avatar-camille-v1/1/file",
                    },
                    source: "test:original",
                    license: "LicenseRef-TestOnly",
                    creator: "test fixture",
                    rightsConfirmed: true,
                    byteSize: 1234,
                  },
                ],
          next: null,
        }),
      );
    }
    return;
  }
  if (request.url.startsWith("/api/v1/operator/voice-auditions")) {
    if (!accounts || !operatorAccount) {
      response.writeHead(401).end("{}");
      return;
    }
    const url = new URL(request.url, "http://fixture"),
      parts = url.pathname.split("/");
    if (request.method === "GET") {
      if (parts.at(-1) === "file") {
        const bytes = Buffer.alloc(44 + 24000 * 4 * 2);
        bytes.write("RIFF");
        bytes.writeUInt32LE(bytes.length - 8, 4);
        bytes.write("WAVEfmt ", 8);
        bytes.writeUInt32LE(16, 16);
        bytes.writeUInt16LE(1, 20);
        bytes.writeUInt16LE(1, 22);
        bytes.writeUInt32LE(24000, 24);
        bytes.writeUInt32LE(48000, 28);
        bytes.writeUInt16LE(2, 32);
        bytes.writeUInt16LE(16, 34);
        bytes.write("data", 36);
        bytes.writeUInt32LE(bytes.length - 44, 40);
        response.setHeader("Content-Type", "audio/wav");
        response.end(bytes);
        return;
      }
      response.end(
        JSON.stringify(
          parts.length === 6
            ? voiceAuditions.find((a) => a.id === parts.at(-1))
            : { items: voiceAuditions, next: null, configured: true },
        ),
      );
      return;
    }
    let body = "";
    request.on("data", (chunk) => (body += chunk));
    request.on("end", () => {
      const change = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...change });
      if (parts.at(-1) === "review") {
        const audition = voiceAuditions.find((a) => a.id === parts.at(-2));
        setTimeout(() => {
          audition.accepted = change.accepted;
          audition.appliedVoiceRevision = change.accepted
            ? audition.baseVoiceRevision + 1
            : null;
          if (change.accepted && !audition.cloneJobId)
            characterVoice = {
              ...characterVoice,
              voiceRevision: audition.appliedVoiceRevision,
              profile: structuredClone(audition.profile),
            };
          response.end(JSON.stringify(audition));
        }, 1200);
        return;
      }
      let audition = voiceAuditions.find((a) => a.id === change.id);
      if (!audition) {
        audition = {
          id: change.id,
          cloneJobId: change.cloneJobId,
          characterId: change.candidate?.characterId ?? "character-camille",
          characterRevision: change.candidate?.characterRevision ?? 1,
          baseVoiceRevision: change.candidate?.expectedVoiceRevision ?? 1,
          profile: structuredClone(
            change.candidate?.profile ?? {
              ...voiceSeed.items[0].profile,
              voiceId: voiceJob.voiceId,
              voiceKind: "cloned",
            },
          ),
          voiceId: change.candidate?.profile.voiceId ?? voiceJob.voiceId,
          text: change.text,
          emotion: change.emotion,
          status: "submitted",
          durationMs: null,
          requestId: null,
          accepted: null,
          appliedVoiceRevision: null,
          createdAt: "2026-10-07T00:00:00Z",
        };
        voiceAuditions.push(audition);
        auditionSynthCalls++;
        const created = audition;
        setTimeout(() => {
          created.status = "ready";
          created.durationMs = 4000;
          created.requestId = "synthetic-audition";
        }, 2500);
      }
      if (auditionLostReply) {
        auditionLostReply = false;
        setTimeout(() => response.writeHead(503).end("{}"), 1200);
      } else response.end(JSON.stringify(audition));
    });
    return;
  }
  if (request.url === "/api/v1/auth/csrf") {
    response.end(JSON.stringify({ csrfToken: "controlled-admin-csrf" }));
    return;
  }
  if (
    request.url === "/api/v1/operator/characters/revisions" &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const change = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...change });
      characterVoice = {
        character: {
          characterId: change.characterId,
          revision: change.expectedRevision + 1,
          displayName: change.displayName,
          avatarId: change.avatarId,
          speechLocale: "fr-FR",
        },
        avatarRevision: change.avatarRevision,
        voiceRevision: 0,
        profile: null,
      };
      response.end(JSON.stringify(characterVoice));
    });
    return;
  }
  if (
    request.url === "/api/v1/operator/voice-jobs" &&
    request.method === "GET"
  ) {
    response.end(
      JSON.stringify({
        items: voiceJob ? [voiceJob] : [],
        next: null,
        configured: true,
      }),
    );
    return;
  }
  if (
    request.url === `/api/v1/operator/voice-jobs/${"e".repeat(32)}` &&
    request.method === "GET"
  ) {
    response.end(JSON.stringify(voiceJob));
    return;
  }
  if (
    request.url === "/api/v1/operator/voice-jobs" &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => (body += chunk));
    request.on("end", () => {
      const change = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...change });
      voiceJob = {
        id: "e".repeat(32),
        grantId: change.grantId,
        characterId: "character-camille",
        characterRevision: 1,
        voiceRevision: 1,
        model: "qwen-audio-3.1-tts-flash",
        prefix: "b123456789",
        version: 1,
        status: "submitted",
        voiceId: null,
        requestId: null,
        createdAt: "2026-10-07T00:00:00Z",
        updatedAt: "2026-10-07T00:00:01Z",
      };
      response.end(JSON.stringify(voiceJob));
      setTimeout(() => {
        voiceJob = {
          ...voiceJob,
          version: 2,
          status: "processing",
          voiceId: "qwen-audio-3.1-tts-flash-b123456789-" + "x".repeat(100),
          requestId: "synthetic-create",
        };
      }, 2500);
    });
    return;
  }
  if (
    request.url === `/api/v1/operator/voice-jobs/${"e".repeat(32)}/check` &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => (body += chunk));
    request.on("end", () => {
      const change = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...change });
      voiceJob = {
        ...voiceJob,
        version: voiceJob.version + 1,
        status: "checking",
        voiceId: voiceJob.voiceId ?? change.voiceId,
        requestId: "synthetic-query",
      };
      response.end(JSON.stringify(voiceJob));
      setTimeout(() => {
        voiceJob = {
          ...voiceJob,
          version: voiceJob.version + 1,
          status: "ready",
        };
      }, 2500);
    });
    return;
  }
  if (
    request.url === "/api/v1/operator/voice-references" &&
    request.method === "GET"
  ) {
    response.end(
      JSON.stringify({
        items: referenceGrant ? [referenceGrant] : [],
        next: null,
      }),
    );
    return;
  }
  if (
    request.url === "/api/v1/operator/voice-references" &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => (body += chunk));
    request.on("end", () => {
      const change = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...change });
      referenceGrant = {
        id: "c".repeat(32),
        characterId: change.characterId,
        characterRevision: change.characterRevision,
        voiceRevision: change.voiceRevision,
        assetId: "qa-reference",
        assetRevision: 2,
        model: "qwen-audio-3.1-tts-flash",
        createdAt: "2026-10-07T00:00:00Z",
        expiresAt: "2026-10-07T00:15:00Z",
        revoked: false,
        readCount: 0,
      };
      response.end(
        JSON.stringify({
          grant: referenceGrant,
          path: `/api/v1/voice-references/${referenceGrant.id}/${"d".repeat(64)}`,
        }),
      );
    });
    return;
  }
  if (
    request.url ===
      `/api/v1/operator/voice-references/${"c".repeat(32)}/revoke` &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => (body += chunk));
    request.on("end", () => {
      adminWrites.push({ operation: request.url, ...JSON.parse(body) });
      referenceGrant.revoked = true;
      response.end(JSON.stringify(referenceGrant));
    });
    return;
  }
  if (
    /^\/api\/v1\/operator\/characters\/character-camille\/1(?:\/voices\/1)?$/.test(
      request.url,
    )
  ) {
    response.end(JSON.stringify(characterVoice));
    return;
  }
  if (
    request.url === "/api/v1/operator/characters" &&
    request.method === "GET"
  ) {
    response.end(JSON.stringify({ items: [characterVoice], nextId: null }));
    return;
  }
  if (
    request.url === "/api/v1/operator/characters" &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const change = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...change });
      characterVoice = {
        ...characterVoice,
        voiceRevision: characterVoice.voiceRevision + 1,
        profile: change.profile,
      };
      response.end(JSON.stringify(characterVoice));
    });
    return;
  }
  if (
    /^\/api\/v1\/operator\/characters\/character-camille\/1\/avatar$/.test(
      request.url,
    )
  ) {
    response.setHeader("content-type", "image/svg+xml");
    response.end(characterAvatar);
    return;
  }
  if (
    request.url.startsWith("/api/v1/operator/accounts/pending-tokens") &&
    request.method === "GET"
  ) {
    response.end(
      JSON.stringify({
        items: pendingTokenRevoked
          ? []
          : [
              {
                id: "b".repeat(64),
                email: "pending@example.test",
                kind: "invite",
                role: "learner",
                expiresAt: "2027-01-01T00:00:00Z",
              },
            ],
        nextId: null,
      }),
    );
    return;
  }
  if (
    request.url ===
      `/api/v1/operator/accounts/pending-tokens/${"b".repeat(64)}/revoke` &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      adminWrites.push({ operation: request.url, ...JSON.parse(body) });
      pendingTokenRevoked = true;
      response.end("true");
    });
    return;
  }
  if (
    finalListening &&
    request.url ===
      `/api/v1/operator/lessons/${lesson.id}/revisions/1/audio-review`
  ) {
    if (request.method === "GET") {
      response.end(JSON.stringify(finalStatus));
      return;
    }
    let body = "";
    request.on("data", (chunk) => (body += chunk));
    request.on("end", () => {
      const decision = JSON.parse(body);
      adminWrites.push(decision);
      if (!finalDecision) {
        finalDecision = decision;
        finalStatus = {
          ...finalStatus,
          version: 1,
          accepted: decision.accepted,
          reason: decision.reason,
          actor: "user:101",
        };
      }
      if (finalLostReply) {
        finalLostReply = false;
        response.writeHead(503).end("{}");
        return;
      }
      response.end(JSON.stringify(finalStatus));
    });
    return;
  }
  if (
    finalListening &&
    request.url === `/api/v1/operator/lessons/${lesson.id}/revisions/1`
  ) {
    response.end(
      JSON.stringify({
        ...lesson,
        audio: [
          {
            assetId: "audio-protocol",
            revision: 1,
            sha256: "a".repeat(64),
            mimeType: "audio/mpeg",
            durationMs: 1000,
            creditZh: "合成协议fixture",
            url: "/api/v1/operator/recordings/audio-protocol/1/file",
          },
        ],
      }),
    );
    return;
  }
  if (request.url.startsWith("/api/v1/operator/overview")) {
    const query = new URL(request.url, "http://test").searchParams;
    overviewReads.push(request.url);
    if (adminPaged) {
      const id = "pagination-course";
      const q = query.get("lessonQ") ?? "";
      const page = query.has("lessonAfterId");
      const match = !q || q === "Pagination";
      response.end(
        JSON.stringify({
          generation: "0",
          activeRelease: null,
          lessons: match
            ? Array.from({ length: page ? 5 : 20 }, (_, n) => ({
                id,
                revision: (page ? 5 : 25) - n,
                title: "Pagination synthetic course",
                level: "a1",
                unit: lesson.unitId,
                published: false,
                withdrawn: false,
                approved: false,
                contentApproved: false,
                audioRequired: false,
                audioAccepted: false,
                reviewVersion: 0,
                reviewNote: "隔离分页测试",
              }))
            : [],
          releases:
            query.get("releaseQ") === "no-match"
              ? []
              : [{ id: "pagination-release", lessonCount: 25 }],
          lessonNext: match && !page ? { id, revision: 6 } : null,
          releaseNext: null,
        }),
      );
      return;
    }
    response.end(
      JSON.stringify({
        generation: "0",
        activeRelease: null,
        lessonNext: null,
        releaseNext: null,
        releases: [],
        lessons: [
          {
            id: lesson.id,
            revision: 1,
            title: "在面包店买早餐",
            level: "a1",
            unit: lesson.unitId,
            published: false,
            withdrawn: false,
            approved:
              adminApproved && (!adminAudio.required || adminAudio.accepted),
            contentApproved: adminApproved,
            audioRequired: adminAudio.required,
            audioAccepted: adminAudio.accepted,
            reviewVersion: adminApproved ? 1 : 0,
            reviewNote: "隔离管理员界面测试",
          },
        ],
      }),
    );
    return;
  }
  if (
    request.url === "/api/v1/operator/accounts/101/sessions" &&
    request.method === "GET"
  ) {
    response.end(
      JSON.stringify({
        account: {
          id: "101",
          email: "learner@example.test",
          displayName: "测试账号",
          role: managedRole,
        },
        items: managedSessionRevoked
          ? []
          : [
              {
                id: "a".repeat(64),
                expiresAt: "2027-01-01T00:00:00Z",
                current: false,
              },
            ],
        nextId: null,
      }),
    );
    return;
  }
  if (
    request.url ===
      "/api/v1/operator/accounts/101/sessions/" + "a".repeat(64) + "/revoke" &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      adminWrites.push({ operation: request.url, ...JSON.parse(body) });
      managedSessionRevoked = true;
      response.end(JSON.stringify({ current: false }));
    });
    return;
  }
  if (
    request.url.startsWith("/api/v1/operator/accounts") &&
    request.method === "GET"
  ) {
    const older = new URL(
      request.url,
      "http://controlled.test",
    ).searchParams.has("afterId");
    response.end(
      JSON.stringify({
        items: [
          {
            id: older ? "102" : "101",
            email: older ? "older@example.test" : "learner@example.test",
            displayName: older ? "较早账号" : "测试账号",
            role: older ? "learner" : managedRole,
          },
        ],
        nextId: older ? null : "101",
      }),
    );
    return;
  }
  if (
    request.url === "/api/v1/operator/accounts/101/role" &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const change = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...change });
      managedRole = change.role;
      response.end(
        JSON.stringify({
          id: "101",
          email: "learner@example.test",
          displayName: "测试账号",
          role: managedRole,
        }),
      );
    });
    return;
  }
  if (
    request.url === "/api/v1/operator/accounts/token" &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const issuance = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...issuance });
      setTimeout(
        () =>
          response.end(
            JSON.stringify({
              token: "f".repeat(64),
              email: issuance.email,
              kind: issuance.kind,
              expiresInSeconds: issuance.kind === "invite" ? 172800 : 1800,
            }),
          ),
        300,
      );
    });
    return;
  }
  if (request.url.startsWith("/api/v1/operator/history")) {
    const older = new URL(
      request.url,
      "http://controlled.test",
    ).searchParams.has("beforeKey");
    response.end(
      JSON.stringify({
        items: [
          {
            key: older ? "content:1" : "review:lesson:1:1",
            action: older ? "stage" : "approve",
            target: "在面包店买早餐 v1",
            actor: older ? "local-author-cli" : "user:101",
            reason: older ? "较早的目录检查" : "界面协议测试批准",
            createdAt: "2026-10-07T01:02:03.123456Z",
          },
        ],
        next: older
          ? null
          : {
              beforeTime: "2026-10-07T01:02:03.123456Z",
              beforeKey: "review:lesson:1:1",
            },
      }),
    );
    return;
  }
  if (request.url.endsWith("/review") && request.method === "POST") {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const decision = JSON.parse(body);
      adminWrites.push(decision);
      adminApproved = decision.approved;
      response.end(JSON.stringify({ ...decision, version: 1 }));
    });
    return;
  }
  if (
    /^\/api\/v1\/operator\/documents\/(lesson|release)\/check$/.test(
      request.url,
    ) &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const uploaded = JSON.parse(body);
      response.end(
        JSON.stringify(
          uploaded.document.includes("preflight-invalid-marker")
            ? {
                valid: false,
                issue: {
                  pointer: "/title/fr",
                  line: 4,
                  column: 12,
                  messageZh: "登记素材版本不存在。",
                },
              }
            : { valid: true, issue: null },
        ),
      );
    });
    return;
  }
  if (
    [
      "/api/v1/operator/lessons/import",
      "/api/v1/operator/releases/stage",
    ].includes(request.url) &&
    request.method === "POST"
  ) {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const upload = JSON.parse(body);
      adminWrites.push({ operation: request.url, ...upload });
      response.end(
        JSON.stringify(
          request.url.endsWith("/import")
            ? { lessonId: lesson.id, revision: 1 }
            : "browser-release",
        ),
      );
    });
    return;
  }
  if (request.url === "/api/v1/me") {
    if (accounts) {
      const id = /(?:^|;\s*)brioche\.sid=(shell-[ab])(?:;|$)/.exec(
        request.headers.cookie ?? "",
      )?.[1];
      identityReads.push({
        id: id ?? null,
        channel: request.headers["x-shell-channel"] ?? "ssr",
      });
      if (id) response.end(JSON.stringify(profile(id)));
      else response.writeHead(401).end("{}");
      return;
    }
    response.writeHead(404).end("{}");
    return;
  }
  if (request.url.startsWith("/api/catalog")) {
    response.end(JSON.stringify({ ...catalog, developmentFixture: !accounts }));
    return;
  }
  if (request.url.startsWith("/api/lessons/")) {
    response.statusCode = lessonStatus;
    response.end(JSON.stringify(lessonStatus === 200 ? lesson : {}));
    return;
  }
  response.writeHead(404).end("{}");
});
const types = {
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".webmanifest": "application/manifest+json",
};
const web = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, origin);
    if (url.pathname === "/__identity-proof" && request.method === "POST") {
      let body = "";
      for await (const chunk of request) body += chunk;
      identityProof = JSON.parse(body);
      response.writeHead(204).end();
      return;
    }
    if (url.pathname.startsWith("/api/")) {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const body = Buffer.concat(chunks);
      const proxied = await fetch(
        process.env.INTERNAL_API_URL + url.pathname + url.search,
        {
          method: request.method,
          ...(body.length ? { body } : {}),
          headers: {
            "X-Shell-Channel": "browser",
            ...(request.headers["x-csrf-token"]
              ? { "x-csrf-token": request.headers["x-csrf-token"] }
              : {}),
            ...(request.headers["content-type"]
              ? { "Content-Type": request.headers["content-type"] }
              : {}),
            ...(request.headers.cookie
              ? { cookie: request.headers.cookie }
              : {}),
          },
        },
      );
      response.writeHead(proxied.status, {
        "Content-Type":
          proxied.headers.get("content-type") ?? "application/json",
        "Cache-Control": "private, no-store",
      });
      response.end(Buffer.from(await proxied.arrayBuffer()));
      return;
    }
    if (
      url.pathname.startsWith("/assets/") ||
      url.pathname.startsWith("/icons/") ||
      ["/apple-touch-icon.png", "/manifest.webmanifest"].includes(url.pathname)
    ) {
      const path = resolve(client, "." + decodeURIComponent(url.pathname));
      if (!path.startsWith(resolve(client) + sep)) {
        response.writeHead(400).end();
        return;
      }
      try {
        const bytes = await readFile(path);
        response.setHeader(
          "Content-Type",
          types[extname(path)] ?? "application/octet-stream",
        );
        response.end(bytes);
      } catch {
        response.writeHead(404).end();
      }
      return;
    }
    const headers = new Headers();
    for (const [key, value] of Object.entries(request.headers)) {
      if (value !== undefined)
        headers.set(key, Array.isArray(value) ? value.join(", ") : value);
    }
    const result = await handler(
      new Request(url, { method: request.method, headers }),
    );
    response.writeHead(result.status, Object.fromEntries(result.headers));
    response.end(Buffer.from(await result.arrayBuffer()));
  } catch (error) {
    serverErrors.push(String(error));
    if (!response.headersSent) response.writeHead(500);
    response.end();
  }
});

test("recording registry plays real media and searches without mobile overflow", async () => {
  accounts = true;
  operatorAccount = true;
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("open", origin + "/admin/recordings");
    await browser("wait", ".recording-preview");
    for (const width of [320, 390, 678, 1024]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-recording').scrollWidth <= document.querySelector('.admin-recording').clientWidth",
        ),
        true,
      );
    }
    await browser("click", ".recording-preview");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.recording-preview').getAttribute('aria-label').startsWith('暂停')",
    );
    await browser("click", ".recording-preview");
    assert.match(
      await evaluate(
        "document.querySelector('.recording-preview').getAttribute('aria-label')",
      ),
      /^试听/,
    );
    await browser("click", ".recording-preview");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.recording-preview-track > span').style.width !== '0%'",
    );
    await browser("fill", "input[name=q]", "missing");
    await browser("press", "Enter");
    await browser("wait", "--text", "没有符合条件的录音。");
    assert.equal(
      await evaluate("new URL(location.href).searchParams.get('q')"),
      "missing",
    );
  } finally {
    accounts = false;
    operatorAccount = false;
  }
});

test("visual registry renders private images and searches at mobile widths", async () => {
  accounts = true;
  operatorAccount = true;
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("open", origin + "/admin/assets");
    await browser("wait", ".admin-asset img");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.admin-asset img')?.naturalWidth > 0",
    );
    for (const width of [320, 390, 678, 1024]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    assert.equal(
      await evaluate(
        "document.querySelector('.admin-asset').textContent.includes('LicenseRef-TestOnly')",
      ),
      true,
    );
    await browser("fill", "input[name=q]", "missing");
    await browser("press", "Enter");
    await browser("wait", "--text", "没有符合条件的素材。");
    assert.equal(
      await evaluate("new URL(location.href).searchParams.get('q')"),
      "missing",
    );
  } finally {
    accounts = false;
    operatorAccount = false;
  }
});

test("operator uploads an actual MP3 with rights and a fixed revision", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/admin/recordings");
    await browser("wait", ".admin-asset");
    const snapshot = await browser("snapshot", "-i");
    const ref = Object.entries(snapshot.refs).find(
      ([, item]) => item.role === "button" && item.name === "上传录音",
    )?.[0];
    assert.ok(ref, JSON.stringify(snapshot));
    await browser("click", "@" + ref);
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-dialog').scrollWidth <= document.querySelector('.admin-dialog').clientWidth",
        ),
        true,
      );
    }
    await browser(
      "upload",
      "input[name=file]",
      fileURLToPath(
        new URL(
          "../../../crates/server/tests/fixtures/audio/synthetic.mp3",
          import.meta.url,
        ),
      ),
    );
    for (const [name, value] of Object.entries({
      assetId: "browser-recording-upload",
      source: "test:original",
      license: "LicenseRef-TestOnly",
      creator: "test fixture",
      creditZh: "仅隔离测试",
      reason: "验证实际文件上传",
    }))
      await browser("fill", `.admin-dialog [name=${name}]`, value);
    await browser("check", "input[name=rightsConfirmed]");
    await browser("fill", "textarea[name=reason]", "验证实际文件上传");
    await browser("press", "Tab");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser(
      "wait",
      "--text",
      "录音 browser-recording-upload · 版本 1 已登记。",
    );
    assert.equal(adminWrites.length, 1);
    assert.equal(adminWrites[0].assetId, "browser-recording-upload");
    assert.equal(adminWrites[0].mimeType, "audio/mpeg");
    assert.equal(adminWrites[0].rightsConfirmed, true);
    assert.equal(adminWrites[0].csrf, "controlled-admin-csrf");
    assert.equal(
      adminWrites[0].fileBytes,
      (
        await readFile(
          new URL(
            "../../../crates/server/tests/fixtures/audio/synthetic.mp3",
            import.meta.url,
          ),
        )
      ).length,
    );
    assert.equal(
      await evaluate("document.querySelector('.admin-dialog[open]') === null"),
      true,
    );
    assert.equal(
      await evaluate("document.documentElement.scrollWidth<=innerWidth"),
      true,
    );
  } finally {
    accounts = false;
    operatorAccount = false;
  }
});

test("operator uploads an actual SVG and supplies provenance in the mobile dialog", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/admin/assets");
    await browser("wait", ".admin-asset");
    const snapshot = await browser("snapshot", "-i");
    const ref = Object.entries(snapshot.refs).find(
      ([, item]) => item.role === "button" && item.name === "上传图片",
    )?.[0];
    assert.ok(ref, JSON.stringify(snapshot));
    await browser("click", "@" + ref);
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-dialog').scrollWidth <= document.querySelector('.admin-dialog').clientWidth",
        ),
        true,
      );
    }
    await browser(
      "upload",
      "input[name=file]",
      fileURLToPath(
        new URL("../../../test-fixtures/visuals/avatars/camille.svg", import.meta.url),
      ),
    );
    for (const [name, value] of Object.entries({
      assetId: "browser-upload",
      altZh: "浏览器上传测试",
      source: "test:original",
      license: "LicenseRef-TestOnly",
      creator: "test fixture",
      creditZh: "仅隔离测试",
      reason: "验证实际文件上传",
    }))
      await browser("fill", `.admin-dialog [name=${name}]`, value);
    await browser("check", "input[name=rightsConfirmed]");
    await browser("fill", "textarea[name=reason]", "验证实际文件上传");
    await browser("press", "Tab");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser("wait", "--text", "素材 browser-upload · 版本 1 已登记。");
    assert.equal(adminWrites.length, 1);
    assert.equal(adminWrites[0].assetId, "browser-upload");
    assert.equal(adminWrites[0].mimeType, "image/svg+xml");
    assert.equal(adminWrites[0].rightsConfirmed, true);
    assert.equal(adminWrites[0].csrf, "controlled-admin-csrf");
    assert.equal(adminWrites[0].fileBytes, characterAvatar.length);
    assert.equal(
      await evaluate("document.querySelector('.admin-dialog[open]') === null"),
      true,
    );
    assert.equal(
      await evaluate("document.documentElement.scrollWidth<=innerWidth"),
      true,
    );
  } finally {
    accounts = false;
    operatorAccount = false;
  }
});

test("operator revokes a pending invitation using the mobile admin page", async () => {
  accounts = true;
  operatorAccount = true;
  pendingTokenRevoked = false;
  adminWrites = [];
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/admin/tokens");
    await browser("wait", ".admin-card");
    await browser("click", ".admin-card button");
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("fill", "#token-reason", "隔离邀请撤销测试");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser("wait", "--text", "链接已撤销。");
    assert.equal(adminWrites[0].reason, "隔离邀请撤销测试");
    assert.match(adminWrites[0].operation, /pending-tokens\/[b]+\/revoke$/);
    assert.equal(
      await evaluate("document.querySelector('.admin-dialog').open"),
      false,
    );
    await browser("wait", "--text", "没有待使用的链接。");
    assert.equal(
      await evaluate("document.documentElement.scrollWidth<=innerWidth"),
      true,
    );
  } finally {
    accounts = false;
    operatorAccount = false;
  }
});

test("operator authorizes and revokes ephemeral reference delivery on the mobile page", async () => {
  accounts = true;
  operatorAccount = true;
  referenceGrant = null;
  adminWrites = [];
  characterVoice = structuredClone(voiceSeed.items[0]);
  characterVoice.profile.referenceAudio = {
    assetId: "qa-reference",
    revision: 2,
    transcript: "Bonjour !",
    cloningPermission: "Synthetic fixture only",
  };
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser(
      "open",
      origin +
        "/admin/voice-references?characterId=character-camille&characterRevision=1&voiceRevision=1",
    );
    await browser("wait", ".reference-delivery-form");
    await browser("check", ".reference-delivery-form input[type=checkbox]");
    await browser("fill", "input[name=deliveryReason]", "隔离参考交付测试");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser("wait", ".reference-delivery-url");
    assert.equal(adminWrites[0].voiceRevision, 1);
    assert.equal(adminWrites[0].singleSpeakerConfirmed, true);
    assert.equal(adminWrites[0].reason, "隔离参考交付测试");
    const address = await evaluate(
      "document.querySelector('.reference-delivery-url').value",
    );
    assert.match(address, /\/api\/v1\/voice-references\/c{32}\/d{64}$/);
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.admin-list button')?.getAttribute('aria-disabled') === 'false'",
    );
    await browser("scrollintoview", ".admin-list button");
    await browser("click", ".admin-list button");
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("fill", "input[name=revokeReason]", "隔离撤销测试");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser("wait", "--text", "交付凭据已撤销。");
    assert.equal(
      await evaluate(
        "document.querySelector('.reference-delivery-url')===null",
      ),
      true,
    );
    assert.equal(adminWrites[1].reason, "隔离撤销测试");
    await browser("open", origin + "/admin/voice-references");
    await browser("wait", ".admin-card");
    assert.equal(
      await evaluate("document.body.textContent.includes('d'.repeat(64))"),
      false,
    );
  } finally {
    accounts = false;
    operatorAccount = false;
    referenceGrant = null;
  }
});

test("operator creates and reconciles a voice enrollment without exposing the reference capability", async () => {
  accounts = true;
  operatorAccount = true;
  referenceGrant = null;
  voiceJob = null;
  adminWrites = [];
  characterVoice = structuredClone(voiceSeed.items[0]);
  characterVoice.profile.referenceAudio = {
    assetId: "qa-reference",
    revision: 2,
    transcript: "Bonjour !",
    cloningPermission: "Synthetic fixture only",
  };
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser(
      "open",
      origin +
        "/admin/voice-references?characterId=character-camille&characterRevision=1&voiceRevision=1",
    );
    await browser("wait", ".reference-delivery-form");
    await browser("check", ".reference-delivery-form input[type=checkbox]");
    await browser("fill", "input[name=deliveryReason]", "isolated creation");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser("wait", ".reference-delivery-url");
    await browser("check", "input[name=voiceCreationConsent]");
    await browser("focus", "input[name=voiceCreationConsent]");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser("wait", ".voice-job-state");
    await browser("wait", "--text", "音色正在处理");
    assert.equal(adminWrites.length, 2);
    assert.equal(adminWrites[1].grantId, "c".repeat(32));
    assert.equal(adminWrites[1].token, "d".repeat(64));
    assert.equal(adminWrites[1].costConfirmed, true);
    assert.equal(await evaluate("location.pathname"), "/admin/voice-jobs");
    assert.equal(
      await evaluate("document.body.textContent.includes('d'.repeat(64))"),
      false,
    );
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    await browser("scrollintoview", ".admin-list button");
    await browser("click", ".admin-list button");
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("fill", "input[name=voiceCheckReason]", "isolated query");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser("wait", "--text", "音色可用，尚未试听或应用");
    assert.equal(adminWrites[2].expectedVersion, 2);
    assert.equal(adminWrites[2].voiceId, null);
    voiceJob = { ...voiceJob, status: "unknown", voiceId: null, version: 5 };
    await browser("open", origin + "/admin/voice-jobs?jobId=" + "e".repeat(32));
    await browser("wait", ".voice-job-state");
    await browser("scrollintoview", ".admin-list button");
    await browser("click", ".admin-list button");
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser(
      "fill",
      "input[name=recoveryVoice]",
      "qwen-audio-3.1-tts-flash-b123456789-found",
    );
    await browser("fill", "input[name=voiceCheckReason]", "isolated recovery");
    await browser("press", "Tab");
    await browser("press", "Enter");
    await browser("wait", "--text", "音色可用，尚未试听或应用");
    assert.equal(adminWrites[3].expectedVersion, 5);
    assert.equal(
      adminWrites[3].voiceId,
      "qwen-audio-3.1-tts-flash-b123456789-found",
    );
    assert.equal(
      adminWrites.filter((w) => w.operation === "/api/v1/operator/voice-jobs")
        .length,
      1,
    );
  } finally {
    accounts = false;
    operatorAccount = false;
    referenceGrant = null;
    voiceJob = null;
  }
});

test("operator auditions a clone privately and confirms a new voice with safe paid-request recovery", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  voiceAuditions = [];
  auditionSynthCalls = 0;
  auditionLostReply = true;
  voiceJob = {
    id: "e".repeat(32),
    grantId: "c".repeat(32),
    characterId: "character-camille",
    characterRevision: 1,
    voiceRevision: 1,
    model: "qwen-audio-3.1-tts-flash",
    prefix: "b123456789",
    version: 6,
    status: "ready",
    voiceId: "qwen-audio-3.1-tts-flash-b123456789-test",
    requestId: "synthetic-query",
    createdAt: "2026-10-07T00:00:00Z",
    updatedAt: "2026-10-07T00:00:00Z",
  };
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser(
      "open",
      origin + "/admin/voice-auditions?jobId=" + voiceJob.id,
    );
    await browser("wait", "--text", "生成一段试听");
    const snapshot = await browser("snapshot", "-i"),
      ref = Object.entries(snapshot.refs).find(
        ([, item]) => item.role === "button" && item.name === "生成一段试听",
      )?.[0];
    assert.ok(ref);
    await browser("click", "@" + ref);
    await browser("wait", "textarea[name=auditionText]");
    await browser(
      "fill",
      "textarea[name=auditionText]",
      "Bonjour ! Je voudrais une baguette.",
    );
    await browser(
      "fill",
      "input[name=auditionEmotion]",
      "A warm, politely expectant request.",
    );
    await browser(
      "fill",
      "textarea[name=auditionReason]",
      "isolated audition generation",
    );
    await browser("check", "input[name=auditionConsent]");
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-dialog[open]').scrollWidth<=document.querySelector('.admin-dialog[open]').clientWidth",
        ),
        true,
      );
    }
    await browser("focus", ".admin-dialog[open] .primary");
    await browser("press", "Enter");
    await browser("wait", "--text", "操作未确认");
    assert.equal(
      await evaluate("document.querySelector('[name=auditionText]').readOnly"),
      true,
    );
    assert.equal(auditionSynthCalls, 1);
    await browser("focus", ".admin-dialog[open] .primary");
    await browser("press", "Enter");
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('.admin-dialog[open]')",
    );
    await browser("wait", "--text", "试听已生成");
    assert.equal(adminWrites.length, 2);
    assert.equal(adminWrites[0].id, adminWrites[1].id);
    assert.equal(adminWrites[0].expectedCloneVersion, 6);
    assert.equal(adminWrites[0].costConfirmed, true);
    assert.equal(auditionSynthCalls, 1);
    await browser("scrollintoview", ".recording-preview");
    await browser("click", ".recording-preview");
    await browser(
      "wait",
      "--fn",
      "parseFloat(document.querySelector('.recording-preview-track > span').style.width)>0",
    );
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    const reviewSnap = await browser("snapshot", "-i"),
      reviewRef = Object.entries(reviewSnap.refs).find(
        ([, item]) => item.role === "button" && item.name === "确认试听结果",
      )?.[0];
    assert.ok(reviewRef);
    await browser("click", "@" + reviewRef);
    await browser("wait", "input[name=auditionConsent]");
    assert.equal(
      await evaluate(
        "document.querySelector('.admin-dialog[open] .primary').disabled",
      ),
      true,
    );
    await browser("check", "input[name=auditionConsent]");
    await browser(
      "fill",
      "textarea[name=auditionReason]",
      "isolated approval, not real audio review",
    );
    await browser("focus", ".admin-dialog[open] .primary");
    await browser("press", "Enter");
    await browser("press", "Enter");
    await browser("wait", "--text", "已通过 · 声音 v2");
    assert.equal(adminWrites.length, 3);
    assert.equal(adminWrites[2].heard, true);
    assert.equal(adminWrites[2].accepted, true);
    assert.equal(adminWrites[2].expectedVoiceRevision, 1);
    assert.equal(auditionSynthCalls, 1);
  } finally {
    accounts = false;
    operatorAccount = false;
    voiceJob = null;
    voiceAuditions = [];
    auditionLostReply = false;
  }
});

test("operator auditions a system voice for a first profile with custom choices and safe paid-request recovery", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  voiceAuditions = [];
  auditionSynthCalls = 0;
  auditionLostReply = true;
  characterVoice = { ...voiceSeed.items[0], voiceRevision: 0, profile: null };
  voiceJob = null;
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser(
      "open",
      origin +
        "/admin/voice-auditions?characterId=character-camille&characterRevision=1",
    );
    await browser("wait", "--text", "生成一段试听");
    const snapshot = await browser("snapshot", "-i"),
      ref = Object.entries(snapshot.refs).find(
        ([, item]) => item.role === "button" && item.name === "生成一段试听",
      )?.[0];
    assert.ok(ref);
    await browser("click", "@" + ref);
    await browser("wait", "textarea[name=auditionText]");
    const choices = await browser("snapshot", "-i");
    const voiceRef = Object.entries(choices.refs).find(
      ([, item]) =>
        item.role === "button" && item.name.startsWith("法语音色："),
    )?.[0];
    assert.ok(voiceRef);
    await browser("click", "@" + voiceRef);
    await browser("wait", ".choice-dialog[open]");
    const voiceOptions = await browser("snapshot", "-i");
    const choiceRef = Object.entries(voiceOptions.refs).find(
      ([, item]) => item.role === "radio" && item.name.startsWith("龙安欢"),
    )?.[0];
    assert.ok(choiceRef);
    await browser("click", "@" + choiceRef);
    await browser(
      "fill",
      "input[name=candidatePersonality]",
      "Curious and kind.",
    );
    await browser(
      "fill",
      "input[name=candidateStyle]",
      "Gentle, clear French.",
    );
    await browser(
      "fill",
      "input[name=candidateEmotion]",
      "Warm and thoughtful.",
    );
    await browser("fill", "input[name=candidateRate]", "0.85");
    assert.equal(
      await evaluate("document.querySelector('select')===null"),
      true,
    );
    await browser(
      "fill",
      "textarea[name=auditionText]",
      "Bonjour ! Je voudrais une baguette.",
    );
    await browser(
      "fill",
      "input[name=auditionEmotion]",
      "A warm, politely expectant request.",
    );
    await browser(
      "fill",
      "textarea[name=auditionReason]",
      "isolated audition generation",
    );
    await browser("check", "input[name=auditionConsent]");
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-dialog[open]').scrollWidth<=document.querySelector('.admin-dialog[open]').clientWidth",
        ),
        true,
      );
    }
    await browser("focus", ".admin-dialog[open] .primary");
    await browser("press", "Enter");
    await browser("wait", "--text", "操作未确认");
    assert.equal(
      await evaluate("document.querySelector('[name=auditionText]').readOnly"),
      true,
    );
    assert.equal(auditionSynthCalls, 1);
    assert.equal(
      await evaluate(
        "document.querySelector('[name=candidatePersonality]').readOnly",
      ),
      true,
    );
    await browser("focus", ".admin-dialog[open] .primary");
    await browser("press", "Enter");
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('.admin-dialog[open]')",
    );
    await browser("wait", "--text", "试听已生成");
    assert.equal(adminWrites.length, 2);
    assert.equal(adminWrites[0].id, adminWrites[1].id);
    assert.equal(adminWrites[0].expectedCloneVersion, null);
    assert.equal(adminWrites[0].cloneJobId, null);
    assert.deepEqual(adminWrites[0], adminWrites[1]);
    assert.equal(adminWrites[0].candidate.expectedVoiceRevision, 0);
    assert.equal(adminWrites[0].candidate.profile.voiceId, "longanhuan_v3.1");
    assert.equal(
      adminWrites[0].candidate.profile.personality,
      "Curious and kind.",
    );
    assert.equal(adminWrites[0].candidate.profile.rate, 0.85);

    assert.equal(adminWrites[0].costConfirmed, true);
    assert.equal(auditionSynthCalls, 1);
    await browser("scrollintoview", ".recording-preview");
    await browser("click", ".recording-preview");
    await browser(
      "wait",
      "--fn",
      "parseFloat(document.querySelector('.recording-preview-track > span').style.width)>0",
    );
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    const reviewSnap = await browser("snapshot", "-i"),
      reviewRef = Object.entries(reviewSnap.refs).find(
        ([, item]) => item.role === "button" && item.name === "确认试听结果",
      )?.[0];
    assert.ok(reviewRef);
    await browser("click", "@" + reviewRef);
    await browser("wait", "input[name=auditionConsent]");
    assert.equal(
      await evaluate(
        "document.querySelector('.admin-dialog[open] .primary').disabled",
      ),
      true,
    );
    await browser("check", "input[name=auditionConsent]");
    await browser(
      "fill",
      "textarea[name=auditionReason]",
      "isolated approval, not real audio review",
    );
    await browser("focus", ".admin-dialog[open] .primary");
    await browser("press", "Enter");
    await browser("press", "Enter");
    await browser("wait", "--text", "已通过 · 声音 v1");
    assert.equal(adminWrites.length, 3);
    assert.equal(adminWrites[2].heard, true);
    assert.equal(adminWrites[2].accepted, true);
    assert.equal(adminWrites[2].expectedVoiceRevision, 0);
    assert.equal(auditionSynthCalls, 1);
    assert.equal(characterVoice.voiceRevision, 1);
    assert.deepEqual(characterVoice.profile, adminWrites[0].candidate.profile);
  } finally {
    accounts = false;
    operatorAccount = false;
    voiceJob = null;
    characterVoice = { ...voiceSeed.items[0], voiceRevision: 0, profile: null };
    voiceAuditions = [];
    auditionLostReply = false;
  }
});

test("operator creates a character using a private avatar picker", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  characterVoice = { ...voiceSeed.items[0], voiceRevision: 0, profile: null };
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/admin/characters");
    await browser("wait", ".character-profile");
    const snapshot = await browser("snapshot", "-i");
    const ref = Object.entries(snapshot.refs).find(
      ([, item]) => item.role === "button" && item.name === "新建角色",
    )?.[0];
    assert.ok(ref);
    await browser("click", "@" + ref);
    await browser("wait", ".avatar-picker button");
    await browser(
      "fill",
      ".admin-dialog[open] input[name=characterId]",
      "character-browser",
    );
    await browser(
      "fill",
      ".admin-dialog[open] input[name=displayName]",
      "Émile",
    );
    await browser("click", ".avatar-picker button");
    await browser(
      "fill",
      ".admin-dialog[open] textarea[name=reason]",
      "隔离角色登记测试",
    );
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-dialog[open]').scrollWidth<=document.querySelector('.admin-dialog[open]').clientWidth",
        ),
        true,
      );
    }
    await browser("focus", ".admin-dialog[open] .primary");
    await browser("press", "Enter");
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('.admin-dialog[open]')",
    );
    await browser("wait", "--text", "Émile");
    assert.equal(adminWrites.length, 1);
    assert.equal(adminWrites[0].expectedRevision, 0);
    assert.equal(adminWrites[0].avatarId, "avatar-camille-v1");
    assert.equal(adminWrites[0].avatarRevision, 1);
    assert.equal(adminWrites[0].displayName, "Émile");
    assert.equal(adminWrites[0].reason, "隔离角色登记测试");
  } finally {
    accounts = false;
    operatorAccount = false;
    characterVoice = { ...voiceSeed.items[0], voiceRevision: 0, profile: null };
  }
});

test("operator versions a character voice profile through the real mobile page", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  characterVoice = { ...voiceSeed.items[0], voiceRevision: 0, profile: null };
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/admin/characters");
    await browser("wait", ".character-profile");
    assert.equal(
      await evaluate("document.documentElement.scrollWidth<=innerWidth"),
      true,
    );
    const voiceSnapshot = await browser("snapshot", "-i");
    const voiceRef = Object.entries(voiceSnapshot.refs).find(
      ([, item]) => item.role === "button" && item.name === "配置声音档案",
    )?.[0];
    assert.ok(voiceRef);
    await browser("click", "@" + voiceRef);
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("check", ".admin-dialog[open] label input[type=checkbox]");
    await browser("wait", ".reference-recording-option");
    for (const width of [320, 390]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-dialog[open]').scrollWidth <= document.querySelector('.admin-dialog[open]').clientWidth",
        ),
        true,
      );
    }
    await browser("click", ".reference-recording-option > button:first-child");
    await browser("fill", "textarea[name=referenceTranscript]", "Bonjour !");
    await browser(
      "fill",
      "textarea[name=cloningPermission]",
      "Synthetic protocol test only; no real speaker.",
    );
    await browser("click", ".reference-recording-option .icon-button");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.reference-recording-option .icon-button').getAttribute('aria-label').startsWith('暂停')",
    );
    await browser("click", ".reference-recording-option .icon-button");
    await browser(
      "fill",
      ".reference-recording-picker input[type=search]",
      "reference-fixture",
    );
    await browser("press", "Enter");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.reference-recording-option')?.textContent.includes('qa-reference')",
    );
    assert.equal(
      await evaluate(
        "document.querySelectorAll('.reference-recording-option').length",
      ),
      2,
    );
    await browser(
      "click",
      ".reference-recording-option:nth-child(2) > button:first-child",
    );
    assert.equal(
      await evaluate(
        "document.querySelector('textarea[name=cloningPermission]').value",
      ),
      "",
    );
    assert.equal(
      await evaluate(
        "document.querySelector('textarea[name=referenceTranscript]').value",
      ),
      "",
    );
    await browser("fill", "textarea[name=referenceTranscript]", "Bonjour !");
    await browser(
      "fill",
      "textarea[name=cloningPermission]",
      "Synthetic protocol test only; no real speaker.",
    );
    await browser(
      "fill",
      ".admin-dialog[open] form > label:first-of-type textarea",
      "Warm, curious and politely reserved.",
    );
    await browser(
      "fill",
      ".admin-dialog[open] form > label:last-of-type input",
      "隔离声音档案测试",
    );
    await browser("press", "Tab");
    assert.equal(await evaluate("document.activeElement.type"), "submit");
    assert.deepEqual(
      await evaluate(
        "[...document.querySelector('.admin-dialog[open] form').querySelectorAll(':invalid')].map(el=>({name:el.name,value:el.value,message:el.validationMessage}))",
      ),
      [],
    );
    await browser("press", "Enter");
    await browser("wait", "--text", "声音档案已保存为新版本。");
    assert.equal(
      await evaluate("document.querySelector('.admin-dialog').open"),
      false,
    );
    assert.equal(adminWrites[0].expectedVoiceRevision, 0);
    assert.equal(
      adminWrites[0].profile.personality,
      "Warm, curious and politely reserved.",
    );
    assert.equal(adminWrites[0].reason, "隔离声音档案测试");
    assert.equal(adminWrites[0].profile.referenceAudio.assetId, "qa-reference");
    assert.equal(adminWrites[0].profile.referenceAudio.revision, 2);
    assert.equal(adminWrites[0].profile.referenceAudio.transcript, "Bonjour !");
    assert.equal(
      adminWrites[0].profile.referenceAudio.cloningPermission,
      "Synthetic protocol test only; no real speaker.",
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.character-profile-head img').naturalWidth>0",
      ),
      true,
    );
  } finally {
    accounts = false;
    operatorAccount = false;
  }
});

test("operator confirms final lesson listening with explicit declaration and exact lost-reply retry", async () => {
  accounts = true;
  operatorAccount = true;
  finalListening = true;
  finalLostReply = true;
  finalDecision = null;
  adminWrites = [];
  finalStatus = {
    required: true,
    published: false,
    lessonHash: "a".repeat(64),
    version: 0,
    accepted: false,
    reason: "",
    actor: null,
  };
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser(
      "open",
      origin + `/author-preview?lessonId=${lesson.id}&revision=1`,
    );
    await browser("wait", ".lesson-audio-review");
    const check = ".lesson-audio-review input[type=checkbox]",
      reason = ".lesson-audio-review textarea";
    assert.equal(
      await evaluate(`document.querySelector('${check}').checked`),
      false,
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.lesson-audio-review .primary').disabled",
      ),
      true,
    );
    await browser("fill", reason, "仅为界面协议确认，非真实法语审听");
    assert.equal(
      await evaluate(
        "document.querySelector('.lesson-audio-review .primary').disabled",
      ),
      true,
    );
    await browser("check", check);
    await browser("scrollintoview", ".lesson-audio-review .primary");
    await browser("focus", ".lesson-audio-review .primary");
    await browser("press", "Enter");
    await browser("wait", "--text", "操作未确认，请刷新核对状态后重试。");
    assert.equal(
      await evaluate(
        `document.querySelector('${reason}').matches(':disabled')`,
      ),
      true,
    );
    await browser("scrollintoview", ".brand");
    await browser("click", ".brand");
    await browser("wait", ".lesson-audio-review dialog[open]");
    await assertNamedModal("审核结果尚未确认");
    await browser("press", "Escape");
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('.lesson-audio-review dialog').open",
    );
    assert.equal(
      adminWrites.length,
      1,
      "staying after a blocked navigation must not issue a decision",
    );
    await browser("scrollintoview", ".lesson-audio-review .primary");
    await browser("focus", ".lesson-audio-review .primary");
    await browser("press", "Enter");
    await browser("wait", "--text", "整课试听已通过，可以回到后台审批课程。");
    assert.equal(adminWrites.length, 2);
    assert.deepEqual(adminWrites[0], adminWrites[1]);
    assert.equal(adminWrites[0].heard, true);
    assert.equal(adminWrites[0].version, 0);
    assert.equal(
      await evaluate(`document.querySelector('${check}').checked`),
      false,
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.lesson-audio-review p').textContent.includes('已通过最终试听')",
      ),
      true,
    );
    for (const width of [320, 390, 678, 1024]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    assert.deepEqual(serverErrors, []);
  } finally {
    finalListening = false;
    accounts = false;
    operatorAccount = false;
  }
});

test("operator distinguishes content approval from final listening without losing rejection", async () => {
  accounts = true;
  operatorAccount = true;
  adminPaged = false;
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    for (const [content, audio, approve, reject] of [
      [true, false, false, true],
      [false, false, false, false],
      [false, true, true, false],
      [true, true, false, true],
    ]) {
      adminApproved = content;
      adminAudio = { required: true, accepted: audio };
      await browser("open", origin + "/admin");
      await browser("wait", ".admin-card");
      const texts = await evaluate(
        "[...document.querySelectorAll('.admin-card button')].map(b=>b.textContent)",
      );
      assert.equal(texts.includes("批准课程"), approve);
      assert.equal(texts.includes("退回修改"), reject);
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-card').textContent.includes('整课试听尚未通过')",
        ),
        !audio,
      );
      assert.equal(
        await evaluate(
          "document.querySelector('.admin-card a').getAttribute('href')",
        ),
        `/author-preview?lessonId=${lesson.id}&revision=1`,
      );
      for (const width of [320, 390, 678, 1024]) {
        await browser("set", "viewport", String(width), "844");
        assert.equal(
          await evaluate("document.documentElement.scrollWidth<=innerWidth"),
          true,
        );
      }
      if (content && !audio) {
        adminWrites = [];
        await browser("scrollintoview", ".admin-card button");
        await browser("click", ".admin-card button");
        await browser("wait", ".admin-dialog[open]");
        await assertNamedModal();
        await browser(
          "fill",
          "#admin-reason",
          "仅协议测试：内容退回独立于试听",
        );
        await browser("focus", ".admin-dialog .primary");
        await browser("press", "Enter");
        await browser(
          "wait",
          "--fn",
          "!document.querySelector('.admin-dialog').open",
        );
        assert.equal(adminWrites.length, 1);
        assert.equal(adminWrites[0].approved, false);
      }
    }
  } finally {
    adminAudio = { required: false, accepted: false };
    adminApproved = false;
    accounts = false;
    operatorAccount = false;
  }
});

test("operator searches and paginates fixed course versions using the real admin page", async () => {
  accounts = true;
  operatorAccount = true;
  adminPaged = true;
  overviewReads = [];
  adminWrites = [];
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/admin");
    await browser("wait", ".admin-card");
    assert.equal(
      await evaluate("document.querySelectorAll('.admin-card').length"),
      20,
    );
    for (const width of [320, 390, 528, 678, 1024]) {
      await browser("set", "viewport", String(width), "884");
      const layout = await evaluate(`(() => {
        const links = [...document.querySelectorAll('.admin-sections a')];
        const rects = links.map(a => a.getBoundingClientRect());
        const input = document.querySelector('input[name=q]');
        const label = input.closest('label');
        const range = document.createRange();
        range.selectNode(label.firstChild);
        const text = range.getBoundingClientRect();
        const tool = document.querySelector('.admin-tool').getBoundingClientRect();
        const card = document.querySelector('.admin-card').getBoundingClientRect();
        return {
          tools: links.length,
          overlap: rects.some((a, i) => rects.slice(i + 1).some(b =>
            Math.min(a.right, b.right) > Math.max(a.left, b.left) &&
            Math.min(a.bottom, b.bottom) > Math.max(a.top, b.top))),
          targets: rects.every(r => r.height >= 44),
          searchSeparated: input.getBoundingClientRect().top >= text.bottom + 4,
          cardSeparated: card.top >= tool.bottom + 16,
          noOverflow: document.documentElement.scrollWidth <= innerWidth
        };
      })()`);
      assert.deepEqual(
        layout,
        {
          tools: 8,
          overlap: false,
          targets: true,
          searchSeparated: true,
          cardSeparated: true,
          noOverflow: true,
        },
        `admin layout at ${width}px: ${JSON.stringify(layout)}`,
      );
    }
    await browser("set", "viewport", "390", "844");
    await browser("fill", "input[name=q]", "Pagination");
    await browser("focus", "input[name=q]");
    await browser("press", "Enter");
    await browser("wait", "--fn", "location.search.includes('q=Pagination')");
    await browser(
      "wait",
      "--fn",
      "document.activeElement===document.querySelector('h1')",
    );
    const pageState = await evaluate(
      "({links:[...document.querySelectorAll('.admin-pagination a')].map(a=>({text:a.textContent,href:a.getAttribute('href')})),cards:document.querySelectorAll('.admin-card').length,search:location.search})",
    );
    assert.ok(
      pageState.links.some((a) => a.text === "后续记录"),
      JSON.stringify({ pageState, overviewReads }),
    );
    await browser("scrollintoview", ".admin-pagination a:last-child");
    await browser("click", ".admin-pagination a:last-child");
    await browser(
      "wait",
      "--fn",
      "document.querySelectorAll('.admin-card').length===5",
    );
    assert.equal(
      await evaluate("document.querySelector('h1')===document.activeElement"),
      true,
    );
    assert.match(await evaluate("location.search"), /lessonAfterRevision=6/);
    assert.equal(
      await evaluate(
        "document.querySelector('.admin-card .profile-level').textContent.includes('v5')",
      ),
      true,
    );
    await browser("scrollintoview", ".admin-card:first-child button");
    await browser("click", ".admin-card:first-child button");
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("fill", "#admin-reason", "分页后的固定版本审批");
    await browser("focus", ".admin-dialog .primary");
    await browser("press", "Enter");
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('.admin-dialog[open]')",
    );
    assert.deepEqual(adminWrites, [
      { version: 0, approved: true, reason: "分页后的固定版本审批" },
    ]);
    assert.equal(
      await evaluate("document.querySelectorAll('.admin-card').length"),
      5,
    );
    await browser("scrollintoview", ".admin-pagination a");
    await browser("click", ".admin-pagination a");
    await browser(
      "wait",
      "--fn",
      "document.querySelectorAll('.admin-card').length===20",
    );
    await browser("fill", "input[name=q]", "no-match");
    await browser("focus", "input[name=q]");
    await browser("press", "Enter");
    await browser("wait", "--text", "没有匹配的课程版本");
    await browser(
      "scrollintoview",
      ".reader-mode a[href='/admin?tab=releases']",
    );
    await browser("click", ".reader-mode a[href='/admin?tab=releases']");
    await browser("wait", "--text", "pagination-release");
    assert.equal(
      await evaluate("document.querySelector('input[name=q]').value"),
      "",
    );
    await browser("fill", "input[name=q]", "no-match");
    await browser("focus", "input[name=q]");
    await browser("press", "Enter");
    await browser("wait", "--text", "没有匹配的发布目录");
    assert.ok(
      overviewReads.some(
        (url) =>
          url.includes("lessonQ=Pagination") &&
          url.includes("lessonAfterRevision=6"),
      ),
    );
    assert.ok(overviewReads.some((url) => url.includes("releaseQ=no-match")));
    for (const width of [320, 390, 678, 1024]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    assert.deepEqual(serverErrors, []);
  } finally {
    adminPaged = false;
    accounts = false;
    operatorAccount = false;
  }
});

test("operator enters admin from profile and approves using the centered dialog", async () => {
  accounts = true;
  operatorAccount = true;
  adminApproved = false;
  adminWrites = [];
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/profile");
    await browser("wait", ".setting-link[href='/admin']");
    const gap = await evaluate(
      `(() => { const heading=[...document.querySelectorAll('.settings-page > h2')].find(el=>el.textContent==='阅读');return heading.getBoundingClientRect().top-heading.previousElementSibling.getBoundingClientRect().bottom; })()`,
    );
    assert.equal(gap, 32);
    await browser("click", ".setting-link[href='/admin']");
    await browser("wait", ".admin-card");
    await browser("scrollintoview", ".admin-card button");
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "批准课程",
      "--exact",
    );
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal("批准课程");
    await browser("press", "Escape");
    assert.equal(
      await evaluate("document.activeElement.textContent.trim()"),
      "批准课程",
    );
    await browser("press", "Enter");
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal("批准课程");
    const geometry = await evaluate(
      `(() => { const r=document.querySelector('.admin-dialog').getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:innerWidth,height:innerHeight}; })()`,
    );
    assert.ok(
      geometry.left >= 0 &&
        geometry.right <= geometry.width &&
        geometry.top >= 0 &&
        geometry.bottom <= geometry.height,
    );
    await browser("fill", "#admin-reason", "界面协议测试批准");
    await browser("focus", ".admin-dialog .primary");
    await browser("press", "Enter");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.admin-card-heading')?.textContent.includes('已批准')",
    );
    assert.deepEqual(adminWrites, [
      { version: 0, approved: true, reason: "界面协议测试批准" },
    ]);
    assert.equal(
      await evaluate("document.querySelectorAll('.admin-dialog[open]').length"),
      0,
    );
    assert.equal(
      await evaluate("document.documentElement.scrollWidth > innerWidth"),
      false,
    );
    await browser(
      "scrollintoview",
      ".reader-mode a[href='/admin?tab=releases']",
    );
    await browser("click", ".reader-mode a[href='/admin?tab=releases']");
    await browser("wait", "--text", "还没有发布目录");
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "创建发布目录",
      "--exact",
    );
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("upload", "#admin-document", releaseUpload);
    await browser("fill", "#admin-reason", "界面目录导入测试");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.admin-dialog .primary')?.disabled === false",
    );
    await browser("focus", ".admin-dialog .primary");
    await browser("press", "Enter");
    await browser("wait", "--text", "发布目录已通过检查，可以预览或切换。");
    assert.equal(adminWrites[1].operation, "/api/v1/operator/releases/stage");
    await browser("scrollintoview", ".reader-mode a[href='/admin']");
    await browser("click", ".reader-mode a[href='/admin']");
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "导入课程",
      "--exact",
    );
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("upload", "#admin-document", lessonUpload);
    await browser("fill", "#admin-reason", "界面课程导入测试");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.admin-dialog .primary')?.disabled === false",
    );
    await browser("focus", ".admin-dialog .primary");
    await browser("press", "Enter");
    await browser("wait", "--text", "课程 v1 已导入，可以预览和审批。");
    assert.equal(adminWrites[2].operation, "/api/v1/operator/lessons/import");
    assert.deepEqual(JSON.parse(adminWrites[2].document), source);
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "导入课程",
      "--exact",
    );
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    assert.equal(
      await evaluate("document.querySelector('#admin-document').files.length"),
      0,
    );
    await writeFile(
      lessonUpload,
      JSON.stringify({
        ...source,
        title: { ...source.title, fr: "preflight-invalid-marker" },
      }),
    );
    await browser("upload", "#admin-document", lessonUpload);
    await browser("fill", "#admin-reason", "检查失败不能导入");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.admin-dialog .primary')?.disabled === false",
    );
    await browser("focus", ".admin-dialog .primary");
    await browser("press", "Enter");
    await browser(
      "wait",
      "--text",
      "文件需要修改：第 4 行，第 12 列，字段 /title/fr。",
    );
    await browser("wait", "--text", "登记素材版本不存在。");
    assert.equal(
      adminWrites.filter(
        (item) => item.operation === "/api/v1/operator/lessons/import",
      ).length,
      1,
    );
    assert.equal(
      await evaluate("document.querySelector('.admin-dialog').open"),
      true,
    );
    assert.equal(
      await evaluate("document.querySelector('#admin-reason').value"),
      "检查失败不能导入",
    );
    await writeFile(lessonUpload, JSON.stringify(source));
    await browser("press", "Escape");
    await browser("click", "a[href='/admin/history']");
    await browser("wait", ".admin-history");
    assert.equal(
      await evaluate("document.querySelector('h1').textContent"),
      "审批与发布记录",
    );
    assert.equal(
      await evaluate("document.querySelector('time').dateTime"),
      "2026-10-07T01:02:03.123456Z",
    );
    assert.match(
      await evaluate("document.querySelector('.admin-actor').textContent"),
      /user:101/,
    );
    await browser("click", "a[href^='/admin/history?']");
    await browser("wait", "--text", "较早的目录检查");
    assert.match(
      await evaluate("document.querySelector('.admin-actor').textContent"),
      /local-author-cli/,
    );
    assert.equal(
      await evaluate(
        "document.querySelectorAll(\"a[href^='/admin/history?']\").length",
      ),
      0,
    );
    await browser("back");
    await browser("wait", "a[href^='/admin/history?']");
    assert.equal(await evaluate("document.activeElement.tagName"), "H1");
    await browser("click", "a[href='/admin']");
    await browser("wait", "a[href='/admin/accounts']");
    await browser("click", "a[href='/admin/accounts']");
    await browser("wait", "#account-search");
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "邀请新账号",
      "--exact",
    );
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("fill", "#account-email", "invited@example.test");
    await browser("fill", "#account-reason", "隔离邀请界面测试");
    await browser("focus", ".admin-dialog .primary");
    await browser("press", "Enter");
    await browser("press", "Enter");
    await browser("wait", "#account-link");
    assert.equal(await evaluate("document.activeElement.id"), "account-link");
    assert.equal(
      adminWrites.filter(
        (item) => item.operation === "/api/v1/operator/accounts/token",
      ).length,
      1,
    );
    assert.equal(
      await evaluate(
        "document.querySelector('#account-link').value.startsWith(location.origin+'/invite#token=')",
      ),
      true,
    );
    assert.equal(
      await evaluate(
        "Object.keys(localStorage).some(key=>localStorage.getItem(key).includes('ffffffffffffffff'))",
      ),
      false,
    );
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "关闭",
      "--exact",
    );
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "生成密码重置链接",
      "--exact",
    );
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    assert.equal(
      await evaluate("document.querySelector('#account-link')===null"),
      true,
    );
    assert.equal(
      await evaluate("document.querySelector('#account-email').readOnly"),
      true,
    );
    await browser("press", "Escape");
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "设为管理员",
      "--exact",
    );
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("fill", "#account-reason", "隔离权限界面测试");
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "确认修改权限",
      "--exact",
    );
    await browser("wait", "--text", "改为学习者");
    assert.equal(
      await evaluate("document.querySelector('.admin-dialog').open"),
      false,
    );
    const change = adminWrites.find(
      (item) => item.operation === "/api/v1/operator/accounts/101/role",
    );
    assert.equal(change.expectedRole, "learner");
    assert.equal(change.role, "operator");
    assert.equal(change.reason, "隔离权限界面测试");
    managedRole = "learner";
    assert.equal(
      await evaluate(
        "document.querySelector('a[href=\"/admin/accounts/101/sessions\"]')?.textContent",
      ),
      "登录会话",
    );
    const sessionsSnapshot = await browser("snapshot", "-i");
    assert.match(JSON.stringify(sessionsSnapshot), /登录会话/);
    const sessionsRef = Object.entries(sessionsSnapshot.refs).find(
      ([, item]) => item.role === "link" && item.name === "登录会话",
    )?.[0];
    assert.ok(sessionsRef);
    await browser("click", `@${sessionsRef}`);
    await browser("wait", "--text", "撤销此会话");
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "撤销此会话",
      "--exact",
    );
    await browser("wait", ".admin-dialog[open]");
    await assertNamedModal();
    await browser("fill", "#session-reason", "隔离会话撤销测试");
    await browser(
      "find",
      "role",
      "button",
      "click",
      "--name",
      "确认撤销",
      "--exact",
    );
    await browser("wait", "--text", "没有有效的登录会话。");
    assert.equal(
      await evaluate("document.querySelector('.admin-dialog').open"),
      false,
    );
    assert.equal(
      adminWrites.find((item) => item.operation?.endsWith("/revoke")).reason,
      "隔离会话撤销测试",
    );
    const accountSnapshot = await browser("snapshot", "-i");
    const accountRef = Object.entries(accountSnapshot.refs).find(
      ([, item]) => item.role === "link" && item.name === "账号管理",
    )?.[0];
    assert.ok(accountRef);
    await browser("click", `@${accountRef}`);
    await browser("wait", "#account-search");
    managedSessionRevoked = false;
    await browser("click", "a[href^='/admin/accounts?']");
    await browser("wait", "--text", "较早账号");
    assert.equal(await evaluate("document.activeElement.tagName"), "H1");
    await browser("back");
    await browser("wait", "--text", "测试账号");
    assert.equal(await evaluate("document.activeElement.tagName"), "H1");
  } finally {
    accounts = false;
    operatorAccount = false;
    await browser("cookies", "clear");
  }
});
async function browser(...args) {
  const { stdout } = await execute(
    process.execPath,
    [cli, "--session", session, "--json", ...args],
    { timeout: 30000, maxBuffer: 2 * 1024 * 1024 },
  );
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, result.error);
  return result.data;
}
const evaluate = async (code) => (await browser("eval", code)).result;
async function assertNamedModal(expected) {
  const modal = await evaluate(`(() => {
    const d=[...document.querySelectorAll('dialog:modal')].at(-1);
    if(!d)return null;
    const ids=(d.getAttribute('aria-labelledby')??'').split(/\\s+/).filter(Boolean);
    const labels=ids.map(id=>document.getElementById(id));
    return {name:labels.map(el=>el?.textContent.trim()??'').join(' ').trim(),
      title:d.querySelector('h2')?.textContent.trim(),
      labelsValid:labels.length>0&&labels.every(el=>el&&d.contains(el)),
      focused:d.contains(document.activeElement)};
  })()`);
  assert.ok(modal, "expected an open native modal");
  assert.equal(
    modal.labelsValid,
    true,
    "modal must reference its own visible title",
  );
  assert.equal(modal.name, expected ?? modal.title);
  assert.ok(modal.name.length > 0);
  assert.equal(modal.focused, true, "modal contains keyboard focus");
  const snapshot = await browser("snapshot");
  assert.ok(
    typeof snapshot.snapshot === "string" &&
      snapshot.snapshot.includes(`dialog "${modal.name}"`),
    "native accessibility tree exposes the dialog title",
  );
}
before(async () => {
  await writeFile(lessonUpload, JSON.stringify(source));
  await writeFile(
    releaseUpload,
    JSON.stringify({ id: "browser-release", schemaVersion: "1.0", levels: [] }),
  );
  await new Promise((resolve) => api.listen(0, "127.0.0.1", resolve));
  process.env.INTERNAL_API_URL = `http://127.0.0.1:${api.address().port}`;
  await new Promise((resolve) => web.listen(0, "127.0.0.1", resolve));
  origin = `http://127.0.0.1:${web.address().port}`;
});
after(async () => {
  await unlink(lessonUpload);
  await unlink(releaseUpload);
  await rmdir(uploadDirectory);
  try {
    if (opened) await browser("close");
  } finally {
    for (const server of [web, api]) {
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
    }
    if (originalBase === undefined) delete process.env.INTERNAL_API_URL;
    else process.env.INTERNAL_API_URL = originalBase;
  }
});

test("production SSR hydrates its real shell, preserves mobile widths, routes focus and modal return", async () => {
  const initial = await fetch(origin);
  assert.equal(initial.status, 200);
  assert.match(await initial.text(), /<html[^>]*class="overlay-scroll"/);
  await browser("open", origin);
  opened = true;
  await browser(
    "wait",
    "--fn",
    "!!window.__reactRouterContext && !!document.querySelector('.profile-avatar')",
  );
  await evaluate("window.shellMarker='hydrated-route-test'");
  for (const width of [320, 390, 768, 1440]) {
    await browser("set", "viewport", String(width), "740");
    assert.deepEqual(
      await evaluate(
        "({width:document.documentElement.clientWidth,overflow:document.documentElement.scrollWidth})",
      ),
      { width, overflow: width },
    );
    assert.equal(
      await evaluate(
        "getComputedStyle(document.documentElement).scrollbarWidth",
      ),
      "none",
    );
    assert.equal(
      await evaluate(
        "(()=>{const r=document.querySelector('.topbar').getBoundingClientRect();return r.left>=0&&r.right<=innerWidth})()",
      ),
      true,
    );
  }
  await browser("set", "viewport", "390", "740");
  await browser("focus", ".skip-link");
  await browser("press", "Enter");
  assert.equal(await evaluate("document.activeElement.id"), "page-content");
  await browser("focus", ".profile-avatar");
  await browser("press", "Enter");
  await browser(
    "wait",
    "--fn",
    "location.pathname==='/profile' && document.activeElement.matches('main h1')",
  );
  assert.equal(await evaluate("window.shellMarker"), "hydrated-route-test");
  await browser("focus", "button[aria-label^='朗读速度']");
  await browser("press", "Enter");
  await browser("wait", "dialog[open]");
  assert.equal(
    await evaluate(
      "(()=>{const r=document.querySelector('dialog[open]').getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight})()",
    ),
    true,
  );
  assert.equal(
    await evaluate(
      "document.querySelector('dialog[open]').contains(document.activeElement)",
    ),
    true,
  );
  assert.equal(
    await evaluate(
      "getComputedStyle(document.querySelector('.page-scrollbar')).visibility",
    ),
    "hidden",
  );
  await browser("press", "Escape");
  assert.equal(
    await evaluate(
      "document.activeElement.matches(\"button[aria-label^='朗读速度']\")",
    ),
    true,
  );
  await browser("focus", ".brand");
  await browser("press", "Enter");
  await browser(
    "wait",
    "--fn",
    "location.pathname==='/' && document.activeElement.matches('main h1')",
  );
  await browser("focus", "a[href='/courses']");
  await browser("press", "Enter");
  await browser(
    "wait",
    "--fn",
    "location.pathname==='/courses' && document.activeElement.matches('main h1')",
  );
  assert.equal(await evaluate("window.shellMarker"), "hydrated-route-test");
  assert.deepEqual(serverErrors, []);
  const { errors } = await browser("errors");
  assert.deepEqual(errors, [], "hydration and route errors must fail");
});

test("medium reading uses an animated modal with retained content and keyboard dismissal", async () => {
  const originalBlocks = lesson.blocks;
  const body = lesson.blocks.find((block) => block.type === "dialogue");
  lesson.blocks = originalBlocks.filter(
    (block) => !["dialogue", "article"].includes(block.type) || block === body,
  );
  try {
    await browser("set", "media", "light");
    await browser("set", "viewport", "678", "884");
    await browser("open", `${origin}/lessons/${lesson.id}`);
    opened = true;
    await browser(
      "wait",
      "--fn",
      "!!document.querySelector('.word.known') && !!window.__reactRouterContext && getComputedStyle(document.documentElement).getPropertyValue('--surface').trim()==='#fffdf7'",
    );
    assert.equal(
      await evaluate("document.querySelectorAll('[role=tab]').length"),
      0,
    );
    assert.equal(
      await evaluate(
        "getComputedStyle(document.querySelector('aside.knowledge')).display",
      ),
      "none",
    );
    assert.equal(
      await evaluate("document.documentElement.scrollWidth <= innerWidth"),
      true,
    );
    await browser("focus", ".word.known");
    await browser("press", "Enter");
    await browser("wait", "dialog.knowledge-sheet[open]");
    const motion = await evaluate(
      "(()=>{const d=document.querySelector('dialog.knowledge-sheet');return {card:getComputedStyle(d).animationName,backdrop:getComputedStyle(d,'::backdrop').animationName,inside:d.contains(document.activeElement),text:d.querySelector('h2')?.textContent}})()",
    );
    assert.equal(motion.card, "knowledge-enter");
    assert.equal(motion.backdrop, "knowledge-backdrop-enter");
    assert.equal(motion.inside, true);
    const closing = await evaluate(
      "(()=>{const d=document.querySelector('dialog.knowledge-sheet');d.dispatchEvent(new Event('cancel',{cancelable:true}));return new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve({open:d.open,closing:d.hasAttribute('data-closing'),text:d.querySelector('h2')?.textContent,animation:getComputedStyle(d).animationName}))))})()",
    );
    assert.equal(closing.open, true);
    assert.equal(closing.closing, true);
    assert.equal(closing.text, motion.text);
    assert.equal(closing.animation, "knowledge-leave");
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('dialog.knowledge-sheet').open",
    );
    assert.equal(
      await evaluate("document.activeElement.matches('.word.known')"),
      true,
    );
    await browser("press", "Enter");
    await browser("wait", "dialog.knowledge-sheet[open]");
    await browser("press", "Escape");
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('dialog.knowledge-sheet').open",
    );
    assert.equal(
      await evaluate("document.activeElement.matches('.word.known')"),
      true,
    );
    await browser("set", "media", "light", "reduced-motion");
    await browser("press", "Enter");
    await browser("wait", "dialog.knowledge-sheet[open]");
    assert.equal(
      await evaluate(
        "getComputedStyle(document.querySelector('dialog.knowledge-sheet')).animationName",
      ),
      "none",
    );
    assert.equal(
      await evaluate(
        "getComputedStyle(document.querySelector('dialog.knowledge-sheet'),'::backdrop').animationName",
      ),
      "none",
    );
    await browser("press", "Escape");
    assert.equal(
      await evaluate("document.querySelector('dialog.knowledge-sheet').open"),
      false,
    );
  } finally {
    lesson.blocks = originalBlocks;
    await browser("set", "media", "light");
  }
});

test("lesson errors recover through native keyboard reload and catalog navigation", async () => {
  try {
    accounts = false;
    lessonStatus = 200;
    opened = true;
    await browser("open", origin);
    await browser("wait", "a[href^='/lessons/']");
    lessonStatus = 503;
    await browser("focus", "a[href^='/lessons/']");
    await browser("press", "Enter");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('main h1')?.textContent==='服务暂时不可用'",
    );
    assert.equal(
      await evaluate("document.querySelectorAll('.reading').length"),
      0,
    );
    lessonStatus = 200;
    await browser("focus", ".error-recovery button.primary");
    await browser("press", "Enter");
    await browser("wait", ".reading");
    assert.ok(
      (await evaluate("document.querySelector('main').textContent")).includes(
        lesson.title.fr,
      ),
    );
    for (const [status, title] of [
      [404, "没有找到这页内容"],
      [410, "课程已撤回"],
    ]) {
      lessonStatus = status;
      await browser("open", `${origin}/lessons/${lesson.id}`);
      await browser(
        "wait",
        "--fn",
        `document.querySelector('main h1')?.textContent===${JSON.stringify(title)}`,
      );
      await browser(
        "wait",
        "--fn",
        "Object.keys(document.querySelector('.error-recovery a')).some(key=>key.startsWith('__reactFiber$'))",
      );
      assert.equal(
        await evaluate("document.querySelectorAll('.reading').length"),
        0,
      );
      await browser("focus", ".error-recovery a[href='/courses']");
      await browser("press", "Enter");
      await browser(
        "wait",
        "--fn",
        "location.pathname==='/courses' && document.activeElement.matches('main h1')",
      );
      assert.equal(
        await evaluate(
          "document.activeElement.tagName + ':' + document.activeElement.textContent",
        ),
        "H1:课程",
      );
      await browser("focus", "a[href^='/lessons/']");
      await browser("press", "Enter");
      await browser(
        "wait",
        "--fn",
        `document.querySelector('main h1')?.textContent===${JSON.stringify(title)}`,
      );
      await browser("back");
      await browser(
        "wait",
        "--fn",
        "location.pathname==='/courses' && !!document.querySelector('.courses-page')",
      );
      await browser(
        "wait",
        "--fn",
        "location.pathname==='/courses' && document.activeElement.matches('main h1')",
      );
    }
    assert.deepEqual(serverErrors, []);
    const { errors } = await browser("errors");
    assert.deepEqual(errors, []);
  } finally {
    lessonStatus = 200;
  }
});

test("server-authorized identity replacement discards the old private page before reload warnings", async () => {
  accounts = true;
  identityReads.length = 0;
  identityProof = null;
  try {
    opened = true;
    await browser("open", origin + "/profile");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("open", origin + "/profile");
    opened = true;
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.profile-summary h2')?.textContent==='Alice'",
    );
    await browser("focus", ".profile-edit");
    await browser("press", "Enter");
    await browser("wait", ".profile-dialog[open]");
    await browser("focus", ".profile-dialog input");
    await browser("press", "Control+a");
    await browser("keyboard", "inserttext", "Unsaved Alice");
    await evaluate(
      `window.addEventListener('beforeunload', e => {const proof=JSON.stringify({guarded:e.defaultPrevented,oldProfile:document.querySelector('.profile-summary h2')?.textContent==='Alice',dialogs:document.querySelectorAll('.profile-dialog[open]').length});sessionStorage.setItem('shell-invalidation-proof',proof);navigator.sendBeacon('/__identity-proof',proof)}, {once:true})`,
    );
    await browser("cookies", "set", "brioche.sid", "shell-b");
    assert.equal(await evaluate("document.visibilityState"), "visible");
    // Controlled focus notification exercises the real IdentitySync HTTP read.
    await evaluate("window.dispatchEvent(new Event('focus'))");
    for (let attempt = 0; attempt < 100 && !identityProof; attempt++)
      await new Promise((resolve) => setTimeout(resolve, 20));
    assert.ok(
      identityReads.some(
        (read) => read.id === "shell-b" && read.channel === "browser",
      ),
    );
    assert.deepEqual(identityProof, {
      guarded: false,
      oldProfile: false,
      dialogs: 0,
    });
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.profile-summary h2')?.textContent==='Bob'",
    );
    assert.deepEqual(
      await evaluate(
        "JSON.parse(sessionStorage.getItem('shell-invalidation-proof'))",
      ),
      { guarded: false, oldProfile: false, dialogs: 0 },
    );
    assert.equal(
      await evaluate(
        "document.querySelectorAll('.profile-dialog[open]').length",
      ),
      0,
    );
    assert.ok(
      !(await evaluate("document.querySelector('main').textContent")).includes(
        "Unsaved Alice",
      ),
    );
    await evaluate("sessionStorage.removeItem('shell-invalidation-proof')");
    assert.deepEqual(serverErrors, []);
    const { errors } = await browser("errors");
    assert.deepEqual(errors, []);
  } finally {
    accounts = false;
    await browser("cookies", "clear");
  }
});

test("operator previews and saves a fixed course speech plan with emotion editing and safe recovery", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  speechPlans = [];
  speechLostReply = true;
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser(
      "open",
      origin + "/admin/speech-plans?lessonId=" + lesson.id + "&revision=1",
    );
    await browser("wait", "--text", "核对配音计划");
    assert.equal(adminWrites.length, 0);
    let snap = await browser("snapshot", "-i");
    let ref = Object.entries(snap.refs).find(
      ([, r]) => r.role === "button" && r.name === "核对配音计划",
    )[0];
    await browser("focus", "@" + ref);
    await browser("press", "Enter");
    await browser("wait", "--text", "配音计划预览");
    snap = await browser("snapshot", "-i");
    ref = Object.entries(snap.refs).find(
      ([, r]) => r.role === "textbox" && r.name === "场景情绪",
    )[0];
    await browser("fill", "@" + ref, "Friendly and surprised");
    assert.equal(
      await evaluate(
        "document.querySelectorAll('.speech-plans-page input').length",
      ),
      3,
    );
    assert.equal(
      await evaluate(
        "[...document.querySelectorAll('button')].find(b=>b.textContent==='保存配音计划').disabled",
      ),
      true,
    );
    snap = await browser("snapshot", "-i");
    ref = Object.entries(snap.refs).find(
      ([, r]) => r.role === "button" && r.name === "核对配音计划",
    )[0];
    await browser("click", "@" + ref);
    await browser("wait", "--text", "Friendly and surprised");
    snap = await browser("snapshot", "-i");
    ref = Object.entries(snap.refs).find(
      ([, r]) => r.role === "textbox" && r.name === "保存理由",
    )[0];
    await browser("fill", "@" + ref, "Fixed course voice plan");
    for (const width of [320, 390, 678]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    snap = await browser("snapshot", "-i");
    ref = Object.entries(snap.refs).find(
      ([, r]) => r.role === "button" && r.name === "保存配音计划",
    )[0];
    await browser("click", "@" + ref);
    await browser("wait", "--text", "重试保存同一计划");
    assert.equal(
      await evaluate("document.querySelector('input').disabled"),
      true,
    );
    snap = await browser("snapshot", "-i");
    ref = Object.entries(snap.refs).find(
      ([, r]) => r.role === "button" && r.name === "重试保存同一计划",
    )[0];
    await browser("click", "@" + ref);
    await browser("wait", "--text", "配音计划已保存，尚未生成音频。");
    const writes = adminWrites.filter(
      (w) => w.operation === "/api/v1/operator/speech-plans",
    );
    assert.equal(writes.length, 2);
    assert.deepEqual(writes[0], writes[1]);
    assert.equal(speechPlans.length, 1);
    assert.equal(
      writes[0].preview.selection.emotions["/blocks/1/turns/0"],
      "Friendly and surprised",
    );
    assert.ok(!Object.hasOwn(writes[0].preview.selection, "items"));
    assert.equal(serverErrors.length, 0);
  } catch (error) {
    console.log("Controlled plan client errors", await browser("errors"));
    console.log(
      "Controlled speech-plan page:",
      await evaluate("document.body.innerText"),
    );
    console.log(
      "Controlled speech-plan operations:",
      adminWrites.map((w) => w.operation),
    );
    throw error;
  } finally {
    accounts = false;
    operatorAccount = false;
    await browser("cookies", "clear");
    speechPlans = [];
    speechLostReply = false;
  }
});

test("course clip batch stops on lost receipt and retries only the same immutable attempt", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  speechClips = [];
  clipLostReply = true;
  const id = "f".repeat(32),
    voice = {
      characterId: lesson.cast[0].characterId,
      characterRevision: 1,
      voiceRevision: 1,
    };
  speechPlans = [
    {
      id,
      lessonId: lesson.id,
      lessonRevision: 1,
      sourceHash: "b".repeat(64),
      planHash: "a".repeat(64),
      requestCount: 2,
      totalRequestCharacters: 20,
      selection: { voices: [voice], knowledgeNarrator: voice, emotions: {} },
      voices: speechVoices,
      createdAt: "2026-10-07T00:00:00Z",
      targets: [
        {
          pointer: "/blocks/1/turns/0",
          entryId: "qa0",
          text: "Bonjour !",
          voice,
          emotion: "Calm",
          generationKey: "c".repeat(64),
          wordCount: 1,
        },
        {
          pointer: "/knowledge/vocabulary/0/lemma",
          entryId: "qa1",
          text: "une baguette",
          voice,
          emotion: "Calm",
          generationKey: "d".repeat(64),
          wordCount: 0,
        },
      ],
    },
  ];
  async function act(role, name, action = "click", value) {
    const snap = await browser("snapshot", "-i"),
      ref = Object.entries(snap.refs).find(
        ([, r]) => r.role === role && r.name === name,
      )?.[0];
    assert.ok(ref, name);
    await browser(action, "@" + ref, ...(value === undefined ? [] : [value]));
  }
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/admin/speech-clips?planId=" + id);
    await browser("wait", "--text", "生成未完成片段（2）");
    assert.equal(adminWrites.length, 0);
    await act("button", "生成未完成片段（2）", "focus");
    await browser("press", "Enter");
    await browser("wait", "--text", "确认生成");
    await assertNamedModal("确认生成");
    await act("textbox", "操作理由", "fill", "Controlled generation");
    await act("checkbox", "我确认本次合成可能收费", "check");
    await act("button", "确认");
    await browser("wait", "--text", "核对同一请求");
    assert.equal(speechClips.length, 1);
    assert.equal(adminWrites.length, 1);
    assert.equal(
      await evaluate("document.querySelector('dialog fieldset').disabled"),
      true,
    );
    await act("button", "核对同一请求");
    await browser("wait", "--text", "本次片段已生成");
    assert.equal(adminWrites.length, 2);
    assert.deepEqual(adminWrites[0], adminWrites[1]);
    assert.equal(speechClips.length, 1);
    await act("button", "生成未完成片段（1）");
    await act("textbox", "操作理由", "fill", "Controlled remaining clip");
    await act("checkbox", "我确认本次合成可能收费", "check");
    await act("button", "确认");
    await browser("wait", "--text", "生成未完成片段（0）");
    assert.equal(speechClips.length, 2);
    assert.equal(adminWrites.length, 3);
    assert.equal(
      await evaluate("!!document.querySelector('a[download]')"),
      false,
    );
    for (const clip of speechClips) clip.accepted = true;
    await browser("reload");
    await browser("wait", "--text", "下载已审听音频与配音清单");
    assert.equal(
      await evaluate(
        "document.querySelector('a[download]').getAttribute('href')",
      ),
      `/api/v1/operator/speech-plans/${id}/export`,
    );
    speechClips[0].accepted = null;
    speechClips[0].status = "unknown";
    await browser("reload");
    await browser("wait", "--text", "结果未确认");
    await act("button", "核对后重新生成");
    await act("textbox", "操作理由", "fill", "Controlled unknown risk");
    await act("checkbox", "我确认本次合成可能收费", "check");
    await act("button", "确认");
    await browser(
      "wait",
      "--text",
      "请先核对原任务，并确认再次合成可能重复收费。",
    );
    assert.equal(adminWrites.length, 3);
    assert.equal(
      await evaluate("document.querySelector('dialog fieldset').disabled"),
      false,
    );
    await act("button", "关闭并核对任务");
    for (const width of [320, 390, 678]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    assert.equal(serverErrors.length, 0);
    assert.deepEqual((await browser("errors")).errors, []);
  } finally {
    accounts = false;
    operatorAccount = false;
    speechPlans = [];
    speechClips = [];
    clipLostReply = false;
    await browser("cookies", "clear");
  }
});

test("speech alignment imports and human corrections preserve exact requests after lost receipts", async () => {
  accounts = true;
  operatorAccount = true;
  adminWrites = [];
  const planId = "f".repeat(32);
  const voice = {
    characterId: lesson.cast[0].characterId,
    characterRevision: 1,
    voiceRevision: 1,
  };
  speechPlans = [
    {
      id: planId,
      lessonId: lesson.id,
      lessonRevision: 1,
      planHash: "a".repeat(64),
      sourceHash: "b".repeat(64),
      targets: [],
      voices: [],
      selection: { voices: [voice], knowledgeNarrator: voice, emotions: {} },
      requestCount: 1,
      totalRequestCharacters: 7,
      createdAt: null,
    },
  ];
  alignmentResult = {
    id: "e".repeat(32),
    planId,
    planHash: "a".repeat(64),
    reportHash: "b".repeat(64),
    createdAt: "2026-10-07T00:00:00Z",
    clips: [
      {
        clipId: "c".repeat(32),
        generationKey: "d".repeat(64),
        text: "Bonjour !",
        durationMs: 4000,
        issues: ["invalidTimeRange"],
        words: [
          { text: "Bonjour", start: 0, end: 7, startMs: null, endMs: null },
        ],
        accepted: null,
      },
    ],
  };
  alignmentLostImport = true;
  alignmentLostReview = true;
  async function act(role, name, action = "click", value) {
    const snap = await browser("snapshot", "-i");
    const ref = Object.entries(snap.refs).find(
      ([, r]) => r.role === role && r.name === name,
    )?.[0];
    assert.ok(ref, name);
    await browser(action, "@" + ref, ...(value === undefined ? [] : [value]));
  }
  const input = resolve(".local/qa/alignment-browser-input.json");
  await mkdir(dirname(input), { recursive: true });
  await writeFile(
    input,
    JSON.stringify({
      test: "Controlled prediction file; real schema tested in PostgreSQL",
    }),
  );
  try {
    await browser("open", origin + "/");
    await browser("cookies", "set", "brioche.sid", "shell-a");
    await browser("set", "viewport", "390", "844");
    await browser("open", origin + "/admin/speech-alignments?planId=" + planId);
    await browser("wait", "--text", "导入预测");
    await act("textbox", "导入理由", "fill", "Controlled import");
    const snap = await browser("snapshot", "-i");
    const fileRef = Object.entries(snap.refs).find(
      ([, r]) => r.name === "对齐结果 JSON",
    )?.[0];
    assert.ok(fileRef);
    await browser("upload", "@" + fileRef, input);
    await act("button", "导入预测");
    await browser("wait", "--text", "核对同一导入请求");
    await act("button", "核对同一导入请求");
    await browser("wait", "--text", "逐片段核对");
    assert.deepEqual(adminWrites[0], adminWrites[1]);
    await act("textbox", "核对理由", "fill", "Controlled correction");
    await act("checkbox", "我已实际试听这个片段", "check");
    await act("checkbox", "我已核对逐词时间", "check");
    await act("button", "确认时间轴");
    await browser(
      "wait",
      "--text",
      "请填写完整、依次排列且不超出录音的单词时间。",
    );
    assert.equal(adminWrites.length, 2);
    await act("spinbutton", "Bonjour 起点（毫秒）", "fill", "80");
    await act("spinbutton", "Bonjour 终点（毫秒）", "fill", "600");
    await act("button", "确认时间轴");
    await browser("wait", "--text", "核对同一审核请求");
    await act("button", "核对同一审核请求");
    await browser("wait", "--text", "时间轴已核对通过");
    assert.deepEqual(adminWrites[2], adminWrites[3]);
    assert.equal(adminWrites[2].words[0].startMs, 80);
    await evaluate(
      "window.__timelineAudio = []; window.__originalAudio = window.Audio; window.Audio = class extends window.__originalAudio { constructor(...args) { super(...args); window.__timelineAudio.push(this); } }",
    );
    await act("button", "试听整句");
    await browser(
      "wait",
      "--fn",
      "parseFloat(document.querySelector('.recording-preview-track > span')?.style.width)>0",
    );
    await act("button", "Bonjour");
    await browser(
      "wait",
      "--fn",
      "window.__timelineAudio.at(-1).paused && window.__timelineAudio.at(-1).currentTime >= 0.6",
    );
    assert.ok(
      await evaluate("window.__timelineAudio.at(-1).currentTime < 0.9"),
    );
    await evaluate("window.Audio = window.__originalAudio");
    await act(
      "textbox",
      "使用授权依据",
      "fill",
      "LicenseRef-ControlledFixture",
    );
    await act("textbox", "创作或授权主体", "fill", "Synthetic test author");
    await act("textbox", "组装说明", "fill", "Controlled assembly");
    await act("button", "下载录音课包");
    await browser(
      "wait",
      "--text",
      "请核对新版本、停顿、来源授权及填写的信息。",
    );
    assert.equal(adminWrites.length, 4);
    await act("checkbox", "我确认录音及角色声音可按以上授权使用。", "check");
    await act("button", "下载录音课包");
    await browser("wait", "--text", "录音课包已下载。");
    assert.equal(adminWrites.length, 5);
    assert.equal(
      adminWrites[4].operation,
      `/api/v1/operator/speech-alignments/${alignmentResult.id}/package`,
    );
    assert.equal(adminWrites[4].expectedReportHash, alignmentResult.reportHash);
    assert.equal(adminWrites[4].lessonRevision, 2);
    assert.equal(adminWrites[4].gapMs, 250);
    assert.equal(adminWrites[4].rightsConfirmed, true);
    assert.equal(adminWrites[4].reason, "Controlled assembly");
    packageLostReply = true;
    await act("button", "登记录音并导入草稿");
    await browser("wait", "--text", "核对同一课包导入请求");
    assert.equal(adminWrites.length, 6);
    assert.equal(
      await evaluate("document.querySelector('input[type=number]').disabled"),
      true,
    );
    await act("button", "核对同一课包导入请求");
    await browser("wait", "--text", "已登记录音并导入课程 v2 草稿");
    assert.equal(adminWrites.length, 7);
    assert.deepEqual(adminWrites[5], adminWrites[6]);
    assert.equal(
      adminWrites[5].package.expectedReportHash,
      alignmentResult.reportHash,
    );
    assert.equal(packageReceipts.length, 1);
    await browser("wait", "--text", "3 条录音");
    assert.equal(
      await evaluate(
        "Array.from(document.querySelectorAll('a')).find(a=>a.textContent==='预览新草稿')?.getAttribute('href')",
      ),
      `/author-preview?lessonId=${lesson.id}&revision=2`,
    );
    for (const width of [320, 390, 678]) {
      await browser("set", "viewport", String(width), "844");
      assert.equal(
        await evaluate("document.documentElement.scrollWidth<=innerWidth"),
        true,
      );
    }
    assert.deepEqual((await browser("errors")).errors, []);
    assert.equal(serverErrors.length, 0);
  } finally {
    accounts = false;
    operatorAccount = false;
    speechPlans = [];
    alignmentResult = null;
    alignmentLostImport = false;
    alignmentLostReview = false;
    packageReceipts = [];
    packageLostReply = false;
    await browser("cookies", "clear");
  }
});
