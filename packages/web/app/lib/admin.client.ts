import type { CsrfToken } from "@brioche/contracts/CsrfToken";
export class AdminWriteError extends Error {
  readonly status: number;
  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}
async function adminResponse(
  path: string,
  body: object | FormData,
  signal?: AbortSignal,
  timeoutMs = 30000,
): Promise<Response> {
  signal?.throwIfAborted();
  const bootstrap = await fetch("/api/v1/auth/csrf", {
    cache: "no-store",
    signal: signal
      ? AbortSignal.any([signal, AbortSignal.timeout(10000)])
      : AbortSignal.timeout(10000),
  });
  if (!bootstrap.ok) throw Error("管理员服务暂时不可用。");
  const { csrfToken } = (await bootstrap.json()) as CsrfToken;
  signal?.throwIfAborted();
  const response = await fetch(`/api/v1/operator/${path}`, {
    method: "POST",
    cache: "no-store",
    headers:
      body instanceof FormData
        ? { "X-CSRF-Token": csrfToken }
        : { "Content-Type": "application/json", "X-CSRF-Token": csrfToken },
    body: body instanceof FormData ? body : JSON.stringify(body),
    signal: signal
      ? AbortSignal.any([signal, AbortSignal.timeout(timeoutMs)])
      : AbortSignal.timeout(timeoutMs),
  });
  if (!response.ok) {
    const messages: Record<number, string> = {
      400: "未通过检查，请核对审批、素材与填写的信息。",
      401: "登录已过期，请重新登录。",
      403: "没有管理权限或请求未通过验证。",
      404: "没有找到指定对象，请刷新后核对。",
      409: "状态已发生变化，请刷新后核对再操作。",
      410: "这个课程版本已撤回。",
      413: "提交内容过大，请核对文件大小。",
    };
    throw new AdminWriteError(
      messages[response.status] ?? "操作未确认，请刷新核对状态后重试。",
      response.status,
    );
  }
  return response;
}
export async function adminWrite<T>(
  path: string,
  body: object | FormData,
  signal?: AbortSignal,
): Promise<T> {
  return (await adminResponse(path, body, signal)).json() as Promise<T>;
}
export async function adminArchive(
  path: string,
  body: object,
  signal: AbortSignal,
): Promise<Blob> {
  const response = await adminResponse(path, body, signal, 120000);
  if (
    response.headers.get("Content-Type")?.split(";")[0] !==
      "application/x-tar" ||
    !response.body
  )
    throw Error("录音课包响应无效，请重新核对。");
  const reader = response.body.getReader();
  const chunks: ArrayBuffer[] = [];
  let length = 0;
  try {
    while (true) {
      signal.throwIfAborted();
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > 128 * 1024 * 1024) throw Error("录音课包超过下载上限。");
      chunks.push(value.slice().buffer);
    }
    signal.throwIfAborted();
    if (!length) throw Error("录音课包为空，请重新核对。");
    return new Blob(chunks, { type: "application/x-tar" });
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
