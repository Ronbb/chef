import { createHash } from "node:crypto";
import { mkdir, writeFile, open } from "node:fs/promises";
import defaultLibrary from "../docs/characters/voices.json" with { type: "json" };
import { resolve, relative, isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";
import { qwenBase } from "./qwen-api.mjs";
import { validateReference, verifyClonedVoice } from "./qwen-voices.mjs";

const roleCharacters = {
  customer: "character-camille",
  shopkeeper: "character-luc",
};
export function characterFor(line, library = defaultLibrary) {
  if (!Array.isArray(library?.items) || library.items.length > 100)
    throw Error("角色库格式无效。");
  const candidates = library.items.filter(
    (item) =>
      item.character?.characterId === roleCharacters[line.role] &&
      item.character?.revision === 1,
  );
  if (candidates.length !== 1)
    throw Error("角色库缺少固定角色版本或存在重复声音档案。");
  const item = candidates[0],
    p = item.profile;
  if (
    !Number.isInteger(item.voiceRevision) ||
    item.voiceRevision < 1 ||
    !p ||
    p.provider !== "qwen" ||
    p.model !== "qwen-audio-3.1-tts-flash" ||
    p.locale !== "fr-FR" ||
    !["system", "cloned"].includes(p.voiceKind) ||
    !/^[a-zA-Z0-9_.-]{1,200}$/.test(p.voiceId ?? "") ||
    !Number.isFinite(p.rate) ||
    p.rate < 0.5 ||
    p.rate > 2
  )
    throw Error("角色声音档案尚未适配：当前生成器只支持 Qwen Flash 法语音色。");
  if (p.voiceKind === "cloned") validateReference(p.referenceAudio);
  for (const key of ["personality", "speakingStyle", "defaultEmotion"])
    if (
      typeof p[key] !== "string" ||
      !p[key].trim() ||
      Buffer.byteLength(p[key]) > 2000 ||
      /[\u0000-\u001f\u007f]/.test(p[key])
    )
      throw Error("角色声音指令无效。");
  return item;
}
export const sample = [
  {
    id: "01",
    role: "customer",
    text: "Bonjour !",
    emotion: "Friendly greeting, gently bright and polite.",
  },
  {
    id: "02",
    role: "shopkeeper",
    text: "Bonjour !",
    emotion: "Warm welcoming reply from a friendly bakery shopkeeper.",
  },
  {
    id: "03",
    role: "customer",
    text: "Je voudrais une baguette, s’il vous plaît.",
    emotion: "A polite request, slightly expectant, relaxed and natural.",
  },
  {
    id: "04",
    role: "shopkeeper",
    text: "Voilà !",
    emotion: "Warm and upbeat, handing over the bread.",
  },
  {
    id: "05",
    role: "customer",
    text: "Merci. C’est combien ?",
    emotion: "Grateful, followed by a natural curious question.",
  },
  {
    id: "06",
    role: "shopkeeper",
    text: "Un euro vingt, s’il vous plaît.",
    emotion: "Matter-of-fact price, warm and courteous.",
  },
  {
    id: "07",
    role: "customer",
    text: "Merci, au revoir !",
    emotion: "A pleased, friendly farewell.",
  },
  {
    id: "08",
    role: "shopkeeper",
    text: "Au revoir !",
    emotion: "Warm farewell with a natural falling cadence.",
  },
];

export function requestFor(line, env, library = defaultLibrary) {
  const character = characterFor(line, library);
  const profile = character.profile;
  const base = qwenBase(env);
  if (
    !line ||
    !/^[a-zA-Z0-9_-]{1,40}$/.test(line.id ?? "") ||
    !roleCharacters[line.role] ||
    typeof line.text !== "string" ||
    !line.text.trim() ||
    [...line.text].length > 600 ||
    typeof line.emotion !== "string" ||
    line.emotion.length > 1000
  ) {
    throw new Error("试听台词无效。");
  }
  return {
    endpoint: `${base}/api/v1/services/audio/tts/SpeechSynthesizer`,
    body: {
      model: profile.model,
      input: {
        text: line.text,
        voice: profile.voiceId,
        format: "wav",
        sample_rate: 24000,
        language_hints: ["fr"],
        rate: profile.rate,
        seed: 0,
        enable_aigc_tag: true,
        instruction: `Speak only the supplied French text, with clear natural French pronunciation for an A1 learner. Do not add words or read these instructions. Character: ${profile.personality} Speaking style: ${profile.speakingStyle} Default emotion: ${profile.defaultEmotion} Scene emotion: ${line.emotion}`,
      },
    },
  };
}

// Only the provider's documented result bucket; signed URLs never enter logs or manifests.
export function audioUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("提供方音频地址无效。");
  }
  if (
    !["http:", "https:"].includes(url.protocol) ||
    url.username ||
    url.password ||
    url.port ||
    url.hostname !== "dashscope-result-bj.oss-cn-beijing.aliyuncs.com"
  ) {
    throw new Error("提供方返回了未允许的音频地址。");
  }
  url.protocol = "https:";
  return url;
}

async function boundedBytes(response, limit) {
  if (!response.ok)
    throw new Error(`语音服务请求失败（HTTP ${response.status}）。`);
  if (!response.body || Number(response.headers.get("content-length")) > limit)
    throw new Error("语音响应超出限额。");
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > limit) throw new Error("语音响应超出限额。");
      chunks.push(value);
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
  return Buffer.concat(chunks, size);
}

export async function synthesize(
  line,
  env,
  fetcher = fetch,
  library = defaultLibrary,
) {
  const request = requestFor(line, env, library);
  const profile = characterFor(line, library).profile;
  const voiceVerification =
    profile.voiceKind === "cloned"
      ? await verifyClonedVoice(profile, env, fetcher)
      : null;
  let response;
  try {
    response = await fetcher(request.endpoint, {
      method: "POST",
      redirect: "error",
      signal: AbortSignal.timeout(120_000),
      headers: {
        Authorization: `Bearer ${env.DASHSCOPE_API_KEY}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(request.body),
    });
  } catch {
    throw new Error("生成连接失败或超时；未自动重试，请先核对提供方计费记录。");
  }
  let result;
  try {
    result = JSON.parse(
      (await boundedBytes(response, 256_000)).toString("utf8"),
    );
  } catch (error) {
    if (error instanceof SyntaxError) throw new Error("提供方响应格式无效。");
    throw error;
  }
  if (result.output?.finish_reason !== "stop" || result.code)
    throw new Error("提供方未返回完成的音频。");
  const url = audioUrl(result.output?.audio?.url);
  let downloaded;
  try {
    downloaded = await fetcher(url, {
      redirect: "error",
      signal: AbortSignal.timeout(60_000),
    });
  } catch {
    throw new Error("生成已完成，但音频下载失败；请核对记录后再重试。");
  }
  const providerWav = await boundedBytes(downloaded, 16 * 1024 * 1024);
  const wav = normalizeWav(providerWav);
  if (
    wav.length < 44 ||
    wav.toString("ascii", 0, 4) !== "RIFF" ||
    wav.toString("ascii", 8, 12) !== "WAVE"
  ) {
    throw new Error("提供方返回的文件不是 WAV，未保存。");
  }
  return {
    wav,
    providerWav,
    parameters: request.body,
    voiceVerification,
    usage: {
      inputTokens: Number.isSafeInteger(result.usage?.input_tokens)
        ? result.usage.input_tokens
        : null,
      outputTokens: Number.isSafeInteger(result.usage?.output_tokens)
        ? result.usage.output_tokens
        : null,
    },
  };
}

// Qwen may return a stream-style RIFF/data length. Preserve all metadata (including AIGC)
// and PCM bytes, repairing only those two lengths for the bounded downloaded file.
export function normalizeWav(source) {
  if (
    source.length < 44 ||
    source.toString("ascii", 0, 4) !== "RIFF" ||
    source.toString("ascii", 8, 12) !== "WAVE"
  )
    throw new Error("提供方返回的文件不是 WAV，未保存。");
  const output = Buffer.from(source);
  if (source.readUInt32LE(4) === source.length - 8) return output;
  if (source.readUInt32LE(4) !== 2147483583)
    throw new Error("提供方 WAV 长度字段不受支持。");
  let format = false;
  for (let offset = 12; offset + 8 <= source.length;) {
    const type = source.toString("ascii", offset, offset + 4);
    const length = source.readUInt32LE(offset + 4);
    const remaining = source.length - offset - 8;
    if (type === "fmt " && length === 16 && remaining >= 16) {
      format =
        source.readUInt16LE(offset + 8) === 1 &&
        source.readUInt16LE(offset + 10) === 1 &&
        source.readUInt32LE(offset + 12) === 24000 &&
        source.readUInt16LE(offset + 20) === 2 &&
        source.readUInt16LE(offset + 22) === 16;
    }
    if (
      type === "data" &&
      length > remaining &&
      format &&
      remaining > 0 &&
      remaining % 2 === 0
    ) {
      output.writeUInt32LE(source.length - 8, 4);
      output.writeUInt32LE(remaining, offset + 4);
      return output;
    }
    if (length > remaining) break;
    offset += 8 + length + (length % 2);
  }
  throw new Error("提供方 WAV 数据长度无法安全修正。");
}

export async function run(args, env = process.env, fetcher = fetch) {
  if (
    ![1, 3].includes(args.length) ||
    !["--plan", "--generate"].includes(args[0]) ||
    (args.length === 3 && args[1] !== "--profiles")
  )
    throw new Error(
      "用法：qwen-tts.mjs --plan 或 --generate [--profiles <角色档案.json>]",
    );
  let library = defaultLibrary;
  if (args.length === 3) {
    const file = await open(args[2], "r");
    try {
      if ((await file.stat()).size > 1024 * 1024)
        throw Error("角色库文件过大。");
      library = JSON.parse(await file.readFile("utf8"));
    } finally {
      await file.close();
    }
  }
  // Validate every required role before any paid request or output file creation.
  for (const line of sample) characterFor(line, library);
  // Validate credentials before creating any output or issuing paid requests.
  if (args[0] === "--generate") {
    requestFor(sample[0], env, library);
    // Check all required cloned voices before any synthesis can incur a charge.
    const checked = new Set();
    for (const line of sample) {
      const character = characterFor(line, library);
      const key = `${character.character.characterId}:${character.voiceRevision}`;
      if (!checked.has(key) && character.profile.voiceKind === "cloned")
        await verifyClonedVoice(character.profile, env, fetcher);
      checked.add(key);
    }
  }
  const privateRoot = resolve(".local/private/tts-qwen");
  const output = resolve(privateRoot, `bakery-${Date.now()}`);
  const within = relative(privateRoot, output);
  if (within.startsWith("..") || isAbsolute(within))
    throw new Error("输出路径无效。");
  if (args[0] === "--plan")
    return `Qwen Audio 3.1：${sample.length} 句、${sample.reduce((n, line) => n + [...line.text].length, 0)} 个原文字符，2 个法语角色声音档案。仅计划，未请求 API，复刻音色可用性尚未查询。`;
  await mkdir(output, { recursive: true });
  // Per-line receipts survive a later failure. New runs are explicit and bill again.
  for (const line of sample) {
    const character = characterFor(line, library);
    const result = await synthesize(line, env, fetcher, library);
    const receipt = {
      status: "unreviewed",
      provider: "qwen",
      modelVersion: "provider-alias-not-immutable",
      generatedAt: new Date().toISOString(),
      role: line.role,
      characterId: character.character.characterId,
      characterRevision: character.character.revision,
      voiceRevision: character.voiceRevision,
      characterProfileSha256: createHash("sha256")
        .update(JSON.stringify(character))
        .digest("hex"),
      id: line.id,
      parameters: result.parameters,
      usage: result.usage,
      voiceVerification: result.voiceVerification,
      inputSha256: createHash("sha256")
        .update(JSON.stringify(result.parameters))
        .digest("hex"),
      audioSha256: createHash("sha256").update(result.wav).digest("hex"),
      providerAudioSha256: createHash("sha256")
        .update(result.providerWav)
        .digest("hex"),
      postprocessing: "qwen-riff-length-v1-metadata-preserved",
      bytes: result.wav.length,
    };
    await writeFile(resolve(output, `${line.id}.wav`), result.wav, {
      flag: "wx",
    });
    await writeFile(
      resolve(output, `${line.id}.provider.wav`),
      result.providerWav,
      { flag: "wx" },
    );
    await writeFile(
      resolve(output, `${line.id}.json`),
      JSON.stringify(receipt, null, 2),
      { flag: "wx" },
    );
  }
  return `试听已保存：${output}。尚未审听、登记或发布。重跑会再次计费。`;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  try {
    console.log(await run(process.argv.slice(2)));
  } catch (error) {
    // Network and filesystem exceptions can contain signed URLs or environment data.
    const safe =
      /^(角色|请在 |试听台词|提供方|语音服务|语音响应|生成连接|生成已完成|用法：|输出路径)/;
    console.error(
      safe.test(error.message)
        ? error.message
        : "试听生成失败；请核对本机配置与已生成文件，未自动重试。",
    );
    process.exitCode = 1;
  }
}
