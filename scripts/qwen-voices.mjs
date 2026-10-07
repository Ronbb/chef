import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { qwenVoiceCall } from "./qwen-api.mjs";

const statuses = new Set(["DEPLOYING", "OK", "UNDEPLOYED"]);
function voiceId(value) {
  if (typeof value !== "string" || !/^[a-zA-Z0-9_.-]{1,200}$/.test(value))
    throw Error("角色复刻音色 ID 无效。");
  return value;
}

export function validateReference(reference) {
  if (
    !reference ||
    !/^[a-zA-Z0-9_-]{1,100}$/.test(reference.assetId ?? "") ||
    !Number.isInteger(reference.revision) ||
    reference.revision < 1 ||
    reference.revision > 2147483647
  )
    throw Error("角色复刻档案必须关联固定版本参考录音。");
  for (const key of ["transcript", "cloningPermission"])
    if (
      typeof reference[key] !== "string" ||
      !reference[key].trim() ||
      Buffer.byteLength(reference[key]) > 4000 ||
      /[\u0000-\u001f\u007f]/.test(reference[key])
    )
      throw Error("角色复刻档案必须包含参考原文及复刻授权依据。");
}

export async function queryVoice(id, env, fetcher = fetch) {
  voiceId(id);
  const data = await qwenVoiceCall(
    { action: "query_voice", voice_id: id },
    env,
    fetcher,
  );
  const output = data.output;
  if (
    !statuses.has(output.status) ||
    typeof output.target_model !== "string" ||
    !/^[a-zA-Z0-9_.-]{1,200}$/.test(output.target_model)
  )
    throw Error("提供方音色详情缺少可验证的状态或绑定模型。");
  // Deliberately omit resource_link, provider raw payload and credentials.
  return {
    voiceId: id,
    model: output.target_model,
    status: output.status,
    requestId: data.request_id,
    checkedAt: new Date().toISOString(),
  };
}

export async function verifyClonedVoice(profile, env, fetcher = fetch) {
  if (
    profile?.provider !== "qwen" ||
    profile.model !== "qwen-audio-3.1-tts-flash" ||
    profile.voiceKind !== "cloned" ||
    profile.locale !== "fr-FR"
  )
    throw Error("角色复刻档案模型尚未适配。");
  validateReference(profile.referenceAudio);
  const verification = await queryVoice(profile.voiceId, env, fetcher);
  if (verification.model !== profile.model)
    throw Error("提供方音色绑定模型与角色档案不一致，未生成语音。");
  if (verification.status !== "OK")
    throw Error("提供方音色正在处理或不可用，未生成语音。");
  return verification;
}

export async function listVoices(
  { prefix = "", pageIndex = 0, pageSize = 20 } = {},
  env,
  fetcher = fetch,
) {
  if (
    typeof prefix !== "string" ||
    !/^[a-zA-Z0-9]{0,10}$/.test(prefix) ||
    !Number.isInteger(pageIndex) ||
    pageIndex < 0 ||
    pageIndex > 10000 ||
    !Number.isInteger(pageSize) ||
    pageSize < 1 ||
    pageSize > 20
  )
    throw Error("角色音色列表参数无效。");
  const data = await qwenVoiceCall(
    {
      action: "list_voice",
      ...(prefix ? { prefix } : {}),
      page_index: pageIndex,
      page_size: pageSize,
    },
    env,
    fetcher,
  );
  if (
    !Array.isArray(data.output.voice_list) ||
    data.output.voice_list.length > pageSize
  )
    throw Error("提供方音色列表格式无效。");
  const items = data.output.voice_list.map((item) => {
    voiceId(item?.voice_id);
    if (!statuses.has(item.status)) throw Error("提供方音色列表状态无效。");
    return { voiceId: item.voice_id, status: item.status };
  });
  if (new Set(items.map((i) => i.voiceId)).size !== items.length)
    throw Error("提供方音色列表存在重复记录。");
  // This API provides no total count. A full page means another page may exist.
  return {
    items,
    pageIndex,
    nextPage: items.length === pageSize ? pageIndex + 1 : null,
    requestId: data.request_id,
  };
}

export async function run(args, env = process.env) {
  if (args.length === 2 && args[0] === "--query")
    return queryVoice(args[1], env);
  if (args.length === 2 && args[0] === "--list" && /^\d{1,5}$/.test(args[1]))
    return listVoices({ pageIndex: Number(args[1]) }, env);
  throw Error(
    "用法：qwen-voices.mjs --query <voice-id> 或 --list <从0开始的页码>",
  );
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  try {
    console.log(JSON.stringify(await run(process.argv.slice(2)), null, 2));
  } catch (error) {
    console.error(
      /^(角色|提供方|请在 |用法：)/.test(error.message)
        ? error.message
        : "提供方音色查询失败，未自动重试。",
    );
    process.exitCode = 1;
  }
}
