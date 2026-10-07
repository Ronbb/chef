import type { CsrfToken } from "@brioche/contracts/CsrfToken";
export class ApiRequestError extends Error {
  status: number;
  phase: "csrf" | "request";
  constructor(
    status: number,
    message: string,
    phase: "csrf" | "request" = "request",
  ) {
    super(message);
    this.status = status;
    this.phase = phase;
  }
}
export const definitiveWriteFailure = (failure: ApiRequestError) =>
  failure.phase === "request" &&
  [400, 404, 409, 410, 422].includes(failure.status);
export async function privateRequest<T>(
  path: string,
  method: "GET" | "PATCH" | "POST" | "PUT",
  body?: object,
  signal?: AbortSignal,
): Promise<T> {
  signal?.throwIfAborted();
  const headers: Record<string, string> = {};
  if (method !== "GET") {
    const bootstrap = await fetch("/api/v1/auth/csrf", {
      cache: "no-store",
      signal: signal
        ? AbortSignal.any([signal, AbortSignal.timeout(10000)])
        : AbortSignal.timeout(10000),
    });
    if (!bootstrap.ok)
      throw new ApiRequestError(
        bootstrap.status,
        "服务暂时不可用，请稍后重试。",
        "csrf",
      );
    headers["X-CSRF-Token"] = ((await bootstrap.json()) as CsrfToken).csrfToken;
    signal?.throwIfAborted();
    headers["Content-Type"] = "application/json";
  }
  const response = await fetch(path, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
    cache: "no-store",
    signal: signal
      ? AbortSignal.any([signal, AbortSignal.timeout(15000)])
      : AbortSignal.timeout(15000),
  });
  if (!response.ok) {
    const messages: Record<number, string> = {
      400: "请检查填写的信息。",
      401: "登录已过期，请重新登录。",
      403: "请求未通过验证，请刷新页面重试。",
      409: "其他设备已更新，请确认最新设置后重试。",
      422: "请检查填写的信息。",
      429: "操作较频繁，请稍后重试。",
    };
    throw new ApiRequestError(
      response.status,
      messages[response.status] ?? "保存未完成，请稍后重试。",
    );
  }
  return response.json() as Promise<T>;
}
