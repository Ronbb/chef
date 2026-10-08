import { productNamespace } from "./product-runtime";
import type { CsrfToken } from "@brioche/contracts/CsrfToken";
import { announceIdentityChange } from "./identity-sync";
export async function authRequest<T>(
  path: string,
  body?: object,
  signal?: AbortSignal,
): Promise<T> {
  signal?.throwIfAborted();
  const bootstrap = await fetch("/api/v1/auth/csrf", {
    cache: "no-store",
    signal: signal
      ? AbortSignal.any([signal, AbortSignal.timeout(10000)])
      : AbortSignal.timeout(10000),
  });
  if (!bootstrap.ok) throw Error("账号服务暂时不可用，请稍后重试。");
  const { csrfToken } = (await bootstrap.json()) as CsrfToken;
  signal?.throwIfAborted();
  const response = await fetch("/api/v1/auth/" + path, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-CSRF-Token": csrfToken },
    body: body ? JSON.stringify(body) : undefined,
    signal: signal
      ? AbortSignal.any([signal, AbortSignal.timeout(15000)])
      : AbortSignal.timeout(15000),
  });
  if (!response.ok) {
    const messages: Record<number, string> = {
      400: "请检查信息，邀请或恢复链接可能已过期。",
      401: "邮箱或密码不正确。",
      403: "请求未通过验证，请刷新页面重试。",
      429: "尝试次数较多，请稍后重试。",
    };
    throw Error(
      messages[response.status] ?? "账号服务暂时不可用，请稍后重试。",
    );
  }
  if (["login", "logout", "accept-invite", "reset-password"].includes(path)) {
    announceIdentityChange(productNamespace);
  }
  return response.json() as Promise<T>;
}
