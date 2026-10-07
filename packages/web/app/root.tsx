import {
  Links,
  Meta,
  Outlet,
  Scripts,
  ScrollRestoration,
  Link,
  isRouteErrorResponse,
  useRouteError,
  data,
  useRouteLoaderData,
} from "react-router";
import { LearningProvider } from "./components/learning";
import { IdentitySync } from "./components/identity-sync";
import "./styles/app.css";
import { Scrollbar } from "./components/scrollbar";
import { RouteFocus, focusPageContent } from "./components/route-focus";
import { getIdentity } from "./lib/api.server";
import type { Route } from "./+types/root";
import { useCallback, useState, useEffect } from "react";
import product from "@chef/product";
export async function loader({ request }: Route.LoaderArgs) {
  return data(await getIdentity(request), {
    headers: { "Cache-Control": "private, no-store", Vary: "Cookie" },
  });
}
export function headers() {
  return { "Cache-Control": "private, no-store", Vary: "Cookie" };
}
export function Layout({ children }: { children: React.ReactNode }) {
  const identity = useRouteLoaderData<typeof loader>("root");
  const [identityInvalidated, setIdentityInvalidated] = useState(false);
  const invalidateIdentity = useCallback(
    () => setIdentityInvalidated(true),
    [],
  );
  return (
    <html
      lang={product.explanationLanguage}
      className="overlay-scroll"
      style={product.theme}
    >
      <head>
        <meta charSet="utf-8" />
        <meta
          name="viewport"
          content="width=device-width,initial-scale=1,viewport-fit=cover"
        />
        <meta name="theme-color" content={product.themeColor} />
        <meta name="application-name" content={product.name} />
        <meta name="mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-title" content={product.name} />
        <meta name="apple-mobile-web-app-status-bar-style" content="default" />
        <link rel="manifest" href="/manifest.webmanifest" />
        <link rel="icon" href="/icons/app.svg" type="image/svg+xml" />
        <link
          rel="icon"
          href="/icons/favicon-32.png"
          type="image/png"
          sizes="32x32"
        />
        <link
          rel="apple-touch-icon"
          href="/apple-touch-icon.png"
          sizes="180x180"
        />
        <title>{`${product.name} · ${product.tagline}`}</title>
        <Meta />
        <Links />
      </head>
      <body>
        <a className="skip-link" href="#page-content">
          跳到正文
        </a>
        <LearningProvider
          key={identityInvalidated ? "invalidated" : "active"}
          user={identityInvalidated ? null : (identity?.user ?? null)}
        >
          <IdentitySync
            user={identityInvalidated ? null : (identity?.user ?? null)}
            enabled={!identityInvalidated && (identity?.enabled ?? false)}
            onInvalidate={invalidateIdentity}
          />
          <div className="app">
            <header className="topbar">
              <Link
                className="brand"
                to="/"
                aria-label={`${product.name} 首页`}
              >
                <span className="brand-mark" aria-hidden="true">
                  <img src={product.brandIcon} alt="" width="27" height="27" />
                </span>
                <span>{product.wordmark}</span>
              </Link>
              <Link
                className="profile-settings profile-avatar"
                to="/profile"
                aria-label="个人信息与设置"
              >
                <img src={product.avatar} alt="" width="44" height="44" />
              </Link>
            </header>
            <main id="page-content" tabIndex={-1}>
              {identityInvalidated ? (
                <section className="home">
                  <h1 tabIndex={-1}>账号状态已更新</h1>
                  <p role="status">正在重新确认账号。</p>
                  <button
                    className="primary"
                    onClick={() => window.location.reload()}
                  >
                    重新加载
                  </button>
                </section>
              ) : (
                children
              )}
            </main>
          </div>
          <Scrollbar />
          <RouteFocus />
        </LearningProvider>
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}
export default function App() {
  return <Outlet />;
}
export function ErrorBoundary() {
  const error = useRouteError();
  useEffect(() => {
    // Layout can remount when this root error resolves, including a history POP.
    // Focus the committed destination after disposal; normal first loads keep their focus.
    return () => {
      requestAnimationFrame(focusPageContent);
    };
  }, []);
  const status = isRouteErrorResponse(error) ? error.status : 500;
  const retryable = status >= 500;
  const [title, message] =
    status === 404
      ? ["没有找到这页内容", "这页内容可能已经移除，请回到课程目录查看。"]
      : status === 410
        ? ["课程已撤回", "这堂课程暂时无法继续学习，请选择其他课程。"]
        : status === 401
          ? ["请先登录", "登录后可以继续查看你的学习记录。"]
          : status === 403
            ? ["暂时无法访问", "当前账号没有访问这页内容的权限。"]
            : status === 400
              ? [
                  "请检查输入信息",
                  isRouteErrorResponse(error) && typeof error.data === "string"
                    ? error.data
                    : "输入信息无效，请检查后重试。",
                ]
              : status === 503
                ? ["服务暂时不可用", "请稍后重新加载。"]
                : ["暂时无法打开", "页面暂时遇到问题，请稍后重试。"];
  return (
    <section className="home">
      <h1 tabIndex={-1}>{title}</h1>
      <p className="error-message">{message}</p>
      <div className="error-recovery">
        {retryable && (
          <button
            type="button"
            className="primary"
            onClick={() => window.location.reload()}
          >
            重新加载
          </button>
        )}
        {status === 401 && (
          <Link className="primary" to="/login">
            登录
          </Link>
        )}
        <Link
          className={retryable || status === 401 ? "text-button" : "primary"}
          to="/courses"
        >
          返回课程目录
        </Link>
      </div>
    </section>
  );
}
