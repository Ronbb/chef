import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { createRequestHandler } from "react-router";
import { productWebUrl, browserCliUrl } from "../test-product.mjs";
const build = await import(productWebUrl("build/server/index.js"));

const source = JSON.parse(
  await readFile(
    new URL("../../../docs/examples/a1-bakery.lesson.json", import.meta.url),
    "utf8",
  ),
);
const fields = [
  "schemaVersion",
  "id",
  "revision",
  "levelId",
  "unitId",
  "title",
  "summaryZh",
  "estimatedMinutes",
  "objectivesZh",
  "knowledge",
  "blocks",
  "steps",
  "completion",
  "reviewItemIds",
  "cast",
];
const lesson = {
  ...Object.fromEntries(fields.map((key) => [key, source[key]])),
  media: [],
  audio: [],
  audioTracks: [],
};
let fixture = false;
let authenticated = false;
let lessonStatus = 200;
let neutralPublicLesson = null;
let neutralPrivateSession = null;
const profile = {
  id: "00000000-0000-0000-0000-000000000001",
  email: "learner@example.test",
  displayName: "测试学习者",
  role: "learner",
  settings: {
    timeZone: "Asia/Shanghai",
    weeklyDays: 3,
    dailyMinutes: 10,
    showTranslation: false,
    speechRate: 1,
  },
  version: 1,
};
const requests = [];
const originalBase = process.env.INTERNAL_API_URL;
const handler = createRequestHandler(build, "production");
const server = createServer((request, response) => {
  requests.push({
    method: request.method,
    path: request.url,
    cookie: request.headers.cookie,
  });
  response.setHeader("Content-Type", "application/json");
  if (request.url === "/api/v1/me") {
    response.statusCode = authenticated ? 200 : fixture ? 404 : 401;
    response.end(JSON.stringify(authenticated ? profile : {}));
  } else if (request.url === "/api/v2/learning-sessions/native-session") {
    response.statusCode = authenticated && neutralPrivateSession ? 200 : 401;
    response.end(JSON.stringify(authenticated ? neutralPrivateSession : {}));
  } else if (
    request.url.startsWith("/api/v1/operator/overview") &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(
      JSON.stringify({
        generation: "0",
        activeRelease: null,
        lessons: [],
        releases: [],
        lessonNext: null,
        releaseNext: null,
      }),
    );
  } else if (
    request.url.endsWith("/speech-options") &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(
      JSON.stringify({
        lesson,
        voices: lesson.cast.map((character) => ({
          character,
          avatarRevision: 1,
          voiceRevision: 0,
          profile: null,
        })),
      }),
    );
  } else if (
    request.url.startsWith("/api/v1/operator/speech-plans") &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(JSON.stringify({ items: [], next: null }));
  } else if (
    request.url.startsWith("/api/v1/operator/accounts/pending-tokens") &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(JSON.stringify({ items: [], nextId: null }));
  } else if (
    (request.url.startsWith("/api/v1/operator/assets") ||
      request.url.startsWith("/api/v1/operator/recordings") ||
      request.url.startsWith("/api/v1/operator/voice-references") ||
      request.url.startsWith("/api/v1/operator/voice-jobs") ||
      request.url.startsWith("/api/v1/operator/voice-auditions")) &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(JSON.stringify({ items: [], next: null, configured: false }));
  } else if (
    request.url.startsWith("/api/v1/operator/characters") &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(
      JSON.stringify(
        request.url === "/api/v1/operator/characters/character-camille/1"
          ? {
              character: source.cast[0],
              avatarRevision: 1,
              voiceRevision: 0,
              profile: null,
            }
          : { items: [], nextId: null },
      ),
    );
  } else if (
    request.url.startsWith("/api/v1/operator/history") &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(JSON.stringify({ items: [], next: null }));
  } else if (
    /^\/api\/v1\/operator\/accounts\/1\/sessions/.test(request.url) &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(
      JSON.stringify({
        account: {
          id: "1",
          email: "controlled@example.test",
          displayName: "测试账号",
          role: "learner",
        },
        items: [
          {
            id: "a".repeat(64),
            expiresAt: "2027-01-01T00:00:00Z",
            current: false,
          },
        ],
        nextId: null,
      }),
    );
  } else if (
    request.url.startsWith("/api/v1/operator/accounts") &&
    authenticated &&
    profile.role === "operator"
  ) {
    response.end(
      JSON.stringify({
        items: [
          {
            id: "1",
            email: "controlled@example.test",
            displayName: "测试账号",
            role: "learner",
          },
        ],
        nextId: null,
      }),
    );
  } else if (request.url === "/api/v1/me/reviews" && authenticated) {
    response.end(
      JSON.stringify({
        items: [],
        dueCount: 0,
        nextDueAt: null,
        localDate: "2026-10-06",
        timeZone: "Asia/Shanghai",
      }),
    );
  } else if (
    request.url.startsWith("/api/v1/me/review-history") &&
    authenticated
  ) {
    response.end(
      JSON.stringify({
        items: request.url.includes("?cursor=")
          ? []
          : [
              {
                id: "ssr-history",
                cardId: "ssr-card",
                vocabulary: lesson.knowledge.vocabulary[0],
                withdrawn: false,
                rating: "familiar",
                oldStage: 0,
                newStage: 1,
                reviewedAt: "2026-10-05T23:30:00Z",
                dueAt: "2026-10-08T23:30:00Z",
                timeZone: "Asia/Shanghai",
                algorithmVersion: "ssr-fixture",
              },
            ],
        nextCursor: request.url.includes("?cursor=") ? null : "older/qa?+",
      }),
    );
  } else if (
    authenticated &&
    profile.role === "operator" &&
    request.url === "/api/v1/operator/releases/ssr-release"
  ) {
    response.end(
      JSON.stringify({
        id: "ssr-release",
        catalog: {
          developmentFixture: false,
          levels: [
            {
              id: "a1",
              label: "A1",
              units: [
                {
                  id: lesson.unitId,
                  titleZh: "早餐与面包店",
                  lessons: [lesson],
                },
              ],
            },
          ],
        },
        withdrawnLessonIds: [],
      }),
    );
  } else if (
    authenticated &&
    profile.role === "operator" &&
    request.url ===
      `/api/v1/operator/lessons/${lesson.id}/revisions/${lesson.revision}`
  ) {
    response.end(JSON.stringify(lesson));
  } else if (
    request.url === "/api/catalog" ||
    request.url === "/api/v2/catalog"
  ) {
    response.end(
      JSON.stringify({
        developmentFixture: fixture,
        levels: [
          {
            id: "a1",
            label: "A1",
            units: [
              { id: lesson.unitId, titleZh: "早餐与面包店", lessons: [lesson] },
            ],
          },
        ],
      }),
    );
  } else if (
    request.url.startsWith("/api/lessons/") ||
    request.url.startsWith("/api/v2/lessons/")
  ) {
    response.statusCode = lessonStatus;
    response.end(
      JSON.stringify(
        lessonStatus === 200 ? (neutralPublicLesson ?? lesson) : {},
      ),
    );
  } else {
    response.statusCode = 401;
    response.end("{}");
  }
});
before(async () => {
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  process.env.INTERNAL_API_URL = `http://127.0.0.1:${server.address().port}`;
});
after(async () => {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  if (originalBase === undefined) delete process.env.INTERNAL_API_URL;
  else process.env.INTERNAL_API_URL = originalBase;
});
const request = (path) =>
  handler(
    new Request("http://brioche.test" + path, {
      headers: authenticated
        ? { cookie: "brioche.sid=controlled-ssr-session; unrelated=omit" }
        : {},
    }),
  );

test("admin history SSR authorizes before reading and only forwards cursor fields", async () => {
  fixture = false;
  authenticated = false;
  requests.length = 0;
  try {
    assert.equal((await request("/admin/history")).status, 401);
    authenticated = true;
    assert.equal((await request("/admin/history")).status, 403);
    assert.equal(
      requests.filter((item) =>
        item.path.startsWith("/api/v1/operator/history"),
      ).length,
      0,
    );
    profile.role = "operator";
    requests.length = 0;
    const parameters = new URLSearchParams({
      beforeTime: "2026-10-07T01:02:03.123456Z",
      beforeKey: "content:123",
      ignored: "client-only",
    });
    const response = await request(`/admin/history?${parameters}`);
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("cache-control"), "private, no-store");
    const forwarded = requests.find((item) =>
      item.path.startsWith("/api/v1/operator/history"),
    );
    assert.equal(forwarded.cookie, "brioche.sid=controlled-ssr-session");
    const query = new URL(forwarded.path, "http://controlled.test")
      .searchParams;
    assert.deepEqual([...query.keys()], ["beforeTime", "beforeKey"]);
    assert.equal(query.get("beforeKey"), "content:123");
    assert.match(await response.text(), /这一页没有更早的记录/);
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});
test("admin account SSR gates identity, filters cursors and never renders generated tokens", async () => {
  fixture = false;
  authenticated = false;
  requests.length = 0;
  try {
    assert.equal((await request("/admin/accounts")).status, 401);
    authenticated = true;
    assert.equal((await request("/admin/accounts")).status, 403);
    assert.equal(
      requests.filter((item) =>
        item.path.startsWith("/api/v1/operator/accounts"),
      ).length,
      0,
    );
    profile.role = "operator";
    requests.length = 0;
    const response = await request(
      "/admin/accounts?q=controlled&afterId=0&ignored=internal",
    );
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("cache-control"), "private, no-store");
    const forwarded = requests.find((item) =>
      item.path.startsWith("/api/v1/operator/accounts"),
    );
    assert.equal(
      forwarded.path,
      "/api/v1/operator/accounts?q=controlled&afterId=0",
    );
    assert.equal(forwarded.cookie, "brioche.sid=controlled-ssr-session");
    const html = await response.text();
    assert.match(html, /测试账号/);
    assert.match(html, /设为管理员/);
    assert.doesNotMatch(html, /id="account-link"/);
    assert.equal(
      requests.some((item) => item.method !== "GET"),
      false,
    );
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("admin session SSR authorizes before reading and forwards only the bounded cursor", async () => {
  try {
    fixture = false;
    authenticated = false;
    requests.length = 0;
    assert.equal((await request("/admin/accounts/1/sessions")).status, 401);
    authenticated = true;
    profile.role = "learner";
    assert.equal((await request("/admin/accounts/1/sessions")).status, 403);
    assert.equal(
      requests.some((item) => item.path.includes("/sessions")),
      false,
    );
    profile.role = "operator";
    requests.length = 0;
    const response = await request(
      "/admin/accounts/1/sessions?afterId=" +
        "b".repeat(64) +
        "&ignored=internal",
    );
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("cache-control"), "private, no-store");
    const read = requests.find((item) => item.path.includes("/sessions"));
    assert.equal(
      read.path,
      "/api/v1/operator/accounts/1/sessions?afterId=" + "b".repeat(64),
    );
    assert.equal(read.cookie, "brioche.sid=controlled-ssr-session");
    const html = await response.text();
    assert.match(html, /撤销此会话/);
    assert.match(html, /中国时间/);
    assert.doesNotMatch(html, /当前浏览器/);
    assert.equal(
      requests.some((item) => item.method !== "GET"),
      false,
    );
    assert.equal(
      (await request("/admin/accounts/not-an-id/sessions")).status,
      400,
    );
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("lesson SSR separates missing, withdrawn and unavailable states with useful recovery entries", async () => {
  fixture = true;
  authenticated = false;
  try {
    for (const [status, title] of [
      [404, "没有找到这页内容"],
      [410, "课程已撤回"],
      [503, "服务暂时不可用"],
    ]) {
      lessonStatus = status;
      const response = await request(`/lessons/${lesson.id}`);
      assert.equal(response.status, status);
      const html = await response.text();
      assert.ok(html.includes(title));
      assert.match(html, /href="\/courses"/);
      assert.equal(html.includes("重新加载"), status === 503);
      assert.ok(!html.includes(lesson.title.fr));
    }
  } finally {
    lessonStatus = 200;
    fixture = false;
  }
});

test("SSR activates overlay scrollbars in the initial document for anonymous and private pages", async () => {
  fixture = false;
  authenticated = false;
  try {
    for (const signedIn of [false, true]) {
      authenticated = signedIn;
      const response = await request("/profile");
      assert.equal(response.status, 200);
      assert.match(await response.text(), /<html[^>]*class="overlay-scroll"/);
    }
  } finally {
    authenticated = false;
    fixture = false;
  }
});

test("author SSR requires an operator and reads only the selected release member revision privately", async () => {
  fixture = false;
  authenticated = false;
  requests.length = 0;
  const path = `/author-preview?releaseId=ssr-release&lessonId=${lesson.id}&revision=${lesson.revision}`;
  try {
    let response = await request(path);
    assert.equal(response.status, 401);
    authenticated = true;
    response = await request(path);
    assert.equal(response.status, 403);
    assert.equal(
      requests.filter((entry) => entry.path.startsWith("/api/v1/operator/"))
        .length,
      0,
    );
    profile.role = "operator";
    requests.length = 0;
    response = await request(path);
    assert.equal(response.status, 200);
    const html = await response.text();
    assert.ok(html.includes(lesson.title.fr));
    assert.match(html, /name="releaseId"[^>]*value="ssr-release"/);
    assert.match(response.headers.get("Cache-Control"), /private, no-store/);
    assert.match(response.headers.get("Vary"), /Cookie/);
    const privateReads = requests.filter((entry) =>
      entry.path.startsWith("/api/v1/operator/"),
    );
    assert.deepEqual(
      privateReads.map((entry) => entry.path),
      [
        "/api/v1/operator/releases/ssr-release",
        `/api/v1/operator/lessons/${lesson.id}/revisions/${lesson.revision}`,
      ],
    );
    assert.ok(
      privateReads.every(
        (entry) => entry.cookie === "brioche.sid=controlled-ssr-session",
      ),
    );
    requests.length = 0;
    response = await request(
      `/author-preview?releaseId=ssr-release&lessonId=${lesson.id}&revision=${lesson.revision + 1}`,
    );
    assert.equal(response.status, 404);
    assert.equal(
      requests.filter((entry) =>
        entry.path.startsWith("/api/v1/operator/lessons/"),
      ).length,
      0,
    );
    requests.length = 0;
    response = await request("/author-preview?releaseId=invalid%2Fbatch");
    assert.equal(response.status, 400);
    assert.match(
      await response.text(),
      /<p class="error-message">发布批次编号无效。<\/p>/,
    );
    assert.equal(
      requests.filter((entry) => entry.path.startsWith("/api/v1/operator/"))
        .length,
      0,
    );
    assert.ok(requests.every((entry) => entry.method === "GET"));
  } finally {
    profile.role = "learner";
    authenticated = false;
    fixture = false;
  }
});

test("production history SSR forwards encoded cursors privately and distinguishes empty older pages", async () => {
  fixture = false;
  authenticated = true;
  requests.length = 0;
  try {
    let response = await request("/review-history");
    assert.equal(response.status, 200);
    let html = await response.text();
    assert.match(html, /href="\/review-history\?cursor=older%2Fqa%3F%2B"/);
    assert.ok(html.includes(lesson.knowledge.vocabulary[0].lemma));
    assert.match(response.headers.get("Cache-Control"), /private, no-store/);
    response = await request("/review-history?cursor=older%2Fqa%3F%2B");
    assert.equal(response.status, 200);
    html = await response.text();
    assert.ok(html.includes("这一页没有更早的复习记录。"));
    assert.match(html, /href="\/review-history"/);
    assert.ok(!html.includes("完成一次复习后，记录会显示在这里。"));
    const reads = requests.filter((entry) =>
      entry.path.startsWith("/api/v1/me/review-history"),
    );
    assert.deepEqual(
      reads.map((entry) => entry.path),
      [
        "/api/v1/me/review-history",
        "/api/v1/me/review-history?cursor=older%2Fqa%3F%2B",
      ],
    );
    assert.ok(
      reads.every(
        (entry) => entry.cookie === "brioche.sid=controlled-ssr-session",
      ),
    );
    assert.ok(requests.every((entry) => entry.method === "GET"));
    authenticated = false;
    response = await request("/review-history");
    assert.equal(response.status, 302);
    assert.equal(
      response.headers.get("Location"),
      "/login?next=/review-history",
    );
  } finally {
    authenticated = false;
    fixture = false;
  }
});

test("account SSR renders anonymous login and keeps invitation and recovery forms gated by their client links", async () => {
  fixture = false;
  authenticated = false;
  requests.length = 0;
  try {
    let response = await request("/login?next=/pending-saves");
    assert.equal(response.status, 200);
    let html = await response.text();
    assert.ok(html.includes("欢迎回来"));
    assert.match(html, /type="email"/);
    assert.match(html, /autoComplete="current-password"/);
    assert.match(response.headers.get("Cache-Control"), /private, no-store/);
    for (const path of ["/invite", "/reset-password"]) {
      response = await request(path);
      assert.equal(response.status, 200, path);
      html = await response.text();
      assert.ok(html.includes("请通过管理员提供的完整链接打开此页面。"));
      assert.doesNotMatch(html, /type="password"/);
    }
    fixture = true;
    response = await request("/login");
    assert.equal(response.status, 200);
    html = await response.text();
    assert.ok(html.includes("当前是访客试学"));
    assert.doesNotMatch(html, /type="password"/);
    assert.ok(requests.every((entry) => entry.method === "GET"));
  } finally {
    fixture = false;
    authenticated = false;
  }
});

test("profile SSR renders logout only for the server-authorized identity without making an auth mutation", async () => {
  fixture = false;
  authenticated = true;
  requests.length = 0;
  try {
    let response = await request("/profile");
    assert.equal(response.status, 200);
    let html = await response.text();
    assert.ok(html.includes(profile.displayName));
    assert.ok(html.includes(profile.email));
    assert.ok(html.includes("退出登录"));
    assert.doesNotMatch(html, /href="\/login"/);
    assert.match(response.headers.get("Cache-Control"), /private, no-store/);
    assert.equal(
      requests.find((entry) => entry.path === "/api/v1/me")?.cookie,
      "brioche.sid=controlled-ssr-session",
    );
    authenticated = false;
    response = await request("/profile");
    assert.equal(response.status, 200);
    html = await response.text();
    assert.ok(html.includes("登录账号"));
    assert.match(html, /href="\/login"/);
    assert.ok(!html.includes(profile.email));
    assert.ok(!html.includes("退出登录"));
    assert.ok(requests.every((entry) => entry.method === "GET"));
  } finally {
    authenticated = false;
    fixture = false;
  }
});

test("production legacy entries reach real learning and authenticated review without writes", async () => {
  fixture = false;
  requests.length = 0;
  for (const [path, destination] of [
    [`/practice/${lesson.id}`, `/lessons/${lesson.id}`],
    [`/review/${lesson.id}`, "/reviews"],
    ["/reviews", "/login?next=/reviews"],
  ]) {
    const response = await request(path);
    assert.equal(response.status, 302, path);
    assert.equal(response.headers.get("Location"), destination);
  }
  assert.ok(requests.every((entry) => entry.method === "GET"));
});

test("fixture practice and review remain available, with demo homepage entry", async () => {
  fixture = true;
  for (const path of [`/practice/${lesson.id}`, `/review/${lesson.id}`]) {
    const response = await request(path);
    assert.equal(response.status, 200, path);
    assert.match(await response.text(), /je voudrais/i);
  }
  const response = await request("/");
  assert.equal(response.status, 200);
  assert.match(
    await response.text(),
    new RegExp(`href="/review/${lesson.id}"`),
  );
});

test("anonymous production homepage leads to account review login", async () => {
  fixture = false;
  const response = await request("/");
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.match(html, /href="\/login\?next=\/reviews"/);
  assert.doesNotMatch(html, /href="\/review\//);
});

test("signed-in legacy review redirects to the private queue and forwards only its session cookie", async () => {
  fixture = false;
  authenticated = true;
  requests.length = 0;
  try {
    const legacy = await request(`/review/${lesson.id}`);
    assert.equal(legacy.status, 302);
    assert.equal(legacy.headers.get("Location"), "/reviews");
    const queue = await request("/reviews");
    assert.equal(queue.status, 200);
    assert.ok((await queue.text()).includes("复习"));
    const privateRead = requests.find(
      (entry) => entry.path === "/api/v1/me/reviews",
    );
    assert.equal(privateRead?.cookie, "brioche.sid=controlled-ssr-session");
    assert.ok(requests.every((entry) => entry.method === "GET"));
  } finally {
    authenticated = false;
  }
});

test("exercise referenced by explore uses the real learning entry in production", async () => {
  const originalSteps = lesson.steps;
  const exercise = lesson.blocks.find((block) => block.type === "exercise");
  lesson.steps = [
    ...originalSteps,
    {
      id: "explore-exercise",
      kind: "explore",
      titleZh: "表达练习",
      blockIds: [exercise.id],
    },
  ];
  requests.length = 0;
  try {
    fixture = false;
    let response = await request(`/lessons/${lesson.id}`);
    assert.equal(response.status, 200);
    let html = await response.text();
    assert.ok(
      !html.includes(`href="/practice/${lesson.id}"`),
      "production exercise must not link back through the legacy redirect",
    );
    assert.ok(
      html.includes(encodeURIComponent(`/lessons/${lesson.id}`)),
      "anonymous entry preserves the lesson after login",
    );
    authenticated = true;
    response = await request(`/lessons/${lesson.id}`);
    assert.equal(response.status, 200);
    html = await response.text();
    assert.ok(!html.includes(`href="/practice/${lesson.id}"`));
    assert.ok(html.includes("开始或继续学习"));
    authenticated = false;
    fixture = true;
    response = await request(`/lessons/${lesson.id}`);
    assert.equal(response.status, 200);
    assert.ok(
      (await response.text()).includes(`href="/practice/${lesson.id}"`),
    );
    assert.ok(requests.every((entry) => entry.method === "GET"));
  } finally {
    lesson.steps = originalSteps;
    authenticated = false;
    fixture = false;
  }
});

test("pending token SSR authorizes and forwards only kind and cursor", async () => {
  authenticated = false;
  requests.length = 0;
  assert.equal((await request("/admin/tokens")).status, 401);
  assert.ok(!requests.some((r) => r.path.includes("pending-tokens")));
  authenticated = true;
  profile.role = "learner";
  requests.length = 0;
  assert.equal((await request("/admin/tokens")).status, 403);
  assert.ok(!requests.some((r) => r.path.includes("pending-tokens")));
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/tokens?kind=invite&afterId=" + "b".repeat(64) + "&token=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /no-store/);
    const read = requests.find((r) => r.path.includes("pending-tokens"));
    assert.ok(read);
    const query = new URL(read.path, "http://test").searchParams;
    assert.equal(query.get("kind"), "invite");
    assert.equal(query.get("afterId"), "b".repeat(64));
    assert.equal(query.has("token"), false);
    assert.ok(!requests.some((r) => r.method !== "GET"));
  } finally {
    profile.role = "learner";
    authenticated = false;
  }
});

test("character library SSR authorizes before reading private voice profiles", async () => {
  authenticated = false;
  requests.length = 0;
  assert.equal((await request("/admin/characters")).status, 401);
  assert.ok(
    !requests.some((r) => r.path.startsWith("/api/v1/operator/characters")),
  );
  authenticated = true;
  profile.role = "learner";
  requests.length = 0;
  assert.equal((await request("/admin/characters")).status, 403);
  assert.ok(
    !requests.some((r) => r.path.startsWith("/api/v1/operator/characters")),
  );
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/characters?afterId=character-camille&secret=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /no-store/);
    assert.ok(
      requests.some(
        (r) =>
          r.path === "/api/v1/operator/characters?afterId=character-camille",
      ),
    );
    assert.ok(!requests.some((r) => r.method !== "GET"));
  } finally {
    profile.role = "learner";
    authenticated = false;
  }
});

test("visual registry SSR protects metadata and allowlists the composite cursor", async () => {
  authenticated = false;
  requests.length = 0;
  assert.equal((await request("/admin/assets")).status, 401);
  assert.ok(
    !requests.some((r) => r.path.startsWith("/api/v1/operator/assets")),
  );
  authenticated = true;
  profile.role = "learner";
  requests.length = 0;
  assert.equal((await request("/admin/assets")).status, 403);
  assert.ok(
    !requests.some((r) => r.path.startsWith("/api/v1/operator/assets")),
  );
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/assets?afterId=art-bakery&afterRevision=20&q=baguette&file=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /no-store/);
    assert.match(await response.text(), /图片素材/);
    assert.ok(
      requests.some(
        (r) =>
          r.path ===
          "/api/v1/operator/assets?afterId=art-bakery&afterRevision=20&q=baguette",
      ),
    );
    assert.ok(!requests.some((r) => r.method !== "GET"));
  } finally {
    profile.role = "learner";
    authenticated = false;
  }
});

test("recording registry SSR authorizes before fetching and keeps its cursor private", async () => {
  authenticated = false;
  requests.length = 0;
  assert.equal((await request("/admin/recordings")).status, 401);
  assert.ok(
    !requests.some((r) => r.path.startsWith("/api/v1/operator/recordings")),
  );
  authenticated = true;
  profile.role = "learner";
  requests.length = 0;
  assert.equal((await request("/admin/recordings")).status, 403);
  assert.ok(
    !requests.some((r) => r.path.startsWith("/api/v1/operator/recordings")),
  );
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/recordings?afterId=qa-recording&afterRevision=20&q=bonjour&file=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /private, no-store/);
    assert.match(await response.text(), /录音管理/);
    assert.ok(
      requests.some(
        (r) =>
          r.path ===
          "/api/v1/operator/recordings?afterId=qa-recording&afterRevision=20&q=bonjour",
      ),
    );
    assert.ok(!requests.some((r) => r.method !== "GET"));
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("reference delivery SSR authorizes before fetching and never restores bearer URLs", async () => {
  authenticated = false;
  requests.length = 0;
  assert.equal((await request("/admin/voice-references")).status, 401);
  assert.ok(
    !requests.some((r) =>
      r.path.startsWith("/api/v1/operator/voice-references"),
    ),
  );
  authenticated = true;
  profile.role = "learner";
  requests.length = 0;
  assert.equal((await request("/admin/voice-references")).status, 403);
  assert.ok(
    !requests.some((r) =>
      r.path.startsWith("/api/v1/operator/voice-references"),
    ),
  );
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/voice-references?afterId=" +
        "c".repeat(32) +
        "&token=discard&path=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /private, no-store/);
    const html = await response.text();
    assert.match(html, /参考录音交付/);
    assert.doesNotMatch(html, /reference-delivery-url/);
    assert.ok(
      requests.some(
        (r) =>
          r.path ===
          "/api/v1/operator/voice-references?afterId=" + "c".repeat(32),
      ),
    );
    assert.ok(!requests.some((r) => r.method !== "GET"));
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("voice jobs SSR is operator-only and never starts provider work", async () => {
  authenticated = false;
  requests.length = 0;
  assert.equal((await request("/admin/voice-jobs")).status, 401);
  assert.ok(
    !requests.some((r) => r.path.startsWith("/api/v1/operator/voice-jobs")),
  );
  authenticated = true;
  profile.role = "learner";
  requests.length = 0;
  assert.equal((await request("/admin/voice-jobs")).status, 403);
  assert.ok(
    !requests.some((r) => r.path.startsWith("/api/v1/operator/voice-jobs")),
  );
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/voice-jobs?afterId=" +
        "a".repeat(32) +
        "&token=discard&url=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /private, no-store/);
    assert.match(await response.text(), /音色创建任务/);
    assert.ok(
      requests.some(
        (r) =>
          r.path === "/api/v1/operator/voice-jobs?afterId=" + "a".repeat(32),
      ),
    );
    assert.ok(requests.every((r) => r.method === "GET"));
    assert.equal(
      (await request("/admin/voice-jobs?jobId=invalid")).status,
      400,
    );
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("audition SSR requires current operator identity, allowlists cursors and never generates audio", async () => {
  authenticated = false;
  requests.length = 0;
  assert.equal((await request("/admin/voice-auditions")).status, 401);
  assert.ok(
    !requests.some((r) =>
      r.path.startsWith("/api/v1/operator/voice-auditions"),
    ),
  );
  authenticated = true;
  profile.role = "learner";
  requests.length = 0;
  assert.equal((await request("/admin/voice-auditions")).status, 403);
  assert.ok(
    !requests.some((r) =>
      r.path.startsWith("/api/v1/operator/voice-auditions"),
    ),
  );
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/voice-auditions?afterId=" +
        "a".repeat(32) +
        "&token=discard&url=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /private, no-store/);
    assert.match(await response.text(), /角色声音试听/);
    assert.ok(
      requests.some(
        (r) =>
          r.path ===
          "/api/v1/operator/voice-auditions?afterId=" + "a".repeat(32),
      ),
    );
    assert.ok(requests.every((r) => r.method === "GET"));
    for (const query of [
      "jobId=invalid",
      "auditionId=invalid",
      "characterId=character-camille",
      "characterRevision=1",
      "characterId=character-camille&characterRevision=0",
      "characterId=character-camille&characterRevision=2147483648",
      "jobId=" +
        "a".repeat(32) +
        "&characterId=character-camille&characterRevision=1",
    ])
      assert.equal(
        (await request("/admin/voice-auditions?" + query)).status,
        400,
      );
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("system audition SSR loads a fixed character without any voice and never sends a paid request", async () => {
  authenticated = true;
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/voice-auditions?characterId=character-camille&characterRevision=1&secret=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /private, no-store/);
    assert.match(await response.text(), /Camille/);
    assert.ok(
      requests.some(
        (r) => r.path === "/api/v1/operator/characters/character-camille/1",
      ),
    );
    assert.ok(
      requests.some(
        (r) =>
          r.path ===
          "/api/v1/operator/voice-auditions?characterId=character-camille&characterRevision=1",
      ),
    );
    assert.ok(
      requests.every((r) => r.method === "GET" && !r.path.includes("secret=")),
    );
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("fixed character SSR works without a voice profile and bounds revisions", async () => {
  authenticated = true;
  profile.role = "operator";
  try {
    requests.length = 0;
    const response = await request(
      "/admin/characters?characterId=character-camille&characterRevision=1&secret=discard",
    );
    assert.equal(response.status, 200);
    assert.match(response.headers.get("cache-control"), /no-store/);
    assert.match(await response.text(), /Camille/);
    assert.ok(
      requests.some(
        (r) => r.path === "/api/v1/operator/characters/character-camille/1",
      ),
    );
    assert.ok(!requests.some((r) => r.path.includes("secret=")));
    assert.equal(
      (
        await request(
          "/admin/characters?characterId=character-camille&characterRevision=2147483648",
        )
      ).status,
      400,
    );
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("public lesson SSR reads v2 and renders native Cantonese words and pronunciation without legacy fields", async () => {
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../../crates/server/tests/fixtures/neutral-cantonese.lesson.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  neutralPublicLesson = Object.fromEntries(
    [...fields, "targetLanguage", "explanationLanguage"].map((key) => [
      key,
      source[key],
    ]),
  );
  neutralPublicLesson.media = [];
  fixture = false;
  authenticated = false;
  requests.length = 0;
  try {
    const response = await request(`/lessons/${source.id}`);
    assert.equal(response.status, 200);
    const html = await response.text();
    assert.ok(html.includes('class="sentence" lang="yue-Hant-HK"'));
    assert.ok(html.includes("<rt>nei5 hou2</rt>"));
    assert.ok(html.includes("<ruby>你好"));
    assert.ok(!html.includes("[object Object]"));
    assert.ok(!html.includes('role="tab"'));
    assert.ok(requests.some((r) => r.path === `/api/v2/lessons/${source.id}`));
    assert.ok(requests.some((r) => r.path === "/api/v2/catalog"));
    assert.ok(!requests.some((r) => r.path.startsWith("/api/lessons/")));
  } finally {
    neutralPublicLesson = null;
  }
});

test("native learning SSR uses the v2 session and forwards only its authorized owner cookie", async () => {
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../../crates/server/tests/fixtures/neutral-cantonese.lesson.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const native = Object.fromEntries(
    [...fields, "targetLanguage", "explanationLanguage"].map((key) => [
      key,
      source[key],
    ]),
  );
  native.media = [];
  neutralPrivateSession = {
    lesson: native,
    progress: {
      id: "native-session",
      lessonId: source.id,
      revision: 1,
      version: 1,
      lastStepId: "step-read",
      confirmedStepIds: [],
      hintedExerciseIds: [],
      attempts: [],
      completedAt: null,
      firstCompletedAt: null,
    },
  };
  fixture = false;
  authenticated = true;
  requests.length = 0;
  try {
    const response = await request("/learning/native-session");
    assert.equal(response.status, 200);
    const html = await response.text();
    assert.ok(html.includes('class="sentence" lang="yue-Hant-HK"'));
    assert.ok(html.includes("<rt>nei5 hou2</rt>"));
    const read = requests.find(
      (r) => r.path === "/api/v2/learning-sessions/native-session",
    );
    assert.equal(read?.cookie, "brioche.sid=controlled-ssr-session");
    assert.ok(
      !requests.some((r) => r.path.startsWith("/api/v1/learning-sessions")),
    );
    authenticated = false;
    const denied = await request("/learning/native-session");
    assert.equal(denied.status, 302);
    assert.equal(
      denied.headers.get("location"),
      "/login?next=%2Flearning%2Fnative-session",
    );
  } finally {
    authenticated = false;
    neutralPrivateSession = null;
  }
});

test("single-body reading has no redundant mode selector", async () => {
  const originalBlocks = lesson.blocks;
  const body = lesson.blocks.find((block) => block.type === "dialogue");
  lesson.blocks = originalBlocks.filter(
    (block) => !["dialogue", "article"].includes(block.type) || block === body,
  );
  try {
    const response = await request(`/lessons/${lesson.id}`);
    assert.equal(response.status, 200);
    const html = await response.text();
    assert.ok(!html.includes('role="tab"'));
    assert.ok(!html.includes('role="tabpanel"'));
    assert.ok(html.includes("Bonjour"));
  } finally {
    lesson.blocks = originalBlocks;
  }
});

test("public reading exposes every body, including multiple dialogues", async () => {
  const originalBlocks = lesson.blocks;
  const originalSteps = lesson.steps;
  const dialogue = structuredClone(
    lesson.blocks.find((block) => block.type === "dialogue"),
  );
  dialogue.id = "dialogue-second";
  dialogue.titleZh = "第二段对话";
  dialogue.turns = dialogue.turns.map((turn, index) => ({
    ...turn,
    id: "second-turn-" + index,
    segments: turn.segments.map((segment, segmentIndex) => ({
      ...segment,
      id: `second-segment-${index}-${segmentIndex}`,
    })),
  }));
  lesson.blocks = [...originalBlocks, dialogue];
  lesson.steps = [
    ...originalSteps,
    {
      id: "read-second",
      kind: "read",
      titleZh: "接着读",
      blockIds: [dialogue.id],
    },
  ];
  fixture = true;
  try {
    const response = await request(`/lessons/${lesson.id}`);
    assert.equal(response.status, 200);
    const html = await response.text();
    const bodyCount = lesson.blocks.filter((block) =>
      ["dialogue", "article"].includes(block.type),
    ).length;
    assert.equal((html.match(/role="tab"/g) ?? []).length, bodyCount);
    assert.ok(html.includes("第二段对话"));
  } finally {
    lesson.blocks = originalBlocks;
    lesson.steps = originalSteps;
    fixture = false;
  }
});

test("course speech plan SSR authorizes fixed versions without generating or saving", async () => {
  fixture = false;
  authenticated = false;
  requests.length = 0;
  const path = `/admin/speech-plans?lessonId=${lesson.id}&revision=1`;
  try {
    assert.equal((await request(path)).status, 401);
    assert.equal(
      requests.some((r) => r.path.includes("/operator/")),
      false,
    );
    authenticated = true;
    profile.role = "learner";
    requests.length = 0;
    assert.equal((await request(path)).status, 403);
    assert.equal(
      requests.some((r) => r.path.includes("/operator/")),
      false,
    );
    profile.role = "operator";
    requests.length = 0;
    const valid = await request(path);
    assert.equal(valid.status, 200);
    assert.equal(valid.headers.get("Cache-Control"), "private, no-store");
    const html = await valid.text();
    assert.ok(html.includes("课程配音"));
    assert.ok(html.includes("尚无声音档案"));
    assert.equal(
      requests.filter((r) => r.path.includes("/operator/")).length,
      2,
    );
    assert.ok(requests.every((r) => r.method === "GET"));
    for (const invalid of [
      "revision=0",
      "revision=2147483648",
      "revision=1&planId=invalid",
      "revision=1&afterId=invalid",
    ]) {
      requests.length = 0;
      assert.equal(
        (await request(`/admin/speech-plans?lessonId=${lesson.id}&${invalid}`))
          .status,
        400,
      );
      assert.equal(
        requests.some((r) => r.path.includes("/operator/")),
        false,
      );
    }
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("course speech clips deny visitors and learners before private reads, and reject malformed plan ids", async () => {
  authenticated = false;
  fixture = false;
  requests.length = 0;
  try {
    assert.equal(
      (await request("/admin/speech-clips?planId=" + "f".repeat(32))).status,
      401,
    );
    assert.ok(!requests.some((r) => r.path.includes("/operator/")));
    authenticated = true;
    profile.role = "learner";
    requests.length = 0;
    assert.equal(
      (await request("/admin/speech-clips?planId=" + "f".repeat(32))).status,
      403,
    );
    assert.ok(!requests.some((r) => r.path.includes("/operator/")));
    profile.role = "operator";
    for (const id of ["", "../escape", "f".repeat(33)]) {
      requests.length = 0;
      assert.equal(
        (await request("/admin/speech-clips?planId=" + encodeURIComponent(id)))
          .status,
        400,
      );
      assert.ok(!requests.some((r) => r.path.includes("/operator/")));
    }
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("speech alignments deny visitors and learners before private reads, and reject malformed plan ids", async () => {
  authenticated = false;
  fixture = false;
  requests.length = 0;
  try {
    assert.equal(
      (await request("/admin/speech-alignments?planId=" + "f".repeat(32)))
        .status,
      401,
    );
    assert.ok(!requests.some((r) => r.path.includes("/operator/")));
    authenticated = true;
    profile.role = "learner";
    requests.length = 0;
    assert.equal(
      (await request("/admin/speech-alignments?planId=" + "f".repeat(32)))
        .status,
      403,
    );
    assert.ok(!requests.some((r) => r.path.includes("/operator/")));
    profile.role = "operator";
    for (const id of ["", "../escape", "f".repeat(33)]) {
      requests.length = 0;
      assert.equal(
        (
          await request(
            "/admin/speech-alignments?planId=" + encodeURIComponent(id),
          )
        ).status,
        400,
      );
      assert.ok(!requests.some((r) => r.path.includes("/operator/")));
    }
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});

test("admin SSR authorizes search and allowlists both version cursors without writes", async () => {
  requests.length = 0;
  authenticated = false;
  fixture = false;
  const path =
    "/admin?tab=releases&q=100%25_%27&lessonAfterId=a1-bakery&lessonAfterRevision=2&releaseAfterId=catalog-old&ignored=private";
  try {
    assert.equal((await request(path)).status, 401);
    authenticated = true;
    profile.role = "learner";
    assert.equal((await request(path)).status, 403);
    assert.equal(
      requests.filter((r) => r.path.startsWith("/api/v1/operator/overview"))
        .length,
      0,
    );
    profile.role = "operator";
    requests.length = 0;
    const result = await request(path);
    assert.equal(result.status, 200);
    assert.match(result.headers.get("Cache-Control"), /private, no-store/);
    assert.match(await result.text(), /没有匹配的发布目录/);
    const calls = requests.filter((r) =>
      r.path.startsWith("/api/v1/operator/overview"),
    );
    assert.equal(calls.length, 1);
    assert.equal(calls[0].method, "GET");
    const query = new URL(calls[0].path, "http://test").searchParams;
    assert.deepEqual(
      [...query.keys()].sort(),
      [
        "lessonAfterId",
        "lessonAfterRevision",
        "releaseAfterId",
        "releaseQ",
      ].sort(),
    );
    assert.equal(query.get("releaseQ"), "100%_'");
    assert.equal(query.get("lessonAfterRevision"), "2");
  } finally {
    authenticated = false;
    profile.role = "learner";
  }
});
