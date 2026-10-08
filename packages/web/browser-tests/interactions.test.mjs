import { productWebUrl, browserCliUrl } from "../test-product.mjs";
import { before, beforeEach, after, afterEach, test } from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { createServer } from "vite";
import tailwindcss from "@tailwindcss/vite";

const execute = promisify(execFile);
const cli = fileURLToPath(browserCliUrl());
const session = "brioche-regression-" + randomUUID();
let server, origin;
let browserOpened = false;
async function browser(...args) {
  const { stdout } = await execute(
    process.execPath,
    [cli, "--session", session, "--json", ...args],
    { timeout: 30000, maxBuffer: 2 * 1024 * 1024 },
  );
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, result.error ?? "browser command failed");
  return result.data;
}
async function evaluate(code) {
  return (await browser("eval", code)).result;
}
const press = (key) => browser("press", key);
async function open(kind = "start") {
  await browser("open", origin + "/?case=" + kind);
  browserOpened = true;
  await browser(
    "wait",
    "--fn",
    "!!window.qa?.ready && !!document.querySelector('main')",
  );
}
before(async () => {
  server = await createServer({
    configFile: false,
    resolve: {
      alias: { "@chef/product": fileURLToPath(productWebUrl("product.ts")) },
      dedupe: ["react", "react-dom", "react-router"],
    },
    root: fileURLToPath(new URL(".", import.meta.url)),
    plugins: [
      tailwindcss(),
      {
        name: "qa-recording",
        configureServer(server) {
          // Real decoded PCM audio, deliberately silent and never a pronunciation fixture.
          const samples = 24000 * 30;
          const wav = Buffer.alloc(44 + samples * 2);
          wav.write("RIFF", 0);
          wav.writeUInt32LE(wav.length - 8, 4);
          wav.write("WAVEfmt ", 8);
          wav.writeUInt32LE(16, 16);
          wav.writeUInt16LE(1, 20);
          wav.writeUInt16LE(1, 22);
          wav.writeUInt32LE(24000, 24);
          wav.writeUInt32LE(48000, 28);
          wav.writeUInt16LE(2, 32);
          wav.writeUInt16LE(16, 34);
          wav.write("data", 36);
          wav.writeUInt32LE(samples * 2, 40);
          server.middlewares.use((request, response, next) => {
            if (!/^\/api\/audio\/[123]{64}\.wav$/.test(request.url ?? ""))
              return next();
            response.setHeader("Content-Type", "audio/wav");
            response.setHeader("Content-Length", wav.length);
            response.end(wav);
          });
        },
      },
    ],
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0 },
  });
  await server.listen();
  origin = "http://127.0.0.1:" + server.httpServer.address().port;
});
after(async () => {
  try {
    if (browserOpened) await browser("close");
  } finally {
    await server?.close();
  }
});
beforeEach(async () => {
  // Keep drafts across navigation within a test, never across independent tests.
  if (browserOpened) await evaluate("sessionStorage.clear()");
});
afterEach(async () => {
  if (!browserOpened) return;
  const { errors } = await browser("errors");
  await browser("errors", "--clear");
  assert.deepEqual(
    errors,
    [],
    "uncaught page errors must fail the interaction regression",
  );
});

test("native Cantonese reading uses one authored word, Jyutping and measured playback across viewport widths", async () => {
  await open("reading-neutral");
  for (const width of [320, 390, 678, 1280]) {
    await browser("set", "viewport", String(width), "844");
    assert.deepEqual(
      await evaluate(
        "({lang:document.querySelector('.sentence').lang,words:document.querySelectorAll('.sentence .word').length,text:document.querySelector('.sentence ruby').firstChild.textContent,jyutping:document.querySelector('.sentence rt').textContent,tabs:document.querySelectorAll('[role=tab]').length,overflow:document.documentElement.scrollWidth>innerWidth})",
      ),
      {
        lang: "yue-Hant-HK",
        words: 1,
        text: "你好",
        jyutping: "nei5 hou2",
        tabs: 0,
        overflow: false,
      },
    );
    await browser("click", ".sentence .word");
    await browser(
      "wait",
      "--fn",
      "qa.playback==='playing' && !!document.querySelector('.knowledge h2 ruby')",
    );
    assert.equal(
      await evaluate("document.querySelector('.knowledge h2 rt').textContent"),
      "nei5 hou2",
    );
    assert.equal(
      await evaluate("qa.playbackId"),
      "neutral-protocol:1:reading:paragraph-greeting:word:segment-greeting:0:2",
    );
    assert.deepEqual(await evaluate("qa.spoken"), []);
    await browser("click", ".knowledge .note-close");
  }
});

test("direct audio publication does not assert hearing and freezes exact retry across uncertain results", async () => {
  await open("audio-publication");
  await browser(
    "find",
    "label",
    "发布说明",
    "fill",
    "Owner publication from shared admin",
  );
  await browser(
    "find",
    "role",
    "button",
    "click",
    "--name",
    "授权直接发布",
    "--exact",
  );
  await browser("wait", "--fn", "qa.previewWrites.length===1");
  assert.deepEqual(await evaluate("qa.previewWrites[0].body"), {
    expectedLessonHash: "a".repeat(64),
    reason: "Owner publication from shared admin",
    evidence: { source: "admin-web", humanListeningAsserted: false },
  });
  assert.equal(
    await evaluate("qa.previewWrites[0].path"),
    "/api/v1/operator/lessons/qa-audio/revisions/7/direct-publication",
  );
  await evaluate("qa.previewWrites[0].release(503)");
  await browser("wait", "--text", "核对同一请求");
  assert.equal(
    await evaluate(
      "document.querySelector('.lesson-audio-review textarea').matches(':disabled')",
    ),
    true,
  );
  await browser("click", ".audio-exit");
  await browser("wait", "dialog[open]");
  await browser("find", "role", "button", "click", "--name", "留在当前页");
  await browser(
    "find",
    "role",
    "button",
    "click",
    "--name",
    "核对同一请求",
    "--exact",
  );
  await browser("wait", "--fn", "qa.previewWrites.length===2");
  assert.equal(
    await evaluate(
      "JSON.stringify(qa.previewWrites[0].body)===JSON.stringify(qa.previewWrites[1].body)",
    ),
    true,
  );
  await evaluate(
    "qa.previewWrites[1].release({required:true,published:false,lessonHash:'a'.repeat(64),version:2,accepted:true,directAuthorized:true,reason:'Owner publication',actor:'user:1'})",
  );
  await browser("wait", "--text", "已保存直接发布授权");
  assert.equal(
    await evaluate(
      "document.querySelector('.lesson-audio-review .primary').disabled",
    ),
    true,
  );
  await browser("click", ".audio-exit");
  await browser("wait", "--text", "上一页");
});

test("optional human listening keeps its separate review endpoint and explicit declaration", async () => {
  await open("audio-publication");
  await browser(
    "find",
    "label",
    "发布说明",
    "fill",
    "Actual synthetic reviewer protocol",
  );
  await browser("click", ".admin-optional-review summary");
  await browser("check", ".admin-optional-review input[type=checkbox]");
  await browser(
    "find",
    "role",
    "button",
    "click",
    "--name",
    "保存试听通过记录",
    "--exact",
  );
  await browser("wait", "--fn", "qa.previewWrites.length===1");
  assert.equal(
    await evaluate("qa.previewWrites[0].path.endsWith('/audio-review')"),
    true,
  );
  assert.deepEqual(await evaluate("qa.previewWrites[0].body"), {
    expectedLessonHash: "a".repeat(64),
    version: 2,
    accepted: true,
    heard: true,
    reason: "Actual synthetic reviewer protocol",
  });
  await evaluate("qa.previewWrites[0].release(409)");
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.lesson-audio-review textarea').matches(':disabled')",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.lesson-audio-review .primary').textContent",
    ),
    "授权直接发布",
  );
});

test("learning entry keeps focus, deduplicates keyboard submits, and returns through login", async () => {
  await open();
  await browser("focus", ".start-learning button");
  await press("Enter");
  await press("Enter");
  await press("Enter");
  assert.deepEqual(
    await evaluate(
      "({focus:document.activeElement.tagName,count:qa.writes.length,busy:document.activeElement.getAttribute('aria-busy')})",
    ),
    { focus: "BUTTON", count: 1, busy: "true" },
  );
  assert.equal(await evaluate("qa.writes[0].schemaVersion"), "2.0");
  await evaluate("qa.release[0](401)");
  await browser("wait", ".start-learning a");
  await press("Tab");
  await press("Enter");
  assert.deepEqual(await evaluate("({route:qa.route,search:qa.search})"), {
    route: "/login",
    search: "?next=%2Flessons%2Fcourse-a",
  });
});

test("native Cantonese session confirms steps, hints and all exercise kinds through v2 before completion", async () => {
  await open("session-neutral");
  await browser("set", "viewport", "390", "844");
  let state = {
    id: "qa-session",
    lessonId: "neutral-protocol",
    revision: 1,
    version: 1,
    lastStepId: "step-read",
    confirmedStepIds: [],
    hintedExerciseIds: [],
    attempts: [],
    completedAt: null,
    firstCompletedAt: null,
  };
  let index = 0;
  assert.equal(
    await evaluate("document.querySelector('.lesson-header h1').lang"),
    "yue-Hant-HK",
  );
  async function confirm(step) {
    await browser("click", ".learning-actions .primary");
    await browser("wait", "--fn", `qa.learningWrites.length===${index + 1}`);
    assert.equal(
      await evaluate(`qa.learningPaths[${index}].path`),
      `/api/v2/learning-sessions/qa-session/steps/${step}`,
    );
    assert.equal(
      await evaluate(`qa.learningWrites[${index}].version`),
      state.version,
    );
    state = {
      ...state,
      version: state.version + 1,
      lastStepId: step,
      confirmedStepIds: [...state.confirmedStepIds, step],
    };
    await evaluate(`qa.learningRelease[${index++}](${JSON.stringify(state)})`);
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('.learning-actions .primary')?.getAttribute('aria-busy') || document.querySelector('.learning-actions .primary')?.getAttribute('aria-busy')==='false'",
    );
  }
  await confirm("step-read");
  await confirm("step-meaning");
  await browser(
    "wait",
    "--fn",
    "document.querySelectorAll('form.exercise-sheet').length===3",
  );
  assert.equal(
    await evaluate("document.querySelector('.practice-input').lang"),
    "yue-Hant-HK",
  );
  assert.equal(
    await evaluate("document.querySelector('.practice-sentence').textContent"),
    "____",
  );
  await browser("click", ".practice-hint");
  await browser("wait", "--fn", `qa.learningWrites.length===${index + 1}`);
  assert.equal(
    await evaluate(`qa.learningPaths[${index}].path`),
    "/api/v2/learning-sessions/qa-session/hints/text",
  );
  state = { ...state, version: state.version + 1, hintedExerciseIds: ["text"] };
  await evaluate(
    `qa.learningRelease[${index++}](${JSON.stringify({ progress: state, hintZh: "仅协议测试。" })})`,
  );
  await browser("wait", "--text", "仅协议测试。");
  const answers = [
    { kind: "choice", optionId: "greeting" },
    { kind: "text", text: "點心" },
    { kind: "order", tokenIds: ["first", "bye", "second"] },
  ];
  await browser("check", 'input[value="greeting"]');
  await browser("fill", ".practice-input", "點心");
  for (let n = 0; n < 3; n++) {
    await evaluate(
      `document.querySelector('.order-bank button:nth-child(${n + 1})').focus()`,
    );
    await press("Enter");
  }
  for (const [n, exerciseId] of ["choice", "text", "order"].entries()) {
    if (exerciseId === "text")
      assert.equal(
        await evaluate("document.querySelector('.practice-input').value"),
        "點心",
      );
    await evaluate(
      `document.querySelector('form.exercise-sheet:nth-of-type(${n + 1}) .primary').focus()`,
    );
    await press("Enter");
    try {
      await browser("wait", "--fn", `qa.learningWrites.length===${index + 1}`);
    } catch (error) {
      throw new Error(
        JSON.stringify(
          await evaluate(
            "({writes:qa.learningWrites,forms:[...document.querySelectorAll('form.exercise-sheet')].map(f=>({text:f.textContent,input:f.querySelector('input')?.value,disabled:f.querySelector('.primary')?.disabled})),messages:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent)})",
          ),
        ),
        { cause: error },
      );
    }
    assert.equal(
      await evaluate(`qa.learningPaths[${index}].path`),
      "/api/v2/learning-sessions/qa-session/attempts",
    );
    assert.deepEqual(
      await evaluate(`qa.learningWrites[${index}].answer`),
      answers[n],
    );
    const result = { correct: true, feedbackZh: "合成协议反馈。" };
    const attempt = {
      id: `attempt-${exerciseId}`,
      exerciseId,
      answer: answers[n],
      result,
      hintUsed: exerciseId === "text",
      attemptIndex: 1,
    };
    state = {
      ...state,
      version: state.version + 1,
      attempts: [...state.attempts, attempt],
    };
    await evaluate(
      `qa.learningRelease[${index++}](${JSON.stringify({ progress: state, result })})`,
    );
    await browser(
      "wait",
      "--fn",
      `document.querySelectorAll('.practice-feedback').length===${n + 1}`,
    );
  }
  await confirm("step-practice");
  await confirm("step-recap");
  await browser("click", ".learning-actions .primary");
  await browser("wait", "--fn", `qa.learningWrites.length===${index + 1}`);
  assert.equal(
    await evaluate(`qa.learningPaths[${index}].path`),
    "/api/v2/learning-sessions/qa-session/complete",
  );
  state = {
    ...state,
    version: state.version + 1,
    completedAt: "2026-10-08T00:00:00Z",
    firstCompletedAt: "2026-10-08T00:00:00Z",
  };
  await evaluate(`qa.learningRelease[${index}](${JSON.stringify(state)})`);
  await browser(
    "wait",
    "--fn",
    "document.activeElement.textContent==='本课已完成'",
  );
  assert.equal(
    await evaluate("document.documentElement.scrollWidth>innerWidth"),
    false,
  );
});

test("a restored v1 step keeps its original endpoint and advances in the v2 client", async () => {
  await open("session-multi");
  await evaluate("qa.navigate('/login')");
  await browser("wait", "--fn", "qa.route==='/login'");
  const job = {
    path: "/api/v1/learning-sessions/qa-session/steps/read",
    method: "PUT",
    body: { version: 1, idempotencyKey: "legacy-fixed-step-operation" },
  };
  await evaluate(
    `sessionStorage.setItem('brioche.learning.v1:qa-account:qa-session:1:pending',${JSON.stringify(JSON.stringify(job))});qa.navigate('/')`,
  );
  await browser("wait", "--text", "重试保存");
  await evaluate(
    "document.querySelector('.learning-actions .primary').focus()",
  );
  await press("Enter");
  try {
    await browser("wait", "--fn", "qa.learningWrites.length===1");
  } catch (error) {
    throw new Error(
      JSON.stringify(
        await evaluate(
          "({writes:qa.learningWrites,paths:qa.learningPaths,body:document.querySelector('.learning-actions')?.textContent,storage:sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending'),bootstrap:qa.authBootstraps.length})",
        ),
      ),
      { cause: error },
    );
  }
  assert.deepEqual(await evaluate("qa.learningPaths[0]"), {
    path: job.path,
    method: job.method,
  });
  assert.deepEqual(await evaluate("qa.learningWrites[0]"), job.body);
  await evaluate(
    "qa.learningRelease[0]({id:'qa-session',lessonId:'reading-protocol',revision:1,version:2,lastStepId:'read',confirmedStepIds:['read'],hintedExerciseIds:[],attempts:[],completedAt:null,firstCompletedAt:null})",
  );
  await browser("wait", "--fn", "document.activeElement.textContent==='回顾'");
  assert.equal(await evaluate("qa.learningWrites.length"), 1);
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending')",
    ),
    null,
  );
});

test("a replaced lesson ignores its late response while current retries keep their exact body", async () => {
  await open();
  await browser("focus", ".start-learning button");
  await press("Enter");
  await press("Tab");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('h1').textContent==='course-b'",
  );
  await browser("focus", ".start-learning button");
  await press("Enter");
  await evaluate("qa.release[0](200)");
  assert.deepEqual(
    await evaluate(
      "({route:qa.route,pending:document.querySelector('.primary').getAttribute('aria-busy'),lessons:qa.writes.map(w=>w.lessonId),separate:qa.writes[0].idempotencyKey!==qa.writes[1].idempotencyKey})",
    ),
    {
      route: "/",
      pending: "true",
      lessons: ["course-a", "course-b"],
      separate: true,
    },
  );
  await evaluate("qa.release[1](500)");
  await browser("wait", "[role=alert]");
  await press("Enter");
  assert.equal(
    await evaluate(
      "JSON.stringify(qa.writes[1])===JSON.stringify(qa.writes[2])",
    ),
    true,
  );
  await evaluate("qa.release[2](200)");
  await browser("wait", "--fn", "qa.route==='/learning/session-course-b'");
});

test("multiple reading bodies support keyboard scrolling, independent translations, and playback cancellation", async () => {
  await open("reading");
  await browser("set", "viewport", "320", "740");
  assert.equal(
    await evaluate("document.querySelectorAll('[role=tab]').length"),
    3,
  );
  await browser("focus", ".speaker");
  await press("Enter");
  await browser("wait", "--fn", "qa.playback==='playing'");
  assert.deepEqual(
    await evaluate(
      "({translation:document.querySelector('.translation')?.textContent ?? null,selected:document.querySelector('[aria-selected=true]').textContent.trim(),recording:qa.mediaPlays.at(-1)?.url.endsWith('/api/audio/'+'1'.repeat(64)+'.wav')})",
    ),
    { translation: "早上好！", selected: "早晨的问候", recording: true },
  );
  await browser("focus", "[role=tab]");
  await press("End");
  assert.deepEqual(
    await evaluate(
      "({first:document.querySelector('.sentence').textContent,playback:qa.playback,translation:!!document.querySelector('.translation'),visible:document.activeElement.getBoundingClientRect().right<=document.querySelector('.reading-tabs').getBoundingClientRect().right+1})",
    ),
    { first: "Bonsoir !", playback: "idle", translation: false, visible: true },
  );
  await browser("focus", ".speaker");
  await press("Enter");
  await browser("wait", "--fn", "qa.playback==='playing'");
  assert.equal(
    await evaluate(
      "qa.mediaPlays.at(-1).url.endsWith('/api/audio/'+'3'.repeat(64)+'.wav')",
    ),
    true,
  );
  assert.deepEqual(await evaluate("qa.spoken"), []);
  await browser("focus", "[aria-selected=true]");
  await press("ArrowLeft");
  assert.deepEqual(
    await evaluate(
      "({first:document.querySelector('.sentence').textContent,avatars:document.querySelectorAll('.speaker').length,playback:qa.playback})",
    ),
    { first: "Camille va à la boulangerie.", avatars: 0, playback: "idle" },
  );
  await press("Home");
  assert.deepEqual(
    await evaluate(
      "({translation:document.querySelector('.translation')?.textContent ?? null,selected:document.querySelector('[aria-selected=true]').textContent.trim(),focus:document.activeElement.getAttribute('role')})",
    ),
    { translation: "早上好！", selected: "早晨的问候", focus: "tab" },
  );
  for (const width of [320, 390, 900]) {
    await browser("set", "viewport", String(width), "844");
    assert.equal(
      await evaluate(
        "document.querySelector('.reading-tabs').getBoundingClientRect().right<=innerWidth",
      ),
      true,
    );
  }
});

test("late recording callbacks cannot interrupt newer playback; failures stop without browser speech and allow explicit retry", async () => {
  await open("reading");
  await browser("focus", ".speaker");
  await press("Enter");
  await browser("wait", "--fn", "qa.playback==='playing'");
  await evaluate("qa.staleEvents=qa.mediaEvents.slice()");
  await browser("focus", "[role=tab]");
  await press("End");
  await browser("focus", ".speaker");
  await press("Enter");
  await browser("wait", "--fn", "qa.playback==='playing'");
  const currentId = await evaluate("qa.playbackId");
  await evaluate(
    "qa.staleEvents.filter(e=>['playing','error','ended'].includes(e.name)).forEach(e=>e.callback(new Event(e.name)))",
  );
  assert.deepEqual(
    await evaluate(
      "({status:qa.playback,id:qa.playbackId,count:qa.mediaPlays.length,spoken:qa.spoken})",
    ),
    { status: "playing", id: currentId, count: 2, spoken: [] },
  );
  await evaluate("qa.media[0].dispatchEvent(new Event('error'))");
  await browser("wait", "--fn", "qa.playback==='idle'");
  assert.equal(
    await evaluate(
      "document.querySelector('.toast [role=status]').textContent",
    ),
    "录音暂时无法播放，请重试。",
  );
  assert.deepEqual(await evaluate("qa.spoken"), []);
  await press("Enter");
  await browser("wait", "--fn", "qa.playback==='playing'");
  assert.equal(await evaluate("qa.mediaPlays.length"), 3);
  assert.equal(await evaluate("qa.playbackId"), currentId);
  await evaluate("window.dispatchEvent(new Event('pagehide'))");
  await browser("wait", "--fn", "qa.playback==='idle'");
  assert.equal(
    await evaluate("qa.media.every(m=>m.paused&&!m.getAttribute('src'))"),
    true,
  );
});

test("missing sentence and word recordings show a toast without invoking browser speech or downloading audio", async () => {
  await open("reading&missing-recording=1");
  await browser("focus", ".speaker");
  await press("Enter");
  await browser("wait", "--fn", "!!document.querySelector('.translation')");
  assert.equal(
    await evaluate(
      "document.querySelector('.toast [role=status]').textContent",
    ),
    "这段录音还在准备中。",
  );
  await browser("focus", ".sentence .word");
  await press("Enter");
  assert.deepEqual(
    await evaluate(
      "({status:qa.playback,media:qa.media.length,spoken:qa.spoken})",
    ),
    { status: "idle", media: 0, spoken: [] },
  );
});

test("a partially recorded reading sequence does not begin a misleading incomplete playback", async () => {
  await open("reading&partial-recording=1");
  await evaluate(`
    qa.observedToast = null;
    const toast = document.querySelector('.toast');
    qa.toastObserver = new MutationObserver(() => {
      if (!toast.hidden) qa.observedToast = toast.querySelector('[role=status]').textContent;
    });
    qa.toastObserver.observe(toast, {
      attributes: true, attributeFilter: ['hidden'],
      childList: true, subtree: true, characterData: true
    });
  `);
  await browser("focus", ".playback-line");
  await press("Enter");
  assert.equal(await evaluate("qa.observedToast"), "这段录音还在准备中。");
  await evaluate("qa.toastObserver.disconnect()");
  assert.deepEqual(
    await evaluate(
      "({status:qa.playback,media:qa.media.length,spoken:qa.spoken})",
    ),
    { status: "idle", media: 0, spoken: [] },
  );
});

test("knowledge recordings play fixed intervals in teaching notes, saved cards and the review queue", async () => {
  for (const [kind, selector, expand] of [
    [
      "reading",
      ".vocabulary-list dt button",
      '.lesson-note[data-kind="vocabulary"] > summary',
    ],
    [
      "reading",
      ".grammar-example button",
      '.lesson-note[data-kind="grammar"] > summary',
    ],
    ["library", ".library-entry-heading", null],
    ["managed-library", ".library-entry-heading", null],
    ["reviews", ".review-flashcard", null],
  ]) {
    await open(kind + "&knowledge-recording=1");
    if (expand) {
      await browser("focus", expand);
      await press("Enter");
    }
    await browser("focus", selector);
    await press("Enter");
    await browser("wait", "--fn", "qa.mediaPlays.length===1");
    const clip = await evaluate("qa.mediaPlays[0]");
    assert.equal(
      new URL(clip.url).pathname,
      "/api/audio/" + "1".repeat(64) + ".wav",
    );
    assert.ok(Math.abs(clip.time - 1.2) < 0.02, JSON.stringify(clip));
    await browser(
      "wait",
      "--fn",
      "qa.media[0].paused && qa.media[0].currentTime>=1.8",
    );
    assert.deepEqual(await evaluate("qa.spoken"), []);
  }
});

test("native review and saved cards render authored Jyutping and reuse recorded playback", async () => {
  for (const [kind, selector] of [
    ["library-neutral", ".library-entry-heading"],
    ["reviews-neutral", ".review-flashcard"],
  ]) {
    await open(kind);
    await browser("set", "viewport", "390", "844");
    assert.ok(await evaluate("!!document.querySelector('ruby rt')"));
    assert.equal(
      await evaluate("document.querySelector('ruby rt').textContent"),
      "nei5 hou2",
    );
    assert.equal(
      await evaluate("document.body.textContent.includes('[object Object]')"),
      false,
    );
    await browser("focus", selector);
    await press("Enter");
    await browser("wait", "--fn", "qa.mediaPlays.length===1");
    assert.ok(Math.abs((await evaluate("qa.mediaPlays[0]")).time - 1.2) < 0.02);
    assert.deepEqual(await evaluate("qa.spoken"), []);
    assert.equal(
      await evaluate("document.documentElement.scrollWidth>innerWidth"),
      false,
    );
    if (kind === "reviews-neutral") {
      await browser("focus", ".review-ratings button:last-child");
      await press("Enter");
      await browser("wait", "--fn", "qa.reviewWrites.length===1");
      assert.equal(
        await evaluate("qa.reviewPaths[0]"),
        "/api/v2/me/reviews/qa-card/attempts",
      );
      await evaluate(
        "qa.reviewRelease[0]({card:{...qa.nativeReviewQueue.items[0],version:2},reviewedAt:'2026-10-08T00:00:00Z',timeZone:'Asia/Shanghai'})",
      );
      await browser("wait", "--text", "本轮回顾");
      assert.ok(
        await evaluate(
          "!!document.querySelector('.result-expression ruby rt')",
        ),
      );
    }
  }
});

test("recording pause and speed changes preserve the same audio position and explicit resume", async () => {
  await open("reading");
  await browser("focus", ".playback-line");
  await press("Enter");
  await browser("wait", "--fn", "qa.playback==='playing'");
  await press("Enter");
  await browser("wait", "--fn", "qa.playback==='paused'");
  const position = await evaluate("qa.media[0].currentTime");
  await press("Shift+F10");
  await browser("wait", "dialog[open]");
  await press("End");
  await press("Enter");
  assert.equal(await evaluate("qa.media[0].playbackRate"), 1.5);
  assert.equal(await evaluate("qa.media[0].currentTime"), position);
  assert.equal(await evaluate("qa.playback"), "paused");
  await browser("focus", ".playback-line");
  await press("Enter");
  await browser("wait", "--fn", "qa.playback==='playing'");
  assert.equal(await evaluate("qa.media.length"), 1);
  assert.deepEqual(await evaluate("qa.spoken"), []);
});

test("custom settings dialogs remain in the viewport and restore keyboard focus and search", async () => {
  await open("choices");
  const rate = 'button[aria-label^="朗读速度："]';
  for (const width of [320, 390, 900]) {
    await browser("set", "viewport", String(width), "700");
    await browser("focus", rate);
    await press("Enter");
    await browser("wait", "dialog[open]");
    assert.equal(
      await evaluate("document.activeElement.getAttribute('aria-checked')"),
      "true",
    );
    assert.equal(
      await evaluate(
        "(()=>{const r=document.querySelector('dialog[open]').getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight})()",
      ),
      true,
    );
    await press("End");
    assert.equal(
      await evaluate("document.activeElement.textContent.trim()"),
      "1.5×",
    );
    await press("Enter");
    assert.equal(
      await evaluate("document.querySelectorAll('dialog[open]').length"),
      0,
    );
    assert.equal(
      await evaluate(
        "document.activeElement.matches('button[aria-label^=\"朗读速度：\"]')",
      ),
      true,
    );
    await press("Enter");
    await press("Escape");
    assert.equal(
      await evaluate(
        "document.activeElement.matches('button[aria-label^=\"朗读速度：\"]')",
      ),
      true,
    );
  }
  const zone = 'button[aria-label^="时区："]';
  await browser("focus", zone);
  await press("Enter");
  await browser("fill", "dialog[open] input", "no-matching-city");
  assert.equal(
    await evaluate(
      "document.querySelectorAll('dialog[open] [role=radio]').length",
    ),
    0,
  );
  await press("Escape");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('dialog[open] input')?.value===''",
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('dialog[open] [role=radio]').length",
    ),
    2,
  );
  assert.equal(
    await evaluate("document.activeElement.matches('input[type=search]')"),
    true,
  );
  await browser("fill", "dialog[open] input", "Paris");
  await press("Tab");
  await press("Enter");
  assert.equal(
    await evaluate("document.activeElement.getAttribute('aria-label')"),
    "时区：巴黎",
  );
});

test("a profile identity change discards the previous editor and permits independent saves", async () => {
  await open("profile");
  const edit = 'button[aria-label="编辑个人资料"]';
  await browser("focus", edit);
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  await browser("fill", ".profile-dialog input", "Alice edited");
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.accountWrites.length===1");
  await evaluate("qa.changeUser()");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.profile-summary h2')?.textContent==='Bob'",
  );
  assert.equal(
    await evaluate("document.querySelectorAll('.profile-dialog[open]').length"),
    0,
  );
  await browser("focus", edit);
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  assert.equal(
    await evaluate("document.querySelector('.profile-dialog input').value"),
    "Bob",
  );
  await browser("fill", ".profile-dialog input", "Bob edited");
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.accountWrites.length===2");
  assert.equal(await evaluate("qa.accountWrites[1].expectedAccountVersion"), 3);
  await evaluate(
    "qa.accountRelease[0]({id:'account-a',email:'a@example.test',displayName:'Alice edited',role:'learner',version:2})",
  );
  assert.equal(
    await evaluate("document.querySelector('.profile-dialog input').value"),
    "Bob edited",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.profile-dialog button[type=submit]').getAttribute('aria-busy')",
    ),
    "true",
  );
  await evaluate(
    "qa.accountRelease[1]({id:'account-b',email:'b@example.test',displayName:'Bob edited',role:'operator',version:4})",
  );
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.profile-summary h2')?.textContent==='Bob edited' && !document.querySelector('.profile-dialog[open]')",
  );
});

test("closing a changed profile asks before discarding and preserves the draft when retained", async () => {
  await open("profile");
  const edit = 'button[aria-label="编辑个人资料"]';
  await browser("focus", edit);
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  await browser("fill", ".profile-dialog input", "Unsaved name");
  await press("Escape");
  assert.equal(
    await evaluate("document.querySelectorAll('.profile-dialog[open]').length"),
    1,
  );
  await browser("wait", ".profile-discard");
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "放弃这些修改？",
  );
  await browser("focus", ".profile-discard .primary");
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  assert.equal(
    await evaluate("document.querySelector('.profile-dialog input').value"),
    "Unsaved name",
  );
  assert.equal(
    await evaluate("document.activeElement.matches('.profile-dialog input')"),
    true,
  );
  assert.equal(await evaluate("qa.accountWrites.length"), 0);
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    true,
  );
  await browser("focus", 'button[aria-label="关闭个人资料"]');
  await press("Enter");
  await browser("focus", ".profile-discard .text-button");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.profile-dialog[open]')",
  );
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    false,
  );
  await browser("focus", edit);
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  assert.equal(
    await evaluate("document.querySelector('.profile-dialog input').value"),
    "Alice",
  );
  await press("Escape");
  assert.equal(
    await evaluate("document.querySelectorAll('.profile-dialog[open]').length"),
    0,
  );
});

test("profile drafts guard route pushes and history pops, including saves already in flight", async () => {
  await open("profile");
  const edit = 'button[aria-label="编辑个人资料"]';
  await browser("focus", edit);
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  await browser("fill", ".profile-dialog input", "Keep this draft");
  await evaluate("qa.navigate('/login')");
  await browser(
    "wait",
    "--fn",
    "qa.route==='/login'||!!document.querySelector('.profile-discard')",
  );
  assert.equal(await evaluate("qa.route"), "/");
  await browser("focus", ".profile-discard .primary");
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  assert.equal(
    await evaluate("document.querySelector('.profile-dialog input').value"),
    "Keep this draft",
  );
  await evaluate("qa.navigate(-1)");
  await browser("wait", ".profile-discard");
  await browser("focus", ".profile-discard .text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.route==='/previous'");
  assert.equal(await evaluate("qa.accountWrites.length"), 0);

  await open("profile");
  await browser("focus", edit);
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  await browser("fill", ".profile-dialog input", "Saved before leaving");
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.accountWrites.length===1");
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".profile-leave-status");
  assert.equal(
    await evaluate(
      "!!document.querySelector('.profile-dialog[open] .profile-leave-status')",
    ),
    true,
  );
  assert.equal(await evaluate("qa.route"), "/");
  await evaluate(
    "qa.accountRelease[0]({id:'account-a',email:'a@example.test',displayName:'Saved before leaving',role:'learner',version:2})",
  );
  await browser("wait", "--fn", "qa.route==='/login'");
  assert.equal(await evaluate("qa.accountWrites.length"), 1);
});

test("failed profile saves retain blocked drafts and require explicit version-aware recovery", async () => {
  const edit = 'button[aria-label="编辑个人资料"]';
  for (const status of [409, 503]) {
    await open("profile");
    await browser("focus", edit);
    await press("Enter");
    await browser("wait", ".profile-dialog[open] input");
    await browser("fill", ".profile-dialog input", "My draft");
    await browser("focus", ".profile-dialog button[type=submit]");
    await press("Enter");
    await browser("wait", "--fn", "qa.accountWrites.length===1");
    await evaluate("qa.navigate('/login')");
    await browser("wait", ".profile-dialog .profile-leave-status");
    await evaluate("qa.accountRelease[0](" + status + ")");
    await browser("wait", "--fn", "qa.accountReads.length===1");
    assert.equal(await evaluate("qa.route"), "/");
    await evaluate(
      "qa.accountReads[0]({id:'account-a',email:'a@example.test',displayName:" +
        JSON.stringify(status === 409 ? "Other device" : "My draft") +
        ",role:'learner',version:7})",
    );
    await browser("wait", ".profile-discard");
    assert.equal(
      await evaluate("document.activeElement.textContent"),
      "放弃这些修改？",
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.profile-discard p').textContent",
      ),
      "上次保存未获确认。离开将丢弃当前编辑草稿。",
    );
    assert.equal(await evaluate("qa.accountWrites.length"), 1);
    await press("Escape");
    assert.equal(
      await evaluate("document.querySelector('.profile-dialog input').value"),
      "My draft",
    );
    await browser("focus", ".profile-dialog button[type=submit]");
    await press("Enter");
    if (status === 409) {
      await browser("wait", "--fn", "qa.accountWrites.length===2");
      assert.deepEqual(await evaluate("qa.accountWrites[1]"), {
        displayName: "My draft",
        expectedAccountVersion: 7,
      });
      await evaluate(
        "qa.accountRelease[1]({id:'account-a',email:'a@example.test',displayName:'My draft',role:'learner',version:8})",
      );
    }
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('.profile-dialog[open]')",
    );
    assert.equal(await evaluate("qa.route"), "/");
    assert.equal(
      await evaluate("qa.accountWrites.length"),
      status === 409 ? 2 : 1,
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.profile-summary h2').textContent",
      ),
      "My draft",
    );
  }
});

test("unauthenticated recovery removes the old profile and releases its navigation guard", async () => {
  for (const status of [503, 401]) {
    await open("profile");
    await browser("focus", 'button[aria-label="编辑个人资料"]');
    await press("Enter");
    await browser("wait", ".profile-dialog[open] input");
    await browser("fill", ".profile-dialog input", "Private draft");
    await browser("focus", ".profile-dialog button[type=submit]");
    await press("Enter");
    await browser("wait", "--fn", "qa.accountWrites.length===1");
    await evaluate("qa.navigate('/login')");
    await browser("wait", ".profile-dialog .profile-leave-status");
    await evaluate("qa.accountRelease[0](" + status + ")");
    if (status === 503) {
      await browser("wait", "--fn", "qa.accountReads.length===1");
      await evaluate("qa.accountReads[0](401)");
    }
    await browser(
      "wait",
      "--fn",
      "!!document.querySelector('.profile-discard')||document.querySelector('.profile-summary h2')?.textContent==='法语学习者'",
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.profile-summary h2').textContent",
      ),
      "法语学习者",
    );
    assert.equal(
      await evaluate(
        "document.querySelectorAll('.profile-dialog[open]').length",
      ),
      0,
    );
    assert.equal(
      await evaluate("document.querySelectorAll('.profile-edit').length"),
      0,
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.profile-summary').textContent.includes('a@example.test')",
      ),
      false,
    );
    assert.equal(
      await evaluate("qa.accountReads.length"),
      status === 503 ? 1 : 0,
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.toast [role=status]').textContent",
      ),
      "登录已过期，请重新登录。",
    );
    await browser("focus", 'a.primary[href="/login"]');
    await press("Enter");
    await browser("wait", "--fn", "qa.route==='/login'");
    assert.equal(await evaluate("qa.accountWrites.length"), 1);
  }
});

test("failed recovery reads retain the profile draft and let the server resolve the old version on explicit retry", async () => {
  await open("profile");
  await browser("focus", 'button[aria-label="编辑个人资料"]');
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  await browser("fill", ".profile-dialog input", "Retained draft");
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.accountWrites.length===1");
  await evaluate("qa.navigate('/login');qa.accountRelease[0](503)");
  await browser("wait", "--fn", "qa.accountReads.length===1");
  await evaluate("qa.accountReads[0](503)");
  await browser("wait", ".profile-discard");
  assert.equal(
    await evaluate("document.querySelector('.profile-summary h2').textContent"),
    "Alice",
  );
  assert.equal(await evaluate("qa.route"), "/");
  assert.equal(await evaluate("qa.accountWrites.length"), 1);
  await press("Escape");
  assert.equal(
    await evaluate("document.querySelector('.profile-dialog input').value"),
    "Retained draft",
  );
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.accountWrites.length===2");
  assert.equal(await evaluate("qa.accountWrites[1].expectedAccountVersion"), 1);
  await evaluate("qa.accountRelease[1](409)");
  await browser("wait", "--fn", "qa.accountReads.length===2");
  await evaluate(
    "qa.accountReads[1]({id:'account-a',email:'a@example.test',displayName:'Another device',role:'learner',version:7})",
  );
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.profile-dialog button[type=submit]')?.getAttribute('aria-busy')==='false'",
  );
  assert.equal(
    await evaluate("document.activeElement.matches('.error-message')"),
    true,
  );
  assert.equal(
    await evaluate("document.querySelector('.profile-dialog input').value"),
    "Retained draft",
  );
  assert.equal(await evaluate("qa.accountWrites.length"), 2);
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.accountWrites.length===3");
  assert.equal(await evaluate("qa.accountWrites[2].expectedAccountVersion"), 7);
  await evaluate(
    "qa.accountRelease[2]({id:'account-a',email:'a@example.test',displayName:'Retained draft',role:'learner',version:8})",
  );
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.profile-dialog[open]')",
  );
  assert.equal(await evaluate("qa.route"), "/");
  assert.equal(await evaluate("qa.accountWrites.length"), 3);
});

test("account and product settings save independently without replacing membership or versions", async () => {
  await open("profile");
  await browser("focus", ".translation-switch");
  await press("Enter");
  await browser("wait", "--fn", "qa.profileWrites.length===1");
  await browser("focus", 'button[aria-label="编辑个人资料"]');
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  assert.equal(
    await evaluate(
      "document.querySelector('.profile-dialog').textContent.includes('每周学习')",
    ),
    false,
  );
  await browser("fill", ".profile-dialog input", "Shared name");
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.accountWrites.length===1");
  assert.deepEqual(await evaluate("qa.accountWrites[0]"), {
    displayName: "Shared name",
    expectedAccountVersion: 1,
  });
  assert.deepEqual(await evaluate("qa.profileWrites[0]"), {
    showTranslation: true,
    version: 1,
  });
  await evaluate(
    "qa.accountRelease[0]({id:'account-a',email:'a@example.test',displayName:'Shared name',role:'operator',version:9})",
  );
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.profile-dialog[open]')",
  );
  await evaluate(
    "qa.profileRelease[0]({id:'account-a',email:'a@example.test',displayName:'Old product snapshot',role:'learner',version:2,settings:{timeZone:'Asia/Shanghai',weeklyDays:5,dailyMinutes:10,showTranslation:true,speechRate:1}})",
  );
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.profile-note').textContent==='设置已保存到账号。'",
  );
  assert.equal(
    await evaluate("document.querySelector('.profile-summary h2').textContent"),
    "Shared name",
  );
  assert.equal(
    await evaluate("!!document.querySelector('a[href=\"/admin\"]')"),
    false,
  );
  await browser("focus", ".translation-switch");
  await press("Enter");
  await browser("wait", "--fn", "qa.profileWrites.length===2");
  assert.deepEqual(await evaluate("qa.profileWrites[1]"), {
    showTranslation: false,
    version: 2,
  });
  await evaluate("qa.profileRelease[1](401)");
});

test("an account switch aborts a delayed account CSRF bootstrap before any write", async () => {
  await open("profile");
  await browser("focus", 'button[aria-label="编辑个人资料"]');
  await press("Enter");
  await browser("wait", ".profile-dialog[open] input");
  await browser("fill", ".profile-dialog input", "Must not reach Bob");
  await evaluate("qa.deferAuthBootstrap=true");
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.authBootstraps.length===1");
  await evaluate("qa.changeUser()");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.profile-summary h2').textContent==='Bob'",
  );
  assert.equal(await evaluate("qa.authBootstraps[0].signal.aborted"), true);
  await evaluate("qa.authBootstraps[0].release(200)");
  assert.equal(await evaluate("qa.accountWrites.length"), 0);
  assert.equal(await evaluate("qa.profileWrites.length"), 0);
  assert.equal(
    await evaluate("document.querySelectorAll('.profile-dialog[open]').length"),
    0,
  );
});

test("learning day editing sends only product preferences", async () => {
  await open("profile");
  await browser("focus", ".settings-group button.setting-row");
  await press("Enter");
  await browser("wait", ".profile-dialog[open]");
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.profile-dialog .profile-field input').length",
    ),
    0,
  );
  await browser(
    "find",
    "role",
    "button",
    "click",
    "--name",
    "每周学习天数：5 天",
  );
  await browser("find", "role", "radio", "click", "--name", "7 天");
  await browser("focus", ".profile-dialog button[type=submit]");
  await press("Enter");
  await browser("wait", "--fn", "qa.profileWrites.length===1");
  assert.deepEqual(await evaluate("qa.profileWrites[0]"), {
    weeklyDays: 7,
    version: 1,
  });
  assert.equal(await evaluate("qa.accountWrites.length"), 0);
  await evaluate(
    "qa.profileRelease[0]({id:'account-a',email:'a@example.test',displayName:'Alice',role:'learner',version:2,settings:{timeZone:'Asia/Shanghai',weeklyDays:7,dailyMinutes:10,showTranslation:false,speechRate:1}})",
  );
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.profile-dialog[open]')",
  );
});

test("learning step and completion preserve waiting focus and retry their exact requests", async () => {
  await open("session");
  const state = {
    id: "qa-session",
    lessonId: "reading-protocol",
    revision: 1,
    version: 2,
    lastStepId: "read",
    confirmedStepIds: ["read"],
    hintedExerciseIds: [],
    attempts: [],
    completedAt: null,
    firstCompletedAt: null,
  };
  for (const [offset, result] of [
    [0, state],
    [
      2,
      {
        ...state,
        version: 3,
        completedAt: "2026-10-06T00:00:00Z",
        firstCompletedAt: "2026-10-06T00:00:00Z",
      },
    ],
  ]) {
    await browser("focus", ".learning-actions .primary");
    await press("Enter");
    await browser("wait", "--fn", `qa.learningWrites.length===${offset + 1}`);
    assert.equal(
      await evaluate(
        "document.activeElement.matches('.learning-actions .primary')",
      ),
      true,
    );
    assert.equal(
      await evaluate("document.activeElement.getAttribute('aria-busy')"),
      "true",
    );
    await press("Enter");
    assert.equal(await evaluate("qa.learningWrites.length"), offset + 1);
    const body = await evaluate(`qa.learningWrites[${offset}]`);
    const target = {
      path:
        "/api/v2/learning-sessions/qa-session/" +
        (offset === 0 ? "steps/read" : "complete"),
      method: offset === 0 ? "PUT" : "POST",
    };
    assert.deepEqual(await evaluate(`qa.learningPaths[${offset}]`), target);
    await evaluate(`qa.learningRelease[${offset}](503)`);
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.learning-actions .primary')?.textContent.includes('重试保存')",
    );
    assert.equal(
      await evaluate(
        "document.activeElement.matches('.learning-actions .primary')",
      ),
      true,
    );
    await press("Enter");
    await browser("wait", "--fn", `qa.learningWrites.length===${offset + 2}`);
    assert.deepEqual(await evaluate(`qa.learningWrites[${offset + 1}]`), body);
    assert.deepEqual(await evaluate(`qa.learningPaths[${offset + 1}]`), target);
    await evaluate(
      `qa.learningRelease[${offset + 1}](${JSON.stringify(result)})`,
    );
    await browser(
      "wait",
      "--fn",
      offset === 0
        ? "document.querySelector('.learning-actions .primary')?.textContent.includes('完成本课')"
        : "document.activeElement.textContent==='本课已完成'",
    );
  }
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending')",
    ),
    null,
  );
  assert.equal(await evaluate("qa.learningWrites.length"), 4);
});

test("completion survives navigation with its exact request and respects remote completion", async () => {
  const confirmed = {
    id: "qa-session",
    lessonId: "reading-protocol",
    revision: 1,
    version: 2,
    lastStepId: "read",
    confirmedStepIds: ["read"],
    hintedExerciseIds: [],
    attempts: [],
    completedAt: null,
    firstCompletedAt: null,
  };
  const completed = {
    ...confirmed,
    version: 3,
    completedAt: "2026-10-06T00:00:00Z",
    firstCompletedAt: "2026-10-06T00:00:00Z",
  };
  for (const outcome of ["lost-receipt", "remote-completion"]) {
    await open("session");
    await browser("focus", ".learning-actions .primary");
    await press("Enter");
    await browser("wait", "--fn", "qa.learningWrites.length===1");
    await evaluate(`qa.learningRelease[0](${JSON.stringify(confirmed)})`);
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.learning-actions .primary')?.textContent.includes('完成本课')",
    );
    await browser("focus", ".learning-actions .primary");
    await press("Enter");
    await browser("wait", "--fn", "qa.learningWrites.length===2");
    const original = await evaluate("qa.learningWrites[1]");
    assert.deepEqual(await evaluate("qa.learningPaths[1]"), {
      path: "/api/v2/learning-sessions/qa-session/complete",
      method: "POST",
    });
    if (outcome === "remote-completion") {
      await evaluate("qa.learningRelease[1](409)");
      await browser("wait", "--fn", "qa.learningReads.length===1");
      await evaluate(`qa.learningReads[0](${JSON.stringify(completed)})`);
    } else {
      await evaluate("qa.learningRelease[1](503)");
      await browser(
        "wait",
        "--fn",
        "document.querySelector('.learning-actions .primary')?.textContent.includes('重试保存')",
      );
      await evaluate("qa.navigate('/login')");
      await browser("wait", ".pending-navigation[open]");
      await browser("focus", ".pending-navigation .text-button");
      await press("Enter");
      await browser("wait", "--fn", "qa.route==='/login'");
      await evaluate("qa.navigate('/')");
      await browser(
        "wait",
        "--fn",
        "document.querySelector('.learning-actions .primary')?.textContent.includes('重试保存')",
      );
      await browser("focus", ".learning-actions .primary");
      await press("Enter");
      await browser("wait", "--fn", "qa.learningWrites.length===3");
      assert.deepEqual(await evaluate("qa.learningWrites[2]"), original);
      assert.deepEqual(
        await evaluate("qa.learningPaths[2]"),
        await evaluate("qa.learningPaths[1]"),
      );
      await evaluate(`qa.learningRelease[2](${JSON.stringify(completed)})`);
    }
    await browser(
      "wait",
      "--fn",
      "document.activeElement.textContent==='本课已完成'",
    );
    assert.equal(
      await evaluate("document.querySelectorAll('.learning-actions').length"),
      0,
    );
    assert.equal(
      await evaluate(
        "sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending')",
      ),
      null,
    );
    assert.equal(
      await evaluate("qa.learningWrites.length"),
      outcome === "lost-receipt" ? 3 : 2,
    );
    assert.equal(
      await evaluate("qa.learningReads.length"),
      outcome === "lost-receipt" ? 0 : 1,
    );
  }
});

test("restored multi-step confirmation advances once without another write", async () => {
  await open("session-multi");
  await browser("focus", ".learning-actions .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===1");
  const original = await evaluate("qa.learningWrites[0]");
  await evaluate("qa.learningRelease[0](503)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.learning-actions .primary')?.textContent.includes('重试保存')",
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  await browser("focus", ".pending-navigation .text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.route==='/login'");
  await evaluate("qa.navigate('/')");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.learning-actions .primary')?.textContent.includes('重试保存')",
  );
  await browser("focus", ".learning-actions .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===2");
  assert.deepEqual(await evaluate("qa.learningWrites[1]"), original);
  await evaluate(
    "qa.learningRelease[1]({id:'qa-session',lessonId:'reading-protocol',revision:1,version:2,lastStepId:'read',confirmedStepIds:['read'],hintedExerciseIds:[],attempts:[],completedAt:null,firstCompletedAt:null})",
  );
  await browser(
    "wait",
    "--fn",
    "!sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending')",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.learning-step-heading h2').textContent",
    ),
    "回顾",
  );
  await browser("wait", "--fn", "document.activeElement.textContent==='回顾'");
  assert.equal(await evaluate("qa.learningWrites.length"), 2);
  await browser("focus", ".learning-actions .text-button");
  await press("Enter");
  await browser("wait", "--fn", "document.activeElement.textContent==='阅读'");
  assert.equal(await evaluate("qa.learningWrites.length"), 2);
  await browser("focus", ".learning-actions .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===3");
  assert.equal(await evaluate("qa.learningWrites[2].version"), 2);
  assert.notEqual(
    await evaluate("qa.learningWrites[2].idempotencyKey"),
    original.idempotencyKey,
  );
  await evaluate(
    "qa.learningRelease[2]({id:'qa-session',lessonId:'reading-protocol',revision:1,version:3,lastStepId:'read',confirmedStepIds:['read'],hintedExerciseIds:[],attempts:[],completedAt:null,firstCompletedAt:null})",
  );
  await browser("wait", "--fn", "document.activeElement.textContent==='回顾'");
  assert.equal(await evaluate("qa.learningWrites.length"), 3);
});

test("learning conflict rereads progress without advancing and removes withdrawn content", async () => {
  await open("session-multi");
  await browser("focus", ".learning-actions .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===1");
  const rejected = await evaluate("qa.learningWrites[0]");
  await evaluate("qa.learningRelease[0](409)");
  await browser("wait", "--fn", "qa.learningReads.length===1");
  await evaluate("qa.learningReads[0](503)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.learning-actions .primary')?.textContent.includes('重新读取进度')",
  );
  assert.equal(
    await evaluate(
      "document.activeElement.matches('.learning-actions .primary')",
    ),
    true,
  );
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending')",
    ),
    null,
  );
  await press("Enter");
  await browser("wait", "--fn", "qa.learningReads.length===2");
  await press("Enter");
  assert.equal(await evaluate("qa.learningReads.length"), 2);
  assert.equal(await evaluate("qa.learningWrites.length"), 1);
  await evaluate(
    "qa.learningReads[1]({id:'qa-session',lessonId:'reading-protocol',revision:1,version:7,lastStepId:'read',confirmedStepIds:['read'],hintedExerciseIds:[],attempts:[],completedAt:null,firstCompletedAt:null})",
  );
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.learning-actions .primary')?.textContent.includes('继续')",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.learning-step-heading h2').textContent",
    ),
    "阅读",
  );
  assert.equal(await evaluate("qa.learningWrites.length"), 1);
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===2");
  assert.equal(await evaluate("qa.learningWrites[1].version"), 7);
  assert.notEqual(
    await evaluate("qa.learningWrites[1].idempotencyKey"),
    rejected.idempotencyKey,
  );
  await evaluate(
    "qa.learningRelease[1]({id:'qa-session',lessonId:'reading-protocol',revision:1,version:8,lastStepId:'read',confirmedStepIds:['read'],hintedExerciseIds:[],attempts:[],completedAt:null,firstCompletedAt:null})",
  );
  await browser("wait", "--fn", "document.activeElement.textContent==='回顾'");
  assert.equal(await evaluate("qa.learningWrites.length"), 2);
  await browser("focus", ".learning-actions .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===3");
  assert.equal(
    await evaluate("qa.learningPaths[2].path"),
    "/api/v2/learning-sessions/qa-session/steps/recap",
  );
  await evaluate("qa.learningRelease[2](409)");
  await browser("wait", "--fn", "qa.learningReads.length===3");
  await evaluate("qa.learningReads[2](410)");
  await browser(
    "wait",
    "--fn",
    "document.activeElement.textContent==='课程已撤回'",
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.sentence,.learning-actions').length",
    ),
    0,
  );
  assert.equal(
    await evaluate(
      "Object.keys(sessionStorage).some(key=>key.startsWith('brioche.learning.v1:qa-account:qa-session:1:'))",
    ),
    false,
  );
  assert.equal(await evaluate("qa.learningWrites.length"), 3);
  assert.equal(await evaluate("qa.learningReads.length"), 3);
});

test("learning pending navigation settles to the current step or withdrawal heading", async () => {
  for (const outcome of ["confirmed", "conflict", "withdrawn"]) {
    if (browserOpened) await evaluate("sessionStorage.clear()");
    await open("session-multi");
    await browser("focus", ".learning-actions .primary");
    await press("Enter");
    await browser("wait", "--fn", "qa.learningWrites.length===1");
    await evaluate("qa.navigate('/login')");
    await browser("wait", ".pending-navigation[open]");
    if (outcome === "confirmed") {
      await evaluate(
        "qa.learningRelease[0]({id:'qa-session',lessonId:'reading-protocol',revision:1,version:2,lastStepId:'read',confirmedStepIds:['read'],hintedExerciseIds:[],attempts:[],completedAt:null,firstCompletedAt:null})",
      );
    } else if (outcome === "conflict") {
      await evaluate("qa.learningRelease[0](409)");
      await browser("wait", "--fn", "qa.learningReads.length===1");
      await evaluate("qa.learningReads[0](503)");
    } else {
      await evaluate("qa.learningRelease[0](410)");
    }
    await browser(
      "wait",
      "--fn",
      "!document.querySelector('.pending-navigation[open]')",
    );
    assert.equal(
      await evaluate("document.activeElement.textContent"),
      outcome === "confirmed"
        ? "回顾"
        : outcome === "conflict"
          ? "阅读"
          : "课程已撤回",
      outcome,
    );
    assert.equal(await evaluate("qa.route"), "/");
    assert.equal(await evaluate("qa.learningWrites.length"), 1);
    if (outcome === "conflict")
      assert.ok(
        (
          await evaluate(
            "document.querySelector('.learning-actions .primary').textContent",
          )
        ).includes("重新读取进度"),
      );
    else assert.equal(await evaluate("qa.learningReads.length"), 0);
    assert.equal(
      await evaluate(
        "sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending')",
      ),
      null,
    );
  }
});

test("learning navigation keeps the exact pending request across leaving and returning", async () => {
  await open("session");
  await browser("set", "viewport", "320", "700");
  await browser("focus", ".speaker");
  await press("Enter");
  assert.equal(
    await evaluate("document.activeElement.matches('.speaker')"),
    true,
  );
  await browser("focus", ".learning-actions .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===1");
  const original = await evaluate("qa.learningWrites[0]");
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    true,
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  assert.equal(
    await evaluate(
      "(()=>{const r=document.querySelector('.pending-navigation[open]').getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight})()",
    ),
    true,
  );
  assert.equal(await evaluate("qa.route"), "/");
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "这次提交尚未确认",
  );
  await press("Escape");
  assert.equal(
    await evaluate(
      "document.activeElement.matches('.learning-step-heading h2')",
    ),
    true,
  );
  await evaluate("qa.learningRelease[0](503)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.learning-actions .primary')?.textContent.includes('重试保存')",
  );
  await evaluate("qa.navigate(-1)");
  await browser("wait", ".pending-navigation[open]");
  await browser("focus", ".pending-navigation .text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.route==='/previous'");
  assert.equal(await evaluate("qa.learningWrites.length"), 1);
  assert.deepEqual(
    await evaluate(
      "JSON.parse(sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending')).body",
    ),
    original,
  );
  await evaluate("qa.navigate('/')");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.learning-actions .primary')?.textContent.includes('重试保存')",
  );
  await browser("focus", ".learning-actions .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===2");
  assert.deepEqual(await evaluate("qa.learningWrites[1]"), original);
  await evaluate(
    "qa.learningRelease[1]({id:'qa-session',lessonId:'reading-protocol',revision:1,version:2,lastStepId:'read',confirmedStepIds:['read'],hintedExerciseIds:[],attempts:[],completedAt:null,firstCompletedAt:null})",
  );
  await browser(
    "wait",
    "--fn",
    "!sessionStorage.getItem('brioche.learning.v1:qa-account:qa-session:1:pending')",
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", "--fn", "qa.route==='/login'");

  await open("session");
  await browser("focus", ".learning-actions .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===1");
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  await evaluate(
    "qa.learningRelease[0]({id:'qa-session',lessonId:'reading-protocol',revision:1,version:2,lastStepId:'read',confirmedStepIds:['read'],hintedExerciseIds:[],attempts:[],completedAt:null,firstCompletedAt:null})",
  );
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.pending-navigation[open]')",
  );
  assert.equal(await evaluate("qa.route"), "/");
  assert.equal(
    await evaluate(
      "document.activeElement.matches('.learning-step-heading h2')",
    ),
    true,
  );
  assert.equal(await evaluate("qa.learningWrites.length"), 1);
  await evaluate("qa.navigate('/login')");
  await browser("wait", "--fn", "qa.route==='/login'");
});

test("review navigation preserves the original rating and does not treat queue reads as pending writes", async () => {
  await open("reviews");
  await browser("focus", ".review-flashcard");
  await press("Enter");
  await browser("focus", '.review-ratings button[data-grade="2"]');
  await press("Enter");
  await browser("wait", "--fn", "qa.reviewWrites.length===1");
  const original = await evaluate("qa.reviewWrites[0]");
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    true,
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  await press("Escape");
  assert.equal(
    await evaluate(
      "document.activeElement.matches('.review-session-header h1')",
    ),
    true,
  );
  await evaluate("qa.reviewRelease[0](503)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.empty-state h2')?.textContent==='确认上次复习'",
  );
  await evaluate("qa.navigate(-1)");
  await browser("wait", ".pending-navigation[open]");
  await browser("focus", ".pending-navigation .text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.route==='/previous'");
  assert.deepEqual(
    await evaluate(
      "JSON.parse(sessionStorage.getItem('brioche.learning.v1:qa-account:reviews:1:pending')).body",
    ),
    original,
  );
  await evaluate("qa.navigate('/')");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.empty-state h2')?.textContent==='确认上次复习'",
  );
  await browser("focus", ".review-page > button.primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.reviewWrites.length===2");
  assert.deepEqual(await evaluate("qa.reviewWrites[1]"), original);
  await evaluate(
    "qa.reviewRelease[1]({card:{id:'qa-card',knowledgeId:'qa-word',sourceLessonId:'reading-protocol',sourceRevision:1,vocabulary:{id:'qa-word',lemma:'bonjour',partOfSpeech:'phrase',gender:null,meaningZh:'你好',noteZh:'日常问候'},stage:1,dueAt:'2026-10-07T00:00:00Z',version:2,suspended:false},reviewedAt:'2026-10-06T00:00:00Z',timeZone:'Asia/Shanghai'})",
  );
  await browser("wait", "--fn", "qa.queueReads.length===1");
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    false,
  );
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:qa-account:reviews:1:pending')",
    ),
    null,
  );
  await evaluate("qa.navigate('/login')");
  await browser(
    "wait",
    "--fn",
    "qa.route==='/login'||!!document.querySelector('.pending-navigation[open]')",
  );
  assert.equal(await evaluate("qa.route"), "/login");
  assert.equal(await evaluate("qa.reviewWrites.length"), 2);
  await evaluate(
    "qa.queueReads[0]({items:[],dueCount:0,nextDueAt:null,localDate:'2026-10-06',timeZone:'Asia/Shanghai'})",
  );
  assert.equal(await evaluate("qa.route"), "/login");
});

test("restored legacy review rating replays its exact v1 endpoint in the v2 client", async () => {
  await open("reviews");
  await evaluate("qa.navigate('/login')");
  await browser("wait", "--fn", "qa.route==='/login'");
  const job = {
    path: "/api/v1/me/reviews/qa-card/attempts",
    method: "POST",
    body: {
      cardVersion: 1,
      rating: "remembered",
      idempotencyKey: "legacy-rating-fixed-1234",
    },
  };
  await evaluate(
    `sessionStorage.setItem('brioche.learning.v1:qa-account:reviews:1:pending',${JSON.stringify(JSON.stringify(job))});qa.navigate('/')`,
  );
  await browser("wait", "--text", "确认上次复习");
  await browser("focus", ".review-page > button.primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.reviewWrites.length===1");
  assert.equal(await evaluate("qa.reviewPaths[0]"), job.path);
  assert.deepEqual(await evaluate("qa.reviewWrites[0]"), job.body);
  await evaluate(
    "qa.reviewRelease[0]({card:{...qa.reviewFixture.items[0],version:2},reviewedAt:'2026-10-08T00:00:00Z',timeZone:'Asia/Shanghai'})",
  );
  await browser("wait", "--fn", "qa.queueReads.length===1");
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:qa-account:reviews:1:pending')",
    ),
    null,
  );
  await evaluate("qa.queueReads[0]({...qa.reviewFixture,items:[],dueCount:0})");
  await browser("wait", "--text", "本轮回顾");
});

test("revoked reviews close the leave prompt, focus recovery, and never revive an unavailable card", async () => {
  await open("reviews");
  await browser("focus", ".review-flashcard");
  await press("Enter");
  await browser("focus", '.review-ratings button[data-grade="0"]');
  await press("Enter");
  await browser("wait", "--fn", "qa.reviewWrites.length===1");
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  await evaluate("qa.reviewRelease[0](410)");
  await browser(
    "wait",
    "--fn",
    "qa.queueReads.length===1 && !document.querySelector('.pending-navigation[open]')",
  );
  assert.equal(await evaluate("qa.route"), "/");
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.review-flashcard,.review-context,.review-ratings').length",
    ),
    0,
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "需要确认复习队列",
  );
  assert.equal(await evaluate("qa.media.every(m=>m.paused)"), true);
  assert.deepEqual(await evaluate("qa.spoken"), []);
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:qa-account:reviews:1:pending')",
    ),
    null,
  );
  await evaluate("qa.queueReads[0](503)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.review-page > button.text-button')?.getAttribute('aria-busy')==='false'",
  );
  await browser("focus", ".review-page > button.text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.queueReads.length===2");
  await evaluate("qa.queueReads[1](qa.reviewFixture)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.review-page > button.text-button')?.getAttribute('aria-busy')==='false'",
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.review-flashcard,.review-context,.review-ratings').length",
    ),
    0,
  );
  await browser("focus", ".review-page > button.text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.queueReads.length===3");
  await evaluate(
    "qa.queueReads[2]({items:[],dueCount:0,nextDueAt:null,localDate:'2026-10-06',timeZone:'Asia/Shanghai'})",
  );
  await browser("wait", "--fn", "!!document.querySelector('.review-summary')");
  assert.equal(
    await evaluate(
      "document.activeElement.matches('.review-session-header h1')",
    ),
    true,
  );
  assert.equal(await evaluate("qa.reviewWrites.length"), 1);
});

test("collapsed library cards retain all pending writes and retry the exact original after navigation", async () => {
  await open("library");
  await browser("focus", ".library-entry-heading");
  await press("Enter");
  await browser("focus", ".bookmark-action");
  await press("Enter");
  await browser(
    "focus",
    ".knowledge-actions:not(:has(.bookmark-action)) button",
  );
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===2");
  const original = await evaluate("qa.ownedWrites[0]");
  await browser("focus", ".library-entry-heading");
  await press("Enter");
  assert.equal(
    await evaluate("!!document.querySelector('.library-entry-body')"),
    false,
  );
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    true,
  );
  // One acknowledged request cannot clear another pending operation.
  await evaluate("qa.ownedRelease[1](qa.reviewFixture.items[0])");
  await browser(
    "wait",
    "--fn",
    "Object.keys(sessionStorage).filter(k=>k.includes(':owned:')).length===1",
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  await press("Escape");
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "我的表达",
  );
  await evaluate("qa.ownedRelease[0](503)");
  await evaluate("qa.navigate(-1)");
  await browser("wait", ".pending-navigation[open]");
  await browser("focus", ".pending-navigation .text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.route==='/previous'");
  assert.deepEqual(
    await evaluate(
      "JSON.parse(sessionStorage.getItem(Object.keys(sessionStorage).find(k=>k.includes(':owned:'))))",
    ),
    { ...original, method: "PUT" },
  );
  await evaluate("qa.navigate('/')");
  await browser("wait", ".library-entry-heading");
  await browser("focus", ".library-entry-heading");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "!!document.querySelector('.bookmark-action:disabled') && !!Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='重试保存')",
  );
  await browser(
    "focus",
    ".knowledge-actions:has(.bookmark-action) button:last-child",
  );
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===3");
  assert.deepEqual(await evaluate("qa.ownedWrites[2]"), original);
  await browser("focus", ".library-entry-heading");
  await press("Enter");
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  // A late acknowledgement clears storage even when its writer is unmounted.
  await evaluate(
    "qa.ownedRelease[2]({...qa.savedFixture,saved:false,version:2})",
  );
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.pending-navigation[open]') && !Object.keys(sessionStorage).some(k=>k.includes(':owned:'))",
  );
  assert.equal(await evaluate("qa.route"), "/");
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "我的表达",
  );
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    false,
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", "--fn", "qa.route==='/login'");
  assert.equal(await evaluate("qa.ownedWrites.length"), 3);
});

test("pending save recovery keeps keyboard focus, prevents duplicate requests, and observes external confirmation", async () => {
  await open("pending");
  await browser(
    "wait",
    "--fn",
    "document.querySelectorAll('.library-entry').length===2",
  );
  await browser("focus", ".library-entry:first-child button");
  await press("Enter");
  await press("Enter");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===1");
  assert.deepEqual(
    await evaluate(
      "({tag:document.activeElement.tagName,busy:document.activeElement.getAttribute('aria-busy'),count:qa.ownedWrites.length})",
    ),
    { tag: "BUTTON", busy: "true", count: 1 },
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  await press("Escape");
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "未确认保存",
  );
  await evaluate("qa.ownedRelease[0](503)");
  await browser("wait", ".error-message");
  await browser("focus", ".library-entry:first-child button");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===2");
  assert.deepEqual(
    await evaluate("qa.ownedWrites[1]"),
    await evaluate("qa.ownedWrites[0]"),
  );
  await evaluate("qa.ownedRelease[1](qa.savedFixture)");
  await browser(
    "wait",
    "--fn",
    "document.querySelectorAll('.library-entry').length===1",
  );
  assert.equal(
    await evaluate("document.activeElement.matches('.library-entry button')"),
    true,
  );
  // A previously mounted writer can confirm while this recovery list is open.
  await evaluate(
    "qa.confirmExternal(qa.ownedWrites[0].path.endsWith('0') ? 1 : 0)",
  );
  assert.deepEqual(
    await evaluate(
      "({rows:document.querySelectorAll('.library-entry').length, keys:Object.keys(sessionStorage).filter(k=>k.includes(':qa-account:')),focus:document.activeElement.textContent})",
    ),
    { rows: 0, keys: [], focus: "没有待确认的保存。" },
  );
  await browser(
    "wait",
    "--fn",
    "document.querySelectorAll('.library-entry').length===0",
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "没有待确认的保存。",
  );
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    false,
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", "--fn", "qa.route==='/login'");
  assert.equal(await evaluate("qa.ownedWrites.length"), 2);
});

test("an old account recovery response cannot replace the new account pending list or lock its writes", async () => {
  await open("pending");
  await browser(
    "wait",
    "--fn",
    "document.querySelectorAll('.library-entry').length===2",
  );
  await browser("focus", ".library-entry:first-child button");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===1");
  await evaluate("qa.changeUser()");
  await browser(
    "wait",
    "--fn",
    "document.querySelectorAll('.library-entry').length===1",
  );
  await browser("focus", ".library-entry button");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===2");
  assert.equal(
    await evaluate("qa.ownedWrites[1].path"),
    "/api/v1/me/saved-items/pending-word-2",
  );
  await evaluate("qa.ownedRelease[0](qa.savedFixture)");
  assert.deepEqual(
    await evaluate(
      "({rows:document.querySelectorAll('.library-entry').length,busy:document.querySelector('.library-entry button')?.getAttribute('aria-busy')})",
    ),
    { rows: 1, busy: "true" },
  );
  await evaluate("qa.ownedRelease[1](qa.savedFixture)");
  await browser(
    "wait",
    "--fn",
    "document.querySelectorAll('.library-entry').length===0",
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "没有待确认的保存。",
  );
});

test("unavailable learning retains independent expression writes and only releases navigation after all confirmations", async () => {
  await open("session-revoked");
  await browser("focus", ".learning-actions button.primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.learningWrites.length===1");
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  await evaluate("qa.learningRelease[0](410)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.lesson-header h1')?.textContent==='课程已撤回'",
  );
  assert.equal(
    await evaluate("!!document.querySelector('.pending-navigation[open]')"),
    true,
  );
  assert.equal(await evaluate("qa.route"), "/");
  await press("Escape");
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "课程已撤回",
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.learning-stage,.learning-actions').length",
    ),
    0,
  );
  assert.equal(
    await evaluate(
      "Object.keys(sessionStorage).filter(k=>k.includes(':qa-account:owned:')).length",
    ),
    2,
  );
  await evaluate("qa.navigate('/login')");
  await browser(
    "wait",
    "--fn",
    "qa.route==='/login'||!!document.querySelector('.pending-navigation[open]')",
  );
  assert.equal(await evaluate("qa.route"), "/");
  await press("Escape");
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "课程已撤回",
  );
  await evaluate("qa.confirmExternal(0)");
  await evaluate("qa.navigate('/login')");
  await browser("wait", ".pending-navigation[open]");
  await evaluate("qa.confirmExternal(1)");
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.pending-navigation[open]')",
  );
  assert.equal(await evaluate("qa.route"), "/");
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "课程已撤回",
  );
  assert.equal(
    await evaluate(
      "(()=>{const e=new Event('beforeunload',{cancelable:true});window.dispatchEvent(e);return e.defaultPrevented})()",
    ),
    false,
  );
  await evaluate("qa.navigate('/login')");
  await browser("wait", "--fn", "qa.route==='/login'");
  assert.equal(await evaluate("qa.learningWrites.length"), 1);
  assert.equal(await evaluate("qa.ownedWrites.length"), 0);
});

test("optional fill-blank hints omit empty actions and reveal meaningful hints once", async () => {
  await open("text-no-hint");
  assert.equal(
    await evaluate("document.querySelectorAll('.practice-hint').length"),
    0,
  );
  await browser("focus", ".exercise-sheet input");
  await browser("keyboard", "inserttext", "bonjour");
  await browser("focus", ".exercise-sheet button.primary");
  await press("Enter");
  assert.deepEqual(await evaluate("qa.textAnswers[0]"), {
    kind: "text",
    text: "bonjour",
  });
  assert.equal(await evaluate("qa.hintRequests"), 0);
  await open("text-limit");
  await browser("focus", ".exercise-sheet .practice-hint");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('.exercise-sheet .profile-note')?.textContent==='边界测试'",
  );
  assert.equal(await evaluate("qa.hintRequests"), 1);
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "边界测试",
  );
  assert.equal(
    await evaluate("document.querySelectorAll('.practice-hint').length"),
    0,
  );
});

test("native fill-blank input bounds UTF-16 units and submits the exact text without removing accents", async () => {
  await open("text-limit");
  await browser("focus", ".exercise-sheet input");
  assert.equal(await evaluate("document.activeElement.maxLength"), 1024);
  await browser("keyboard", "inserttext", "a".repeat(1025));
  assert.equal(await evaluate("document.activeElement.value.length"), 1024);
  await browser("focus", ".exercise-sheet button.primary");
  await press("Enter");
  assert.deepEqual(await evaluate("qa.textAnswers[0]"), {
    kind: "text",
    text: "a".repeat(1024),
  });
  await browser("focus", ".exercise-sheet input");
  await press("Control+a");
  await browser("keyboard", "inserttext", "😀".repeat(513));
  assert.equal(await evaluate("document.activeElement.value.length"), 1024);
  await browser("focus", ".exercise-sheet button.primary");
  await press("Enter");
  assert.deepEqual(await evaluate("qa.textAnswers[1]"), {
    kind: "text",
    text: "😀".repeat(512),
  });
  await browser("focus", ".exercise-sheet input");
  await press("Control+a");
  await browser("keyboard", "inserttext", "e\u0301");
  await browser("focus", ".exercise-sheet button.primary");
  await press("Enter");
  assert.deepEqual(await evaluate("qa.textAnswers[2]"), {
    kind: "text",
    text: "e\u0301",
  });
});

test("responsive content keeps long words and controls inside the viewport", async () => {
  const selectors =
    ".lesson-header h1,.sentence .word,.practice-option span,.practice-sentence,.order-bank button,.order-answer button,.learning-step-heading h2,.review-expression,.profile-summary h2,.library-entry-heading,.library-entry-heading strong,.lesson-label small,.lesson-label b,.resume-learning strong,.review .fr,.course-search-field";
  for (const kind of [
    "reading",
    "session",
    "reviews",
    "profile",
    "library",
    "home",
    "courses",
    "text-limit",
  ]) {
    await open(kind + "&stress=1");
    await browser(
      "wait",
      kind === "profile"
        ? ".profile-summary h2"
        : kind === "reviews"
          ? ".review-expression"
          : kind === "text-limit"
            ? ".order-bank button"
            : kind === "library"
              ? ".library-entry-heading"
              : kind === "home" || kind === "courses"
                ? ".lesson-row"
                : ".sentence .word",
    );
    for (const width of [320, 390, 768, 1440]) {
      await browser("set", "viewport", String(width), "844");
      await browser(
        "wait",
        "--fn",
        "document.getAnimations().every(a=>a.playState!=='running'||!Number.isFinite(a.effect.getComputedTiming().endTime))",
      );
      const overflow = await evaluate(`(() => {
        const nodes = [...document.querySelectorAll(${JSON.stringify(selectors)})];
        return nodes.filter(e => e.getClientRects().length).flatMap(e => {
          const r = e.getBoundingClientRect();
          const range = document.createRange();
          range.selectNodeContents(e);
          const text = range.getBoundingClientRect();
          const parent = e.parentElement.getBoundingClientRect();
          return Math.min(r.left,text.left) < Math.max(0,parent.left) - 1 || Math.max(r.right,text.right) > Math.min(innerWidth,parent.right) + 1 ? [{text:e.textContent,left:r.left,right:r.right,textRight:text.right,parentLeft:parent.left,parentRight:parent.right}] : [];
        });
      })()`);
      assert.deepEqual(overflow, [], kind + " at " + width + "px");
      const documentBounds = await evaluate(
        `({width:document.documentElement.scrollWidth,viewport:innerWidth,overflow:[...document.querySelectorAll('main *')].filter(e=>e.getClientRects().length && e.getBoundingClientRect().right > innerWidth + 1).map(e=>({tag:e.tagName,cls:e.className,text:e.textContent.slice(0,100),right:e.getBoundingClientRect().right})).slice(0,12)})`,
      );
      assert.ok(
        documentBounds.width <= documentBounds.viewport,
        kind +
          " document at " +
          width +
          "px: " +
          JSON.stringify(documentBounds),
      );
      if (kind === "reading") {
        await browser("focus", "[aria-selected=true]");
        await press("ArrowRight");
        assert.equal(
          await evaluate("document.querySelectorAll('.speaker').length"),
          0,
        );
        assert.equal(
          await evaluate(
            "[...document.querySelectorAll('.sentence .word')].every(e=>{const r=e.getBoundingClientRect(),p=e.parentElement.getBoundingClientRect();return r.left>=p.left-1&&r.right<=p.right+1&&r.right<=innerWidth+1})",
          ),
          true,
          "article at " + width + "px",
        );
        await press("Home");
      }
    }
    if (kind === "reading") {
      await browser("focus", ".sentence .word");
      await press("Enter");
      await browser("wait", "--fn", "qa.playback==='playing'");
      assert.equal(
        await evaluate(
          "qa.mediaPlays.at(-1).url.endsWith('/api/audio/'+'1'.repeat(64)+'.wav')",
        ),
        true,
      );
    }
    if (kind === "library") {
      await browser("focus", ".library-entry-heading");
      await press("Enter");
      await browser("wait", ".library-entry-body");
      assert.deepEqual(await evaluate("qa.spoken"), []);
      assert.equal(
        await evaluate(
          "document.querySelector('.toast [role=status]').textContent",
        ),
        "这段录音还在准备中。",
      );
      assert.equal(
        await evaluate("document.activeElement.className"),
        "library-entry-heading",
      );
      await press("Enter");
      assert.equal(
        await evaluate(
          "document.querySelectorAll('.library-entry-body').length",
        ),
        0,
      );
    }
  }
  await browser("set", "viewport", "320", "844");
  await browser("focus", ".order-bank button");
  await press("Enter");
  assert.equal(
    await evaluate(
      "document.querySelector('.order-answer button').textContent",
    ),
    "anticonstitutionnellement",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.order-answer button').getBoundingClientRect().right<=innerWidth",
    ),
    true,
  );
});

test("reduced motion disables card expansion animations", async () => {
  await browser("set", "media", "light", "reduced-motion");
  try {
    await open("reviews");
    assert.equal(
      await evaluate("matchMedia('(prefers-reduced-motion: reduce)').matches"),
      true,
    );
    await browser("focus", ".review-flashcard");
    await press("Enter");
    await browser("wait", ".review-ratings");
    assert.equal(await evaluate("document.getAnimations().length"), 0);
    assert.equal(
      await evaluate(
        "getComputedStyle(document.querySelector('.review-flashcard')).transitionDuration",
      ),
      "0s",
    );
    await open("home");
    await browser("focus", ".home-review button.review");
    await press("Enter");
    assert.equal(
      await evaluate("document.activeElement.getAttribute('aria-expanded')"),
      "true",
    );
    assert.equal(await evaluate("document.getAnimations().length"), 0);
  } finally {
    await browser("set", "media", "light");
  }
});

test("managed review recovery preserves keyboard focus and deduplicates reads and retries", async () => {
  await open("managed-library");
  await browser("set", "viewport", "320", "844");
  await browser("focus", ".library-entry-heading");
  await press("Enter");
  await browser("focus", ".library-entry-body button");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===1");
  await evaluate("qa.ownedRelease[0](503)");
  await browser("wait", "--text", "重试保存");
  await browser("focus", ".library-entry-body button:nth-of-type(2)");
  await press("Enter");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===2");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "重试保存",
  );
  assert.equal(
    await evaluate(
      "JSON.stringify(qa.ownedWrites[0])===JSON.stringify(qa.ownedWrites[1])",
    ),
    true,
  );
  await evaluate(
    "qa.ownedRelease[1]({...qa.reviewFixture.items[0],version:2,suspended:true})",
  );
  await browser("wait", "--text", "恢复复习");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "恢复复习",
  );
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===3");
  await evaluate("qa.ownedRelease[2](409)");
  await browser("wait", "--fn", "qa.cardReads.length===1");
  await evaluate("qa.cardReads[0](503)");
  await browser("wait", "--text", "重新读取记录");
  await browser("focus", ".library-entry-body button:nth-of-type(2)");
  await press("Enter");
  await press("Enter");
  await press("Enter");
  await browser("wait", "--fn", "qa.cardReads.length>=2");
  assert.equal(await evaluate("qa.cardReads.length"), 2);
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "正在读取",
  );
  await evaluate("qa.cardReads[1](503)");
  await browser("wait", "--text", "重新读取记录");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "重新读取记录",
  );
  await press("Enter");
  await browser("wait", "--fn", "qa.cardReads.length===3");
  await evaluate(
    "qa.cardReads[2]({...qa.reviewFixture.items[0],version:3,suspended:false})",
  );
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.library-entry-body')?.textContent.includes('重新读取记录')",
  );
  assert.equal(
    await evaluate("document.activeElement.className"),
    "library-entry-heading",
  );
  assert.equal(await evaluate("qa.ownedWrites.length"), 3);
  await browser("focus", ".library-entry-body button");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===4");
  await evaluate("qa.ownedRelease[3](409)");
  await browser("wait", "--fn", "qa.cardReads.length===4");
  await evaluate("qa.cardReads[3](503)");
  await browser("wait", "--text", "重新读取记录");
  await browser("focus", ".library-entry-body button:nth-of-type(2)");
  await press("Enter");
  await browser("wait", "--fn", "qa.cardReads.length===5");
  await browser("focus", ".library-entry-body a");
  await evaluate(
    "qa.cardReads[4]({...qa.reviewFixture.items[0],version:4,suspended:false})",
  );
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.library-entry-body')?.textContent.includes('重新读取记录')",
  );
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "回看来源课程",
  );
  await open("managed-library");
  await browser("focus", ".library-entry-heading");
  await press("Enter");
  await browser("focus", ".library-entry-body button");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===1");
  await evaluate("qa.ownedRelease[0](503)");
  await browser("wait", "--text", "重试保存");
  await browser("focus", ".library-entry-body button:nth-of-type(2)");
  await press("Enter");
  await browser("wait", "--fn", "qa.ownedWrites.length===2");
  await evaluate("qa.ownedRelease[1](422)");
  await browser(
    "wait",
    "--fn",
    "document.activeElement.getAttribute('role')==='alert'",
  );
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "请检查填写的信息。",
  );
  assert.equal(await evaluate("qa.cardReads.length"), 0);
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.library-entry-body button').length",
    ),
    1,
  );
});

test("homepage omits an empty expression card while keeping review and continuation entries", async () => {
  await open("home-no-expression");
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.home-review button.review').length",
    ),
    0,
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.resume-learning').getAttribute('href')",
    ),
    "/learning/qa-home-session",
  );
  await browser("focus", ".home-review-link");
  await press("Enter");
  await browser("wait", "--fn", "qa.route==='/reviews'");
});

test("homepage rapid card toggles keep a single height animation and immediate expanded state", async () => {
  await open("home&hold-animation=1");
  await browser("focus", ".home-review button.review");
  await press("Enter");
  assert.equal(
    await evaluate("document.activeElement.getAttribute('aria-expanded')"),
    "true",
  );
  await press("Space");
  assert.equal(
    await evaluate("document.activeElement.getAttribute('aria-expanded')"),
    "false",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.home-review button.review').getAnimations().filter(a=>a.effect.target===document.querySelector('.home-review button.review')).length<=1",
    ),
    true,
  );
});

test("course search retains focus while pending and shows explicit empty results and reset", async () => {
  await open("courses");
  await browser("focus", "#course-query");
  await browser("keyboard", "inserttext", "introuvable");
  await browser("focus", ".course-search button");
  await press("Enter");
  await browser("wait", "--fn", "qa.catalogReads.length===1");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "搜索",
  );
  await press("Enter");
  assert.equal(await evaluate("qa.catalogReads.length"), 1);
  assert.equal(
    await evaluate(
      "document.querySelector('.courses-page [role=status]').textContent",
    ),
    "正在查找…",
  );
  await evaluate(
    "qa.catalogReads[0].release({levels:[],developmentFixture:false})",
  );
  await browser("wait", "--text", "还没有找到这个场景");
  assert.equal(
    await evaluate("document.querySelector('#course-query').value"),
    "introuvable",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.courses-page [role=status]').textContent",
    ),
    "找到 0 堂课程",
  );
  await browser("focus", ".empty-state a");
  await press("Enter");
  await browser("wait", ".lesson-row");
  assert.equal(
    await evaluate("document.querySelector('#course-query').value"),
    "",
  );
  await browser("focus", "#course-query");
  await browser("keyboard", "inserttext", "bonjour");
  await press("Enter");
  await browser("wait", "--fn", "qa.catalogReads.length===2");
  await evaluate("qa.catalogReads[1].release(qa.catalogFixture)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('[role=status]').textContent==='找到 1 堂课程'",
  );
  assert.equal(await evaluate("document.activeElement.id"), "course-query");
  await press("Enter");
  await browser("wait", "--fn", "qa.catalogReads.length===3");
  await browser("keyboard", "inserttext", " demain");
  await evaluate("qa.catalogReads[2].release(qa.catalogFixture)");
  await browser(
    "wait",
    "--fn",
    "document.querySelector('[role=status]').textContent==='找到 1 堂课程'",
  );
  assert.equal(await evaluate("document.activeElement.id"), "course-query");
  assert.equal(
    await evaluate("document.querySelector('#course-query').value"),
    "bonjour demain",
  );
});

test("review history pagination focuses results, distinguishes empty continuations, and returns to latest records", async () => {
  await open("history");
  await browser("wait", ".review-result-list li");
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.review-result-list li').length",
    ),
    2,
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.result-expression')[1].textContent",
    ),
    "来源内容已撤回",
  );
  assert.equal(
    await evaluate("document.querySelectorAll('.result-expression')[1].lang"),
    "zh-CN",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.review-result-list small').textContent.includes('2026/10/6 07:30')",
    ),
    true,
  );
  await browser("focus", "a[href*='cursor=']");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "new URLSearchParams(qa.search).get('cursor')==='older/qa?+'",
  );
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "复习记录",
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.review-result-list li').length",
    ),
    1,
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.review-result-list small').textContent.includes('还不熟')",
    ),
    true,
  );
  await browser("focus", "a[href*='cursor=end']");
  await press("Enter");
  await browser("wait", "--fn", "qa.search==='?cursor=end'");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "复习记录",
  );
  assert.equal(
    await evaluate("document.querySelector('.profile-note').textContent"),
    "这一页没有更早的复习记录。",
  );
  await browser("focus", "a[href='/review-history']");
  await press("Enter");
  await browser("wait", ".review-result-list li");
  assert.equal(await evaluate("qa.search"), "");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "复习记录",
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.review-result-list li').length",
    ),
    2,
  );
  await open("history-empty");
  await browser("wait", ".profile-note");
  assert.equal(
    await evaluate("document.querySelector('.profile-note').textContent"),
    "完成一次复习后，记录会显示在这里。",
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('a[href*=cursor],a[href=\"/review-history\"]').length",
    ),
    0,
  );
});

test("review history keeps long expressions and recorded dates inside four viewport widths", async () => {
  await open("history&stress=1");
  await browser("wait", ".review-result-list li");
  for (const width of [320, 390, 768, 1440]) {
    await browser("set", "viewport", String(width), "844");
    await browser(
      "wait",
      "--fn",
      "document.getAnimations().every(a=>a.playState!=='running'||!Number.isFinite(a.effect.getComputedTiming().endTime))",
    );
    const overflow = await evaluate(`(() => {
      return [...document.querySelectorAll('.result-expression,.review-result-list small,.review-result-grade')].flatMap(e => {
        const r=e.getBoundingClientRect(),p=e.parentElement.getBoundingClientRect();
        const range=document.createRange();range.selectNodeContents(e);const t=range.getBoundingClientRect();
        return Math.min(r.left,t.left)<Math.max(0,p.left)-1 || Math.max(r.right,t.right)>Math.min(innerWidth,p.right)+1 ? [{text:e.textContent,right:r.right,textRight:t.right,parentRight:p.right}] : [];
      });
    })()`);
    assert.deepEqual(overflow, [], "history at " + width + "px");
    assert.equal(
      await evaluate("document.documentElement.scrollWidth<=innerWidth"),
      true,
    );
    assert.equal(
      await evaluate(
        "document.querySelector('.result-expression').textContent",
      ),
      "anticonstitutionnellement",
    );
  }
});

async function fillAccount(
  password = "only-test-password",
  email = "qa@example.test",
) {
  if (
    await evaluate(
      "!!document.querySelector('.account-page input[type=email]')",
    )
  ) {
    await browser("focus", ".account-page input[type=email]");
    await press("Control+a");
    await browser("keyboard", "inserttext", email);
  }
  await browser("focus", ".account-page input[type=password]");
  await press("Control+a");
  await browser("keyboard", "inserttext", password);
  await browser("focus", ".account-page button.primary");
}

test("account login retains focus and input while pending, deduplicates submits, and focuses retry errors", async () => {
  await open("account-login");
  await fillAccount();
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===1");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "正在确认",
  );
  await browser("focus", ".account-page input[type=password]");
  assert.equal(await evaluate("document.activeElement.readOnly"), true);
  await browser("keyboard", "inserttext", "ignored");
  assert.equal(
    await evaluate("document.activeElement.value"),
    "only-test-password",
  );
  await browser("focus", ".account-page button.primary");
  await press("Enter");
  assert.equal(await evaluate("qa.authRequests.length"), 1);
  assert.deepEqual(await evaluate("qa.authRequests[0].body"), {
    email: "qa@example.test",
    password: "only-test-password",
  });
  await evaluate("qa.authRequests[0].release(401)");
  await browser(
    "wait",
    "--fn",
    "document.activeElement.getAttribute('role')==='alert'",
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "邮箱或密码不正确。",
  );
  assert.equal(
    await evaluate("document.querySelector('input[type=password]').value"),
    "only-test-password",
  );
  await browser("focus", ".account-page button.primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===2");
  await evaluate("qa.authRequests[1].release(429)");
  await browser(
    "wait",
    "--fn",
    "document.activeElement.getAttribute('role')==='alert'",
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "尝试次数较多，请稍后重试。",
  );
  await browser("focus", ".account-page button.primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===3");
  await browser("focus", ".account-exit");
  await evaluate("qa.authRequests[2].release(503)");
  await browser("wait", "--text", "账号服务暂时不可用，请稍后重试。");
  assert.equal(
    await evaluate("document.activeElement.className"),
    "account-exit",
  );
});

test("leaving an account form cancels pending CSRF and never starts the late auth mutation", async () => {
  await open("account-abort");
  await fillAccount();
  await press("Enter");
  await browser("wait", "--fn", "qa.authBootstraps.length===1");
  await browser("focus", ".account-exit");
  await press("Enter");
  await browser("wait", "--fn", "qa.route==='/previous'");
  await evaluate("qa.authBootstraps[0].release(200)");
  await browser(
    "wait",
    "--fn",
    "qa.authRequests.length>0||qa.authBootstraps[0].signal.aborted",
  );
  assert.equal(await evaluate("qa.authRequests.length"), 0);
  assert.equal(await evaluate("qa.authBootstraps[0].signal.aborted"), true);
});

test("password recovery removes its fragment, submits the token, and focuses the cleared success screen", async () => {
  await open(
    "account-reset#token=only-test-recovery-token&email=qa%40example.test",
  );
  await browser("wait", ".account-page input[type=password]");
  const previousNotice = await evaluate(
    "localStorage.getItem('brioche.identity-change.v1')",
  );
  assert.equal(await evaluate("location.hash"), "");
  await fillAccount();
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===1");
  assert.deepEqual(await evaluate("qa.authRequests[0].body"), {
    token: "only-test-recovery-token",
    password: "only-test-password",
  });
  await evaluate("qa.authRequests[0].release(200)");
  await browser("wait", "--text", "密码已更新");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "密码已更新",
  );
  assert.equal(
    await evaluate("document.querySelectorAll('.account-page input').length"),
    0,
  );
  assert.equal(
    await evaluate("!!localStorage.getItem('brioche.identity-change.v1')"),
    true,
  );
  assert.notEqual(
    await evaluate("localStorage.getItem('brioche.identity-change.v1')"),
    previousNotice,
  );
});

test("a new invitation link cancels the old request and clears credentials before its own submission", async () => {
  await open(
    "account-invite#token=only-test-old-invite&email=qa%40example.test",
  );
  await browser("wait", ".account-page input[type=password]");
  await fillAccount();
  await browser("focus", "input[autocomplete=nickname]");
  await browser("keyboard", "inserttext", "First QA");
  await browser("focus", ".account-page button.primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===1");
  await evaluate(
    "location.hash='token=only-test-next-invite&email=qa-next%40example.test'",
  );
  await browser(
    "wait",
    "--fn",
    "document.querySelector('input[type=email]').value==='qa-next@example.test'&&location.hash===''",
  );
  assert.equal(await evaluate("qa.authRequests[0].signal.aborted"), true);
  assert.equal(
    await evaluate("document.querySelector('input[type=password]').value"),
    "",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('input[autocomplete=nickname]').value",
    ),
    "",
  );
  await evaluate("qa.authRequests[0].release(200)");
  assert.equal(await evaluate("location.pathname"), "/");
  await fillAccount("only-test-next-password", "qa-next@example.test");
  await browser("focus", "input[autocomplete=nickname]");
  await browser("keyboard", "inserttext", "Next QA");
  await browser("focus", ".account-page button.primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===2");
  assert.deepEqual(await evaluate("qa.authRequests[1].body"), {
    email: "qa-next@example.test",
    password: "only-test-next-password",
    token: "only-test-next-invite",
    displayName: "Next QA",
  });
  await evaluate("qa.authRequests[1].release(400)");
  await browser(
    "wait",
    "--fn",
    "document.activeElement.getAttribute('role')==='alert'",
  );
  assert.equal(
    await evaluate("document.querySelector('input[type=password]').value"),
    "only-test-next-password",
  );
});

test("logout keeps focus and owner drafts on failure, then clears only the confirmed owner's drafts", async () => {
  await open("profile");
  await browser("wait", ".profile-summary h2");
  await evaluate(
    "sessionStorage.setItem('brioche.learning.v1:account-a:logout-qa:1:answer','owner draft');sessionStorage.setItem('brioche.learning.v1:account-b:logout-qa:1:answer','other draft');sessionStorage.setItem('logout-qa-unrelated','unrelated')",
  );
  await browser("focus", ".settings-page > button.text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===1");
  assert.equal(await evaluate("document.activeElement.tagName"), "BUTTON");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "正在退出",
  );
  await press("Enter");
  assert.equal(await evaluate("qa.authRequests.length"), 1);
  assert.equal(
    await evaluate("qa.authRequests[0].path"),
    "/api/v1/auth/logout",
  );
  assert.equal(await evaluate("qa.authRequests[0].body"), null);
  await evaluate("qa.authRequests[0].release(503)");
  await browser("wait", "--text", "退出未完成，请重试。");
  assert.equal(
    await evaluate("document.activeElement.textContent.trim()"),
    "退出登录",
  );
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:account-a:logout-qa:1:answer')",
    ),
    "owner draft",
  );
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===2");
  await evaluate("qa.authRequests[1].release(200)");
  await browser(
    "wait",
    "--fn",
    "window.qa?.ready&&document.querySelector('main h1')?.textContent==='course-a'",
  );
  assert.equal(await evaluate("location.search"), "");
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:account-a:logout-qa:1:answer')",
    ),
    null,
  );
  assert.equal(
    await evaluate(
      "sessionStorage.getItem('brioche.learning.v1:account-b:logout-qa:1:answer')",
    ),
    "other draft",
  );
  assert.equal(
    await evaluate("sessionStorage.getItem('logout-qa-unrelated')"),
    "unrelated",
  );
  await evaluate(
    "sessionStorage.removeItem('brioche.learning.v1:account-b:logout-qa:1:answer');sessionStorage.removeItem('logout-qa-unrelated')",
  );
});

test("an old logout cannot navigate or toast into a new profile and leaving cancels its CSRF before POST", async () => {
  await open("profile");
  await browser("focus", ".settings-page > button.text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===1");
  await evaluate("qa.changeUser()");
  await browser("wait", "--text", "Bob");
  await evaluate("qa.authRequests[0].release(200)");
  await browser(
    "wait",
    "--fn",
    "qa.authRequests[0]?.signal.aborted||location.search===''",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.profile-summary h2')?.textContent",
    ),
    "Bob",
  );
  assert.equal(await evaluate("qa.authRequests[0].signal.aborted"), true);
  assert.equal(await evaluate("document.querySelector('.toast').hidden"), true);
  await open("profile");
  await browser("focus", ".settings-page > button.text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.authRequests.length===1");
  await evaluate("qa.changeUser()");
  await browser("wait", "--text", "Bob");
  await evaluate("qa.authRequests[0].release(503)");
  await browser(
    "wait",
    "--fn",
    "qa.authRequests[0].signal.aborted||!document.querySelector('.toast').hidden",
  );
  assert.equal(await evaluate("document.querySelector('.toast').hidden"), true);
  assert.equal(
    await evaluate("document.querySelector('.profile-summary h2').textContent"),
    "Bob",
  );
  await open("profile");
  await evaluate("qa.deferAuthBootstrap=true");
  await browser("focus", ".settings-page > button.text-button");
  await press("Enter");
  await browser("wait", "--fn", "qa.authBootstraps.length===1");
  await browser("focus", ".setting-link[href='/reviews']");
  await press("Enter");
  await browser("wait", "--fn", "qa.route==='/reviews'");
  await evaluate("qa.authBootstraps[0].release(200)");
  await browser(
    "wait",
    "--fn",
    "qa.authRequests.length>0||qa.authBootstraps[0].signal.aborted",
  );
  assert.equal(await evaluate("qa.authRequests.length"), 0);
  assert.equal(await evaluate("qa.authBootstraps[0].signal.aborted"), true);
  assert.equal(await evaluate("qa.route"), "/reviews");
});

test("guest practice keeps answers and focus through failures, retry and three exercise kinds", async () => {
  await open("demo");
  await browser("wait", ".exercise-sheet input[type=radio]");
  const answers = [
    { kind: "choice", optionId: "bonjour" },
    { kind: "text", text: "une" },
    { kind: "order", tokenIds: ["bonjour", "luc"] },
  ];
  for (let index = 0; index < 3; index++) {
    if (index === 0) {
      await browser("focus", ".exercise-sheet input[type=radio]");
      await press("Space");
    } else if (index === 1) {
      await browser("focus", ".practice-input");
      await browser("keyboard", "inserttext", "une");
    } else {
      await browser("focus", ".order-bank button");
      await press("Enter");
      await press("Enter");
    }
    await browser("focus", ".exercise-sheet button[type=submit]");
    await press("Enter");
    await browser("wait", "--fn", `qa.demoWrites.length===${index * 3 + 1}`);
    assert.equal(
      await evaluate(
        "document.activeElement.matches('.exercise-sheet button[type=submit]')",
      ),
      true,
    );
    await press("Enter");
    assert.equal(await evaluate("qa.demoWrites.length"), index * 3 + 1);
    await evaluate(`qa.demoWrites[${index * 3}].release(503)`);
    await browser("wait", ".error-message");
    assert.equal(
      await evaluate(
        "document.activeElement.matches('.exercise-sheet button[type=submit]')",
      ),
      true,
    );
    await press("Enter");
    await browser("wait", "--fn", `qa.demoWrites.length===${index * 3 + 2}`);
    assert.deepEqual(await evaluate(`qa.demoWrites[${index * 3 + 1}].body`), {
      revision: 1,
      exerciseId: ["demo-choice", "demo-text", "demo-order"][index],
      answer: answers[index],
    });
    await evaluate(
      `qa.demoWrites[${index * 3 + 1}].release({correct:false,feedbackZh:'再试一下'})`,
    );
    await browser(
      "wait",
      "--fn",
      "document.activeElement.matches('.practice-feedback')",
    );
    await browser("focus", ".practice-next .text-button");
    await press("Enter");
    await browser(
      "wait",
      "--fn",
      "document.activeElement.matches('fieldset input,fieldset button')",
    );
    await browser("focus", ".exercise-sheet button[type=submit]");
    await press("Enter");
    await browser("wait", "--fn", `qa.demoWrites.length===${index * 3 + 3}`);
    assert.deepEqual(
      await evaluate(`qa.demoWrites[${index * 3 + 2}].body.answer`),
      answers[index],
    );
    await evaluate(
      `qa.demoWrites[${index * 3 + 2}].release({correct:true,feedbackZh:'确认完成'})`,
    );
    await browser(
      "wait",
      "--fn",
      "document.activeElement.matches('.practice-feedback')",
    );
    await browser("focus", ".practice-next .primary");
    await press("Enter");
    await browser(
      "wait",
      "--fn",
      `document.querySelector('.review-progress')?.getAttribute('aria-valuenow')==='${index + 1}' || !!document.querySelector('.practice-recap')`,
    );
  }
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "本次练习",
  );
  assert.equal(
    await evaluate("document.querySelector('.practice-recap').children.length"),
    3,
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.practice-intro').textContent.replace(/\\s+/g,' ').trim()",
    ),
    "完成了 3 道题，其中 3 道答对。",
  );
});

test("leaving guest practice aborts grading and old responses cannot change a fresh session", async () => {
  for (const oldOutcome of [{ correct: true, feedbackZh: "旧反馈" }, 503]) {
    await open("demo");
    await browser("wait", ".exercise-sheet input[type=radio]");
    await browser("focus", ".exercise-sheet input[type=radio]");
    await press("Space");
    await browser("focus", ".exercise-sheet button[type=submit]");
    await press("Enter");
    await browser("wait", "--fn", "qa.demoWrites.length===1");
    await evaluate("qa.navigate('/login')");
    await browser("wait", "--fn", "qa.route==='/login'");
    assert.equal(await evaluate("qa.demoWrites[0].signal.aborted"), true);
    await evaluate("qa.navigate('/')");
    await browser("wait", ".exercise-sheet input[type=radio]");
    assert.equal(
      await evaluate("document.querySelectorAll('input:checked').length"),
      0,
    );
    await browser("focus", ".exercise-sheet input[type=radio]");
    await press("Space");
    await browser("focus", ".exercise-sheet button[type=submit]");
    await press("Enter");
    await browser("wait", "--fn", "qa.demoWrites.length===2");
    await evaluate(`qa.demoWrites[0].release(${JSON.stringify(oldOutcome)})`);
    assert.equal(
      await evaluate(
        "document.querySelector('.exercise-sheet fieldset').disabled",
      ),
      true,
    );
    assert.equal(
      await evaluate(
        "document.querySelectorAll('.practice-feedback,.error-message').length",
      ),
      0,
    );
    assert.equal(
      await evaluate("document.querySelector('.toast').hidden"),
      true,
    );
    await evaluate(
      "qa.demoWrites[1].release({correct:false,feedbackZh:'当前反馈'})",
    );
    await browser("wait", ".practice-feedback");
    assert.equal(
      await evaluate(
        "document.querySelector('.practice-feedback p').textContent",
      ),
      "当前反馈",
    );
  }
});

test("overlay scrollbars preserve width through content changes and support native keyboard scrolling", async () => {
  await open("scrollbar");
  for (const width of [320, 390, 768, 1440]) {
    await browser("set", "viewport", String(width), "740");
    await browser("wait", ".page-scrollbar:not([hidden])");
    assert.deepEqual(
      await evaluate(
        "({width:document.documentElement.clientWidth,overflow:document.documentElement.scrollWidth,style:getComputedStyle(document.documentElement).scrollbarWidth,position:getComputedStyle(document.querySelector('.page-scrollbar')).position})",
      ),
      { width, overflow: width, style: "none", position: "fixed" },
    );
    await browser("focus", ".page-scrollbar");
    await press("End");
    await browser(
      "wait",
      "--fn",
      "Number(document.querySelector('.page-scrollbar').getAttribute('aria-valuenow'))===Number(document.querySelector('.page-scrollbar').getAttribute('aria-valuemax'))",
    );
    assert.equal(
      await evaluate("document.activeElement.matches('.page-scrollbar')"),
      true,
    );
    await press("Home");
    await browser(
      "wait",
      "--fn",
      "Number(document.querySelector('.page-scrollbar').getAttribute('aria-valuenow'))===0",
    );
    await browser("focus", ".scroll-toggle");
    await press("Enter");
    await browser(
      "wait",
      "--fn",
      "document.querySelector('.page-scrollbar').hidden",
    );
    assert.equal(await evaluate("document.documentElement.clientWidth"), width);
    await press("Enter");
    await browser("wait", ".page-scrollbar:not([hidden])");
    assert.equal(await evaluate("document.documentElement.clientWidth"), width);
  }
});

test("all exercise kinds keep keyboard focus and exact answers through pending grading and retry", async () => {
  await open("author");
  await browser("wait", ".exercise-sheet input[type=radio]");
  const selectors = [
    ".exercise-sheet:has(input[type=radio])",
    ".exercise-sheet:has(.practice-input)",
    ".exercise-sheet:has(.order-bank)",
  ];
  for (const [index, selector] of selectors.entries()) {
    if (index === 0) {
      await browser("focus", selector + " input[type=radio]");
      await press("Space");
    } else if (index === 1) {
      await browser("focus", selector + " .practice-input");
      await browser("keyboard", "inserttext", "une");
    } else {
      await browser("focus", selector + " .order-bank button");
      await press("Enter");
      await press("Enter");
    }
    await browser("focus", selector + " .primary");
    await press("Enter");
    await browser("wait", "--fn", `qa.previewWrites.length===${index * 2 + 1}`);
    assert.deepEqual(await evaluate(`qa.previewWrites[${index * 2}].body`), {
      revision: 1,
      exerciseId: ["preview-choice", "preview-text", "preview-order"][index],
      answer: [
        { kind: "choice", optionId: "bonjour" },
        { kind: "text", text: "une" },
        { kind: "order", tokenIds: ["bonjour", "luc"] },
      ][index],
    });
    assert.equal(
      await evaluate(
        `document.activeElement.matches(${JSON.stringify(selector + " .primary")})`,
      ),
      true,
    );
    assert.equal(
      await evaluate("document.activeElement.getAttribute('aria-busy')"),
      "true",
    );
    await press("Enter");
    await press("Enter");
    assert.equal(await evaluate("qa.previewWrites.length"), index * 2 + 1);
    await evaluate(`qa.previewWrites[${index * 2}].release(503)`);
    await browser(
      "wait",
      "--fn",
      `!document.querySelector(${JSON.stringify(selector + " fieldset")}).disabled`,
    );
    assert.equal(
      await evaluate(
        `document.activeElement.matches(${JSON.stringify(selector + " .primary")})`,
      ),
      true,
    );
    await press("Enter");
    await browser("wait", "--fn", `qa.previewWrites.length===${index * 2 + 2}`);
    assert.equal(
      await evaluate(
        `JSON.stringify(qa.previewWrites[${index * 2}].body)===JSON.stringify(qa.previewWrites[${index * 2 + 1}].body)`,
      ),
      true,
    );
    await evaluate(
      `qa.previewWrites[${index * 2 + 1}].release({correct:true,feedbackZh:'确认完成'})`,
    );
    await browser("wait", selector + " .practice-feedback");
    await browser(
      "wait",
      "--fn",
      `document.activeElement.matches(${JSON.stringify(selector + " .practice-feedback")})`,
    );
  }
});

test("author preview grading isolates a replaced operator and ignores old results", async () => {
  await open("author");
  await browser("wait", ".exercise-sheet input[type=radio]");
  await browser("focus", ".exercise-sheet input[type=radio]");
  await press("Space");
  await browser("focus", ".exercise-sheet .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.previewWrites.length===1");
  await evaluate("qa.changeUser()");
  await browser(
    "wait",
    "--fn",
    "!document.querySelector('.exercise-sheet fieldset').disabled",
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.exercise-sheet input:checked').length",
    ),
    0,
  );
  assert.equal(await evaluate("qa.previewWrites[0].signal.aborted"), true);
  await browser("focus", ".exercise-sheet input[type=radio]");
  await press("Space");
  await browser("focus", ".exercise-sheet .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.previewWrites.length===2");
  await evaluate(
    "qa.previewWrites[0].release({correct:true,feedbackZh:'Old operator result'})",
  );
  assert.equal(
    await evaluate(
      "document.querySelector('.exercise-sheet fieldset').disabled",
    ),
    true,
  );
  assert.equal(
    await evaluate("document.querySelectorAll('.practice-feedback').length"),
    0,
  );
  await evaluate(
    "qa.previewWrites[1].release({correct:false,feedbackZh:'Current operator result'})",
  );
  await browser("wait", ".practice-feedback");
  assert.equal(
    await evaluate(
      "document.querySelector('.practice-feedback p').textContent",
    ),
    "Current operator result",
  );
  assert.equal(
    await evaluate(
      "document.activeElement.classList.contains('practice-feedback')",
    ),
    true,
  );
});

test("changing an author preview cancels pending CSRF before grading the old revision", async () => {
  await open("author");
  await browser("wait", ".exercise-sheet input[type=radio]");
  await evaluate("qa.deferAuthBootstrap=true");
  await browser("focus", ".exercise-sheet input[type=radio]");
  await press("Space");
  await browser("focus", ".exercise-sheet .primary");
  await press("Enter");
  await browser("wait", "--fn", "qa.authBootstraps.length===1");
  await browser("focus", ".course-directory .lesson-list li:nth-child(2) a");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "new URLSearchParams(qa.search).get('lessonId')==='qa-second'",
  );
  assert.equal(await evaluate("qa.authBootstraps[0].signal.aborted"), true);
  await evaluate("qa.authBootstraps[0].release(200)");
  assert.equal(await evaluate("qa.previewWrites.length"), 0);
  assert.equal(
    await evaluate(
      "document.querySelector('.exercise-sheet fieldset').disabled",
    ),
    false,
  );
  assert.equal(
    await evaluate(
      "document.querySelectorAll('.exercise-sheet input:checked').length",
    ),
    0,
  );
  assert.equal(await evaluate("document.querySelector('.toast').hidden"), true);
});

test("author preview resets edited opener fields to the selected immutable context and focuses loaded content", async () => {
  await open("author");
  await browser("wait", ".author-preview > div:last-child .lesson-header h2");
  for (const [name, value] of [
    ["releaseId", "draft-release"],
    ["lessonId", "draft-lesson"],
    ["revision", "9"],
  ]) {
    await browser("focus", `.auth-form input[name=${name}]:not([type=hidden])`);
    await press("Control+a");
    await browser("keyboard", "inserttext", value);
  }
  await browser("focus", ".course-directory .lesson-list li:nth-child(2) a");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "new URLSearchParams(qa.search).get('lessonId')==='qa-second'",
  );
  assert.deepEqual(
    await evaluate(
      "[...document.querySelectorAll('.auth-form input:not([type=hidden])')].map(e=>e.value)",
    ),
    ["qa-release-one", "qa-second", "2"],
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "Une autre conversation",
  );
  await browser("focus", ".auth-form input[name=releaseId]:not([type=hidden])");
  await press("Control+a");
  await browser("keyboard", "inserttext", "qa-release-two");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "new URLSearchParams(qa.search).get('releaseId')==='qa-release-two'",
  );
  assert.deepEqual(
    await evaluate(
      "[...document.querySelectorAll('.auth-form input:not([type=hidden])')].map(e=>e.value)",
    ),
    ["qa-release-two", "", "1"],
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "qa-release-two",
  );
  await browser("focus", ".course-directory .lesson-list li:nth-child(2) a");
  await press("Enter");
  await browser(
    "wait",
    "--fn",
    "new URLSearchParams(qa.search).get('revision')==='3'",
  );
  assert.deepEqual(
    await evaluate(
      "[...document.querySelectorAll('.auth-form input:not([type=hidden])')].map(e=>e.value)",
    ),
    ["qa-release-two", "qa-second", "3"],
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "Une autre conversation",
  );
  await evaluate("qa.navigate(-1)");
  await browser(
    "wait",
    "--fn",
    "!new URLSearchParams(qa.search).has('lessonId')",
  );
  assert.deepEqual(
    await evaluate(
      "[...document.querySelectorAll('.auth-form input:not([type=hidden])')].map(e=>e.value)",
    ),
    ["qa-release-two", "", "1"],
  );
  assert.equal(
    await evaluate("document.activeElement.textContent"),
    "qa-release-two",
  );
});
