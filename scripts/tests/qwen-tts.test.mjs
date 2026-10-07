import test from "node:test";
import assert from "node:assert/strict";
import {
  audioUrl,
  requestFor,
  run,
  sample,
  synthesize,
  normalizeWav,
  characterFor,
} from "../qwen-tts.mjs";
const env = {
  DASHSCOPE_API_KEY: "private-test-key",
  QWEN_WORKSPACE_ID: "test-space",
};
test("generation locks character and voice revisions and refuses ambiguous or unsupported clones", () => {
  const a = structuredClone(characterFor(sample[0]));
  assert.equal(a.character.characterId, "character-camille");
  assert.equal(a.voiceRevision, 1);
  a.voiceRevision = 2;
  a.profile.rate = 0.85;
  a.profile.speakingStyle = "A reserved, quietly cheerful voice.";
  const library = { items: [a] };
  const request = requestFor(sample[0], env, library);
  assert.equal(request.body.input.rate, 0.85);
  assert.match(request.body.input.instruction, /quietly cheerful/);
  assert.throws(() => characterFor(sample[0], { items: [a, a] }), /重复/);
  a.character.revision = 2;
  assert.throws(() => characterFor(sample[0], library), /缺少/);
  a.character.revision = 1;
  a.profile.voiceKind = "cloned";
  assert.throws(() => requestFor(sample[0], env, library), /参考录音/);
});
const wav = Buffer.alloc(44);
wav.write("RIFF");
wav.write("WAVE", 8);
wav.writeUInt32LE(36, 4);

test("actual Qwen stream-length WAV is repaired without deleting AIGC or changing PCM", () => {
  const raw = Buffer.alloc(68);
  raw.write("RIFF");
  raw.writeUInt32LE(2147483583, 4);
  raw.write("WAVE", 8);
  raw.write("fmt ", 12);
  raw.writeUInt32LE(16, 16);
  raw.writeUInt16LE(1, 20);
  raw.writeUInt16LE(1, 22);
  raw.writeUInt32LE(24000, 24);
  raw.writeUInt32LE(48000, 28);
  raw.writeUInt16LE(2, 32);
  raw.writeUInt16LE(16, 34);
  raw.write("AIGC", 36);
  raw.writeUInt32LE(4, 40);
  raw.write("tag!", 44);
  raw.write("data", 48);
  raw.writeUInt32LE(2147483315, 52);
  raw.fill(42, 56);
  const fixed = normalizeWav(raw);
  assert.equal(fixed.readUInt32LE(4), 60);
  assert.equal(fixed.readUInt32LE(52), 12);
  assert.deepEqual(fixed.subarray(8, 52), raw.subarray(8, 52));
  assert.deepEqual(fixed.subarray(56), raw.subarray(56));
  assert.equal(raw.readUInt32LE(4), 2147483583);
  assert.throws(() => normalizeWav(raw.subarray(0, 67)), /安全修正/);
});
const success = () =>
  Response.json({
    output: {
      finish_reason: "stop",
      audio: {
        url: "http://dashscope-result-bj.oss-cn-beijing.aliyuncs.com/test.wav?Signature=private",
      },
    },
  });

test("private base URL supports a workspace root or /api/v1, without credential forwarding to arbitrary hosts", () => {
  for (const path of [
    "",
    "/",
    "/api/v1",
    "/api/v1/",
    "/compatible-mode/v1",
    "/compatible-mode/v1/",
  ]) {
    const request = requestFor(sample[0], {
      DASHSCOPE_API_KEY: "test",
      DASHSCOPE_BASE_URL: `https://workspace.cn-beijing.maas.aliyuncs.com${path}`,
    });
    assert.equal(
      request.endpoint,
      "https://workspace.cn-beijing.maas.aliyuncs.com/api/v1/services/audio/tts/SpeechSynthesizer",
    );
  }
  for (const url of [
    "http://workspace.cn-beijing.maas.aliyuncs.com",
    "https://evil.test",
    "https://dashscope-intl.aliyuncs.com/api/v1",
    "https://workspace.cn-beijing.maas.aliyuncs.com/api/v1?key=private",
  ]) {
    assert.throws(() =>
      requestFor(sample[0], {
        DASHSCOPE_API_KEY: "test",
        DASHSCOPE_BASE_URL: url,
      }),
    );
  }
});

test("French role voices, emotion, and AI provenance use the documented new API", () => {
  const a = requestFor(sample[0], env);
  const b = requestFor(sample[1], env);
  assert.match(
    a.endpoint,
    /^https:\/\/test-space\.cn-beijing\.maas\.aliyuncs\.com\//,
  );
  assert.deepEqual(a.body.input.language_hints, ["fr"]);
  assert.notEqual(a.body.input.voice, b.body.input.voice);
  assert.match(a.body.input.instruction, /Friendly greeting/);
  assert.equal(a.body.input.enable_aigc_tag, true);
  assert.equal(a.body.input.instructions, undefined);
});
test("plan is offline; generation fails before I/O without credentials", async () => {
  assert.match(await run(["--plan"], {}), /未请求 API/);
  await assert.rejects(run(["--generate"], {}), /DASHSCOPE_API_KEY/);
  assert.throws(() =>
    requestFor(sample[0], { ...env, QWEN_WORKSPACE_ID: "../evil" }),
  );
});
test("signed result URLs are upgraded to TLS and cannot redirect secrets elsewhere", () => {
  assert.equal(
    audioUrl("http://dashscope-result-bj.oss-cn-beijing.aliyuncs.com/a")
      .protocol,
    "https:",
  );
  for (const url of [
    "http://127.0.0.1/a",
    "https://dashscope-result-bj.oss-cn-beijing.aliyuncs.com.evil/a",
    "https://user:pass@dashscope-result-bj.oss-cn-beijing.aliyuncs.com/a",
  ])
    assert.throws(() => audioUrl(url));
});
test("API auth is not sent to audio bucket; receipts contain neither key nor signed URL", async () => {
  const calls = [];
  const result = await synthesize(sample[0], env, async (url, init) => {
    calls.push({ url: String(url), init });
    return calls.length === 1 ? success() : new Response(wav);
  });
  assert.equal(calls.length, 2);
  assert.equal(calls[0].init.headers.Authorization, "Bearer private-test-key");
  assert.equal(calls[1].init.headers, undefined);
  assert.equal(calls[1].init.redirect, "error");
  assert.doesNotMatch(JSON.stringify(result), /private-test-key|Signature/);
});
test("paid POST is never retried; raw provider failures are not printed", async () => {
  let calls = 0;
  await assert.rejects(
    synthesize(sample[0], env, async () => {
      calls++;
      throw new Error("private-test-key");
    }),
    /未自动重试/,
  );
  assert.equal(calls, 1);
  await assert.rejects(
    synthesize(
      sample[0],
      env,
      async () => new Response("private-test-key", { status: 401 }),
    ),
    /HTTP 401/,
  );
});
test("oversized API bodies and invalid audio are rejected", async () => {
  await assert.rejects(
    synthesize(sample[0], env, async () => new Response("x".repeat(256001))),
    /限额/,
  );
  let calls = 0;
  await assert.rejects(
    synthesize(sample[0], env, async () =>
      ++calls === 1 ? success() : new Response("not wav"),
    ),
    /不是 WAV/,
  );
});
