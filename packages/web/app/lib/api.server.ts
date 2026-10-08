import type { Catalog } from "@brioche/contracts/Catalog";
import type { PublicLesson } from "@brioche/contracts/PublicLesson";
import type { NeutralLesson } from "@brioche/contracts/NeutralLesson";
import type { NeutralCatalog } from "@brioche/contracts/NeutralCatalog";
import type { UserProfile } from "@brioche/contracts/UserProfile";
import { productNamespace } from "./product-runtime";
import { productSessionCookie } from "./product-session";
const base = () => process.env.INTERNAL_API_URL ?? "http://127.0.0.1:3001";
function sessionCookie(request: Request) {
  return productSessionCookie(request.headers, productNamespace);
}
export async function getPrivate<T>(
  request: Request,
  path: string,
): Promise<T> {
  const cookie = sessionCookie(request);
  let response: Response;
  try {
    response = await fetch(base() + path, {
      headers: cookie ? { cookie } : {},
      cache: "no-store",
      signal: AbortSignal.timeout(5000),
    });
  } catch {
    throw new Response("学习服务暂时无法连接。", { status: 503 });
  }
  if (!response.ok)
    throw new Response(
      response.status === 401
        ? "请登录后继续学习。"
        : response.status === 410
          ? "课程已撤回，暂时无法继续学习。"
          : response.status === 404
            ? "没有找到这次学习记录。"
            : "学习服务暂时不可用。",
      { status: response.status },
    );
  return response.json() as Promise<T>;
}
async function api<T>(path: string): Promise<T> {
  let response: Response;
  try {
    response = await fetch(base() + path, {
      signal: AbortSignal.timeout(5000),
    });
  } catch {
    throw new Response("课程服务暂时无法连接，请稍后重试。", { status: 503 });
  }
  if (!response.ok)
    throw new Response(
      response.status === 404
        ? "没有找到这堂课程。"
        : response.status === 410
          ? "课程已撤回，暂时无法继续学习。"
          : "课程服务暂时不可用。",
      { status: response.status },
    );
  return response.json() as Promise<T>;
}
export const getCatalog = (query?: string) =>
  api<Catalog>(
    "/api/catalog" + (query ? "?q=" + encodeURIComponent(query) : ""),
  );
export const getLesson = (id: string, revision?: number) =>
  api<PublicLesson>(
    "/api/lessons/" +
      encodeURIComponent(id) +
      (revision ? "?revision=" + revision : ""),
  );
export const getReadingCatalog = () => api<NeutralCatalog>("/api/v2/catalog");
export const getReadingLesson = (id: string, revision?: number) =>
  api<NeutralLesson>(
    "/api/v2/lessons/" +
      encodeURIComponent(id) +
      (revision ? "?revision=" + revision : ""),
  );
export async function getIdentity(
  request: Request,
): Promise<{ user: UserProfile | null; enabled: boolean }> {
  const cookie = sessionCookie(request);
  let response: Response;
  try {
    response = await fetch(base() + "/api/v1/me", {
      headers: cookie ? { cookie } : {},
      cache: "no-store",
      signal: AbortSignal.timeout(5000),
    });
  } catch {
    throw new Response("账号服务暂时无法连接。", { status: 503 });
  }
  if (response.status === 401) return { user: null, enabled: true };
  if (response.status === 404) {
    const catalog = await getCatalog();
    if (catalog.developmentFixture) return { user: null, enabled: false };
  }
  if (!response.ok) throw new Response("账号服务暂时不可用。", { status: 503 });
  return { user: (await response.json()) as UserProfile, enabled: true };
}
