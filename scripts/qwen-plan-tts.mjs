// Private, explicit paid generation from an offline Chef speech plan. Never publishes.
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve, relative, isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";
import { qwenBase } from "./qwen-api.mjs";
import { synthesizeRequest } from "./qwen-tts.mjs";
import { QWEN_FRENCH_SYSTEM_VOICES as multilingualVoices } from "../packages/contracts/src/generated/tts-voices.ts";

const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
// Serde emits f64 rates such as 1.0; JSON.parse loses that distinction.
// Preserve the compiler's numeric representation when checking immutable hashes.
export function rustJson(value, field = "") {
  if (Array.isArray(value))
    return `[${value.map((v) => rustJson(v)).join(",")}]`;
  if (value && typeof value === "object")
    return `{${Object.entries(value)
      .map(([k, v]) => `${JSON.stringify(k)}:${rustJson(v, k)}`)
      .join(",")}}`;
  if (field === "rate" && typeof value === "number" && Number.isInteger(value))
    return `${value}.0`;
  return JSON.stringify(value);
}
const hash = (value) => digest(rustJson(value));
const require = (condition, message) => {
  if (!condition) throw Error(message);
};
const bounded = (v, limit) =>
  typeof v === "string" &&
  v.trim() &&
  Buffer.byteLength(v) <= limit &&
  !/[\u0000-\u001f\u007f]/.test(v);
export function validatePlan(plan) {
  require(plan?.compilerVersion ===
    "speech-plan-2/author-scalar-1", "Expected native Chef speech plan.");
  require(/^[a-f0-9]{64}$/.test(plan.sourceHash) &&
    /^[a-f0-9]{64}$/.test(plan.planHash), "Invalid fixed source/plan hash.");
  require(hash({ ...plan, planHash: "" }) ===
    plan.planHash, "Plan hash mismatch; recompile source.");
  const requests = Object.entries(plan.requests ?? {});
  require(requests.length > 0 &&
    requests.length <= 1000 &&
    Array.isArray(plan.targets), "Invalid request coverage.");
  let characters = 0;
  for (const [key, request] of requests) {
    require(hash(request) === key &&
      request.compilerVersion ===
        plan.compilerVersion, "Fixed generation key mismatch.");
    const p = request.profile,
      body = request.parameters,
      input = body?.input;
    // First native pilot uses registered multilingual system voices. Cloned voice
    // tasks keep their existing authenticated generator and enrollment verification.
    require(p?.provider === "qwen" &&
      p.model === "qwen-audio-3.1-tts-flash" &&
      p.voiceKind === "system" &&
      ["fr-FR", "yue-Hant-HK"].includes(p.locale) &&
      p.referenceAudio === null &&
      multilingualVoices.some(
        ([voice]) => voice === p.voiceId,
      ), "Unsupported fixed voice profile.");
    require(body?.model === p.model &&
      input?.voice === p.voiceId &&
      input.format === "wav" &&
      input.sample_rate === 24000 &&
      input.rate === p.rate &&
      Number.isFinite(p.rate) &&
      p.rate >= 0.5 &&
      p.rate <= 2 &&
      input.seed === 0 &&
      input.enable_aigc_tag === true &&
      bounded(input.text, 2400) &&
      [...input.text].length <= 600 &&
      bounded(input.instruction, 10000), "Unsupported synthesis parameters.");
    require(p.locale === "fr-FR"
      ? JSON.stringify(input.language_hints) === '["fr"]'
      : !Object.hasOwn(input, "language_hints"), "Wrong language hints.");
    const targets = plan.targets.filter((t) => t.generationKey === key);
    require(targets.length > 0 &&
      targets.every(
        (t) => t.text === input.text && hash(t.voice) === hash(request.voice),
      ), "Missing or mismatched fixed target.");
    require(Array.isArray(request.wordUnits) &&
      request.wordUnits.length > 0 &&
      targets.every(
        (t) =>
          t.words.length === request.wordUnits.length &&
          t.words.every(
            (w, i) =>
              w.text === request.wordUnits[i].text &&
              w.entryStart === request.wordUnits[i].start &&
              w.entryEnd === request.wordUnits[i].end,
          ),
      ), "Mismatched authored alignment units.");
    const direction =
      p.locale === "fr-FR"
        ? "Speak only the supplied French text, with clear natural French pronunciation for an A1 learner. Do not add words or read these instructions."
        : "请用自然的香港粤语朗读提供的原文，保持粤语声调和口语节奏，适合粤语初学者。不要用普通话，不要翻译，不要添加内容或朗读这些指令。";
    require(targets.every(
      (t) =>
        bounded(t.emotion, 1000) &&
        input.instruction ===
          `${direction} Character: ${p.personality} Speaking style: ${p.speakingStyle} Default emotion: ${p.defaultEmotion} Scene emotion: ${t.emotion}`,
    ), "Unbound voice directions.");
    require([p.personality, p.speakingStyle, p.defaultEmotion].every((v) =>
      bounded(v, 2000),
    ), "Invalid character directions.");
    characters += [...input.text].length;
  }
  require(characters === plan.totalRequestCharacters &&
    plan.targets.every((t) =>
      Object.hasOwn(plan.requests, t.generationKey),
    ), "Wrong character budget or target coverage.");
  return requests;
}
export async function run(args, env = process.env, fetcher = fetch) {
  require(args.length === 5 &&
    args[0] === "--plan" &&
    args[2] === "--output" &&
    args[4] ===
      "--confirm-cost", "Usage: --plan <compiled-plan.json> --output <new-private-directory> --confirm-cost");
  const bytes = await readFile(args[1]);
  require(bytes.length <= 4 * 1024 * 1024, "Plan exceeds size limit.");
  const plan = JSON.parse(bytes.toString("utf8")),
    requests = validatePlan(plan);
  const endpoint = `${qwenBase(env)}/api/v1/services/audio/tts/SpeechSynthesizer`;
  const root = resolve(".local/private"),
    output = resolve(args[3]),
    within = relative(root, output);
  require(within &&
    !within.startsWith("..") &&
    !isAbsolute(within), "Output must be a new private subdirectory.");
  await mkdir(root, { recursive: true });
  await mkdir(output); // Existing attempts are never silently retried or overwritten.
  await writeFile(resolve(output, "plan.json"), bytes, {
    flag: "wx",
    mode: 0o600,
  });
  for (const [key, request] of requests) {
    const attempt = {
      generationKey: key,
      planHash: plan.planHash,
      startedAt: new Date().toISOString(),
      status: "attempted",
    };
    const attemptPath = resolve(output, `${key}.attempt.json`);
    await writeFile(attemptPath, JSON.stringify(attempt), {
      flag: "wx",
      mode: 0o600,
    });
    const result = await synthesizeRequest(
      { endpoint, body: request.parameters },
      request.profile,
      env,
      fetcher,
    );
    await writeFile(
      resolve(output, `${key}.provider.wav`),
      result.providerWav,
      { flag: "wx", mode: 0o600 },
    );
    await writeFile(resolve(output, `${key}.wav`), result.wav, {
      flag: "wx",
      mode: 0o600,
    });
    await writeFile(
      resolve(output, `${key}.receipt.json`),
      JSON.stringify({
        ...attempt,
        status: "generated",
        usage: result.usage,
        parameters: result.parameters,
        providerSha256: digest(result.providerWav),
        sha256: digest(result.wav),
        postprocessing: "qwen-riff-length-v1-metadata-preserved",
        humanListeningAsserted: false,
      }),
      { flag: "wx", mode: 0o600 },
    );
  }
  return `Generated ${requests.length} fixed requests (${plan.totalRequestCharacters} source characters). Private output only; not registered or published.`;
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  run(process.argv.slice(2))
    .then((message) => console.log(message))
    .catch((error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
}
