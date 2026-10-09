import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile, unlink, rmdir } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  queryVoice,
  listVoices,
  verifyClonedVoice,
  validateReference,
  run as voiceRun,
} from "../qwen-voices.ts";
import { synthesize, characterFor, sample, run } from "../qwen-tts.ts";

const env = {
  DASHSCOPE_API_KEY: "private-key",
  QWEN_WORKSPACE_ID: "qa-workspace",
};
const model = "qwen-audio-3.1-tts-flash";
const reference = {
  assetId: "QA_reference",
  revision: 2,
  transcript: "Bonjour !",
  cloningPermission: "Synthetic fixture permission.",
};
function clone(line = sample[0]) {
  const item = structuredClone(characterFor(line));
  Object.assign(item.profile, {
    voiceKind: "cloned",
    voiceId: `${model}-qa-123`,
    referenceAudio: { ...reference },
  });
  return item;
}
const details = (status = "OK", target = model) =>
  Response.json({
    request_id: "request-123",
    output: {
      status,
      target_model: target,
      resource_link: "https://private.example/reference?secret=do-not-save",
    },
  });

test("voice query uses the supported workspace with no redirects and strips reference URLs", async () => {
  const result = await queryVoice("clone-123", env, async (url, init) => {
    assert.equal(
      url,
      "https://qa-workspace.cn-beijing.maas.aliyuncs.com/api/v1/services/audio/tts/customization",
    );
    assert.equal(init.redirect, "error");
    assert.equal(init.headers.Authorization, "Bearer private-key");
    assert.deepEqual(JSON.parse(init.body), {
      model: "voice-enrollment",
      input: { action: "query_voice", voice_id: "clone-123" },
    });
    return details();
  });
  assert.equal(result.model, model);
  assert.equal(result.status, "OK");
  assert.equal(result.requestId, "request-123");
  assert.doesNotMatch(
    JSON.stringify(result),
    /secret|private-key|resource_link/,
  );
});

test("invalid reference authorization and voice IDs fail without network calls", async () => {
  let count = 0;
  const fetcher = async () => {
    count++;
    return details();
  };
  for (const bad of [
    null,
    { ...reference, revision: 0 },
    { ...reference, assetId: "../outside" },
    { ...reference, transcript: "" },
    { ...reference, cloningPermission: "" },
    { ...reference, cloningPermission: "x\nsecret" },
    { ...reference, transcript: "字".repeat(1400) },
  ]) {
    assert.throws(() => validateReference(bad), /角色复刻/);
    await assert.rejects(
      verifyClonedVoice(
        { ...clone().profile, referenceAudio: bad },
        env,
        fetcher,
      ),
      /角色复刻/,
    );
  }
  await assert.rejects(queryVoice("https://private-key", env, fetcher), /ID/);
  assert.equal(count, 0);
});

test("processing, unavailable, unexpected status and model mismatch prevent synthesis POST", async () => {
  for (const [status, target] of [
    ["DEPLOYING", model],
    ["UNDEPLOYED", model],
    ["UNKNOWN", model],
    ["OK", "qwen-audio-3.1-tts-next"],
  ]) {
    let count = 0;
    await assert.rejects(
      synthesize(
        sample[0],
        env,
        async (_url, init) => {
          count++;
          assert.equal(JSON.parse(init.body).input.action, "query_voice");
          return details(status, target);
        },
        { items: [clone()] },
      ),
      /提供方音色/,
    );
    assert.equal(count, 1);
  }
});

test("available cloned voice generates with the verified model and retains only safe verification metadata", async () => {
  const wav = Buffer.alloc(44);
  wav.write("RIFF");
  wav.writeUInt32LE(36, 4);
  wav.write("WAVE", 8);
  let count = 0;
  const result = await synthesize(
    sample[0],
    env,
    async (url, init) => {
      count++;
      if (count === 1) return details();
      if (count === 2) {
        assert.match(url, /SpeechSynthesizer$/);
        const body = JSON.parse(init.body);
        assert.equal(body.model, model);
        assert.equal(body.input.voice, clone().profile.voiceId);
        return Response.json({
          output: {
            finish_reason: "stop",
            audio: {
              url: "https://dashscope-result-bj.oss-cn-beijing.aliyuncs.com/qa.wav?Signature=private",
            },
          },
        });
      }
      assert.equal(init.headers, undefined);
      return new Response(wav);
    },
    { items: [clone()] },
  );
  assert.equal(count, 3);
  assert.equal(result.voiceVerification.status, "OK");
  assert.equal(result.voiceVerification.model, result.parameters.model);
  assert.doesNotMatch(
    JSON.stringify(result),
    /private-key|Signature|resource_link|do-not-save/,
  );
});

test("all role clones are preflighted before any paid synthesis or output directory", async () => {
  const dir = await mkdtemp(join(tmpdir(), "brioche-clone-test-"));
  const file = join(dir, "profiles.json");
  try {
    const library = { items: [clone(sample[0]), clone(sample[1])] };
    await writeFile(file, JSON.stringify(library));
    let count = 0;
    await assert.rejects(
      run(["--generate", "--profiles", file], env, async (_url, init) => {
        count++;
        assert.equal(JSON.parse(init.body).input.action, "query_voice");
        return details(count === 1 ? "OK" : "UNDEPLOYED");
      }),
      /不可用/,
    );
    assert.equal(count, 2);
    assert.match(
      await run(["--plan", "--profiles", file], {}, () =>
        assert.fail("offline plan"),
      ),
      /未请求 API/,
    );
  } finally {
    await unlink(file);
    await rmdir(dir);
  }
});

test("list has bounded explicit pages and never treats listing as proof of model binding", async () => {
  const result = await listVoices(
    { prefix: "Camille", pageIndex: 1, pageSize: 2 },
    env,
    async (_url, init) => {
      assert.deepEqual(JSON.parse(init.body).input, {
        action: "list_voice",
        prefix: "Camille",
        page_index: 1,
        page_size: 2,
      });
      return Response.json({
        request_id: "list-123",
        output: {
          voice_list: [
            { voice_id: "clone-a", status: "OK", resource_link: "private-url" },
            { voice_id: "clone-b", status: "DEPLOYING" },
          ],
        },
      });
    },
  );
  assert.equal(result.nextPage, 2);
  assert.deepEqual(result.items[0], { voiceId: "clone-a", status: "OK" });
  assert.doesNotMatch(JSON.stringify(result), /private-url|target_model/);
  await assert.rejects(
    listVoices({ pageIndex: -1 }, env, () => assert.fail()),
    /参数/,
  );
  await assert.rejects(
    listVoices({ pageSize: 21 }, env, () => assert.fail()),
    /参数/,
  );
  await assert.rejects(voiceRun(["--list", "-1"], env), /用法/);
});

test("query transport, provider errors and malformed or excessive responses are bounded and sanitized without retries", async () => {
  for (const fetcher of [
    async () => {
      throw Error("private-key?secret=reference");
    },
    async () => new Response("private-key", { status: 401 }),
    async () =>
      Response.json({
        code: "InvalidParameter",
        message: "private-key",
        request_id: "r",
        output: {},
      }),
    async () => new Response("{not-json"),
    async () => new Response("x", { headers: { "Content-Length": "256001" } }),
    async () => new Response("x".repeat(256001)),
    async () =>
      new Response(
        new ReadableStream({
          start(c) {
            c.error(Error("private-key"));
          },
        }),
      ),
  ]) {
    let count = 0;
    await assert.rejects(
      queryVoice("clone-123", env, async () => {
        count++;
        return fetcher();
      }),
      (error) =>
        error instanceof Error && !/private-key|secret=reference/.test(error.message) &&
        /提供方/.test(error.message),
    );
    assert.equal(count, 1);
  }
  for (const bad of [
    null,
    {},
    { request_id: "https://private-key", output: {} },
    { request_id: "r", output: { status: "OK" } },
  ])
    await assert.rejects(
      queryVoice("clone-123", env, async () => Response.json(bad)),
      /提供方/,
    );
});
