export type SessionNamespace = "brioche" | "hargow";

/** Only the configured product's session crosses the SSR service boundary. */
export function productSessionCookie(
  headers: Headers,
  namespace: SessionNamespace,
): string {
  if (namespace !== "brioche" && namespace !== "hargow")
    throw Error("Invalid product session namespace");
  const names = new Set([`${namespace}.sid`, `__Host-${namespace}.sid`]);
  const matches = (headers.get("cookie") ?? "")
    .split(";")
    .map((part) => part.trim())
    .filter((part) => {
      const separator = part.indexOf("=");
      return separator > 0 && names.has(part.slice(0, separator));
    });
  if (matches.length > 1)
    throw new Response("会话信息不明确，请重新登录。", { status: 401 });
  const cookie = matches[0] ?? "";
  if (cookie) {
    const value = cookie.slice(cookie.indexOf("=") + 1);
    if (
      !value ||
      value.length > 4096 ||
      /[^\x21\x23-\x2B\x2D-\x3A\x3C-\x5B\x5D-\x7E]/.test(value)
    )
      throw new Response("会话信息无效，请重新登录。", { status: 401 });
  }
  return cookie;
}
