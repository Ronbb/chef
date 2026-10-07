// Credentials may only reach the explicitly supported Beijing workspace API.
export function qwenBase(env) {
  if (!env.DASHSCOPE_API_KEY)
    throw Error("请在 .local/tts.env 配置北京地域 DASHSCOPE_API_KEY。");
  let base;
  try {
    if (env.DASHSCOPE_BASE_URL) base = new URL(env.DASHSCOPE_BASE_URL);
    else {
      if (!/^[a-zA-Z0-9_-]{1,100}$/.test(env.QWEN_WORKSPACE_ID ?? ""))
        throw Error();
      base = new URL(
        `https://${env.QWEN_WORKSPACE_ID}.cn-beijing.maas.aliyuncs.com`,
      );
    }
  } catch {
    throw Error(
      "提供方基础地址无效，请配置北京业务空间 DASHSCOPE_BASE_URL 或 QWEN_WORKSPACE_ID。",
    );
  }
  if (
    base.protocol !== "https:" ||
    base.username ||
    base.password ||
    base.port ||
    base.search ||
    base.hash ||
    !/^[a-zA-Z0-9_-]{1,100}\.cn-beijing\.maas\.aliyuncs\.com$/.test(
      base.hostname,
    ) ||
    ![
      "/",
      "/api/v1",
      "/api/v1/",
      "/compatible-mode/v1",
      "/compatible-mode/v1/",
    ].includes(base.pathname)
  )
    throw Error(
      "提供方基础地址需为北京业务空间 HTTPS 域名，路径为根路径、/api/v1 或 /compatible-mode/v1。",
    );
  return base.origin;
}

export async function qwenVoiceCall(input, env, fetcher = fetch) {
  const endpoint = `${qwenBase(env)}/api/v1/services/audio/tts/customization`;
  let response;
  try {
    response = await fetcher(endpoint, {
      method: "POST",
      redirect: "error",
      signal: AbortSignal.timeout(30_000),
      headers: {
        Authorization: `Bearer ${env.DASHSCOPE_API_KEY}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ model: "voice-enrollment", input }),
    });
  } catch {
    throw Error("提供方音色查询连接失败或超时，未自动重试。");
  }
  if (!response.ok) {
    await response.body?.cancel().catch(() => {});
    throw Error(`提供方音色查询失败（HTTP ${response.status}）。`);
  }
  if (
    !response.body ||
    Number(response.headers.get("content-length")) > 256_000
  ) {
    await response.body?.cancel().catch(() => {});
    throw Error("提供方音色响应超出限额。");
  }
  const reader = response.body.getReader(),
    chunks = [];
  let size = 0,
    data;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > 256_000) throw Error();
      chunks.push(value);
    }
    data = JSON.parse(Buffer.concat(chunks, size).toString("utf8"));
  } catch {
    // A stream error can contain the provider URL or credentials; never forward it.
    throw Error("提供方音色响应不完整、超出限额或格式无效。");
  } finally {
    await reader.cancel().catch(() => {});
  }
  if (
    !data ||
    typeof data !== "object" ||
    data.code ||
    !data.output ||
    typeof data.request_id !== "string" ||
    !/^[a-zA-Z0-9_-]{1,200}$/.test(data.request_id)
  )
    throw Error("提供方音色响应格式无效。");
  return data;
}
