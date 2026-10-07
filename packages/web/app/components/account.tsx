import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, useRouteLoaderData } from "react-router";
import type { loader } from "../root";
import type { LoginRequest } from "@brioche/contracts/LoginRequest";
import type { AcceptInviteRequest } from "@brioche/contracts/AcceptInviteRequest";
import type { ResetPasswordRequest } from "@brioche/contracts/ResetPasswordRequest";
import { authRequest } from "../lib/auth.client";
import { accountReturnPath } from "../lib/account-return";
import { useLearning } from "./learning";
export function Account({
  mode,
}: {
  mode: "login" | "invite" | "reset-password";
}) {
  const identity = useRouteLoaderData<typeof loader>("root");
  const learning = useLearning();
  const busy = useRef(false);
  const alive = useRef(true);
  const request = useRef<AbortController | null>(null);
  const form = useRef<HTMLFormElement>(null);
  const feedback = useRef<HTMLParagraphElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const [email, setEmail] = useState(""),
    [password, setPassword] = useState(""),
    [name, setName] = useState(""),
    [token, setToken] = useState(""),
    [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    [done, setDone] = useState(false);
  useLayoutEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      request.current?.abort();
      request.current = null;
    };
  }, []);
  useLayoutEffect(() => {
    if (error && form.current?.contains(document.activeElement))
      feedback.current?.focus();
  }, [error]);
  useLayoutEffect(() => {
    if (done) heading.current?.focus();
  }, [done]);
  useEffect(() => {
    if (mode === "login") return;
    function readLink() {
      if (!window.location.hash) return;
      request.current?.abort();
      request.current = null;
      busy.current = false;
      setPending(false);
      setPassword("");
      setName("");
      setError("");
      setDone(false);
      const params = new URLSearchParams(window.location.hash.slice(1));
      setToken(params.get("token") ?? "");
      setEmail(params.get("email") ?? "");
      window.history.replaceState(
        window.history.state,
        "",
        window.location.pathname,
      );
    }
    readLink();
    window.addEventListener("hashchange", readLink);
    return () => window.removeEventListener("hashchange", readLink);
  }, [mode]);
  const title =
    mode === "login"
      ? "欢迎回来"
      : mode === "invite"
        ? "开始你的法语日常"
        : "设置新密码";
  async function submit() {
    if (busy.current || !alive.current) return;
    busy.current = true;
    const attempt = new AbortController();
    request.current = attempt;
    setPending(true);
    setError("");
    try {
      const body: LoginRequest | AcceptInviteRequest | ResetPasswordRequest =
        mode === "login"
          ? { email, password }
          : mode === "invite"
            ? { email, password, token, displayName: name }
            : { token, password };
      await authRequest(
        mode === "invite" ? "accept-invite" : mode,
        body,
        attempt.signal,
      );
      if (
        !alive.current ||
        request.current !== attempt ||
        attempt.signal.aborted
      )
        return;
      setPassword("");
      learning.stop();
      if (mode === "reset-password") {
        setDone(true);
        setToken("");
      } else {
        const next =
          mode === "login"
            ? new URLSearchParams(window.location.search).get("next")
            : null;
        window.location.assign(accountReturnPath(next));
      }
    } catch (e) {
      if (
        !alive.current ||
        request.current !== attempt ||
        attempt.signal.aborted
      )
        return;
      setError(
        e instanceof Error && !["TypeError", "TimeoutError"].includes(e.name)
          ? e.message
          : "请求未完成，请稍后重试。",
      );
    } finally {
      if (request.current === attempt) {
        request.current = null;
        busy.current = false;
        if (alive.current) setPending(false);
      }
    }
  }
  return (
    <section className="account-page page-arrive">
      <h1 ref={heading} tabIndex={-1}>
        {done ? "密码已更新" : title}
      </h1>
      {done ? (
        <>
          <p>旧会话已退出，请用新密码登录。</p>
          <Link className="primary" to="/login">
            登录
          </Link>
        </>
      ) : !identity?.enabled ? (
        <>
          <p>当前是访客试学。账号功能需要连接数据库后使用。</p>
          <Link className="text-button" to="/">
            返回课程
          </Link>
        </>
      ) : mode !== "login" && !token ? (
        <>
          <p>请通过管理员提供的完整链接打开此页面。</p>
          <Link className="text-button" to="/login">
            返回登录
          </Link>
        </>
      ) : (
        <form
          ref={form}
          aria-busy={pending}
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          {mode !== "reset-password" && (
            <label>
              邮箱
              <input
                type="email"
                required
                autoComplete={mode === "login" ? "username" : "email"}
                maxLength={254}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                readOnly={pending}
              />
            </label>
          )}
          {mode === "invite" && (
            <label>
              怎么称呼你
              <input
                required
                autoComplete="nickname"
                maxLength={80}
                value={name}
                onChange={(e) => setName(e.target.value)}
                readOnly={pending}
              />
            </label>
          )}
          <label>
            {mode === "reset-password" ? "新密码" : "密码"}
            <input
              type="password"
              required
              minLength={mode === "login" ? undefined : 12}
              maxLength={128}
              autoComplete={
                mode === "login" ? "current-password" : "new-password"
              }
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              readOnly={pending}
            />
          </label>
          {mode !== "login" && (
            <p className="profile-note">
              使用 12–128 个字符，建议设置容易记住的长密码。
            </p>
          )}
          {error && (
            <p
              ref={feedback}
              tabIndex={-1}
              className="error-message"
              role="alert"
            >
              {error}
            </p>
          )}
          <button
            className="primary"
            aria-disabled={pending}
            aria-busy={pending}
          >
            {pending
              ? "正在确认"
              : mode === "login"
                ? "登录"
                : mode === "invite"
                  ? "创建账号"
                  : "更新密码"}
          </button>
          {mode === "login" && (
            <p className="profile-note">
              需要账号或忘记密码时，请联系管理员获取邀请或恢复链接。
            </p>
          )}
        </form>
      )}
    </section>
  );
}
