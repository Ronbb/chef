import { useId, useRef, useState, type SyntheticEvent } from "react";
import { Link } from "react-router";
import type { Segment } from "@brioche/contracts/Segment";
import type { PublicLesson } from "@brioche/contracts/PublicLesson";
import type { Vocabulary } from "@brioche/contracts/Vocabulary";
import type { Grammar } from "@brioche/contracts/Grammar";
import { TeachingBlock } from "../components/teaching-block";
import { getCatalog, getLesson } from "../lib/api.server";
import { StartLearning } from "../components/start-learning";
import { Player, useLearning } from "../components/learning";
import { Icon } from "../components/icon";
import { ResponsiveKnowledge } from "../components/responsive-knowledge";
import { PendingNavigation } from "../components/pending-navigation";
import { usePendingOwnedWrites } from "../components/pending-owned-writes";
import {
  readingUnits,
  wordUnit,
  knowledgeUnit,
} from "../lib/recording-playback";
import type { Route } from "./+types/lesson";
export async function loader({ params }: Route.LoaderArgs) {
  const [lesson, catalog] = await Promise.all([
    getLesson(params.lessonId),
    getCatalog(),
  ]);
  return { lesson, demo: catalog.developmentFixture };
}
export const avatar = (id: string, lesson?: PublicLesson) =>
  lesson?.media.find((asset) => asset.assetId === id)?.url ??
  "/assets/avatars/" +
    ({
      "avatar-camille-v1": "camille",
      "avatar-luc-v1": "luc",
      "avatar-lea-v1": "lea",
    }[id] ?? "learner") +
    ".svg";
export function avatarFallback(event: SyntheticEvent<HTMLImageElement>) {
  replaceAvatar(event.currentTarget);
}
export function avatarReady(image: HTMLImageElement | null) {
  if (image?.complete && image.naturalWidth === 0) replaceAvatar(image);
}
function replaceAvatar(image: HTMLImageElement) {
  const fallback = "/assets/avatars/learner.svg";
  if (image.getAttribute("src") !== fallback) image.src = fallback;
}
export function Sentence({
  segments,
  lesson,
  onTerm,
  onGrammar,
  blockId,
  entryId,
}: {
  segments: Segment[];
  lesson: PublicLesson;
  onTerm: (v: Vocabulary) => void;
  onGrammar: (v: Grammar) => void;
  blockId: string;
  entryId: string;
}) {
  const learning = useLearning();
  return (
    <span className="sentence" lang="fr">
      {segments.flatMap((segment) =>
        Array.from(
          new Intl.Segmenter("fr", { granularity: "word" }).segment(
            segment.text,
          ),
        ).map((token, i) =>
          token.isWordLike ? (
            <button
              key={segment.id + i}
              className={
                "word" +
                (segment.vocabularyId || segment.grammarId ? " known" : "") +
                (learning.player.wordId ===
                wordUnit(
                  lesson,
                  blockId,
                  entryId,
                  segment.id,
                  token.segment,
                  segment.text,
                  token.index,
                ).id
                  ? " is-speaking"
                  : "")
              }
              onClick={() => {
                learning.play([
                  wordUnit(
                    lesson,
                    blockId,
                    entryId,
                    segment.id,
                    token.segment,
                    segment.text,
                    token.index,
                  ),
                ]);
                const term = lesson.knowledge.vocabulary.find(
                  (v) => v.id === segment.vocabularyId,
                );
                if (term) onTerm(term);
                else {
                  const grammar = lesson.knowledge.grammar.find(
                    (g) => g.id === segment.grammarId,
                  );
                  if (grammar) onGrammar(grammar);
                }
              }}
            >
              {token.segment}
            </button>
          ) : (
            <span key={segment.id + i}>{token.segment}</span>
          ),
        ),
      )}
    </span>
  );
}
export default function Lesson({
  loaderData: { lesson, demo },
}: Route.ComponentProps) {
  return (
    <LessonContent
      key={lesson.id + ":" + lesson.revision}
      lesson={lesson}
      demo={demo}
    />
  );
}
function LessonContent({
  lesson,
  demo,
}: {
  lesson: PublicLesson;
  demo: boolean;
}) {
  const bodies = lesson.blocks.filter(
    (block) => block.type === "dialogue" || block.type === "article",
  );
  const learning = useLearning(),
    [selectedBody, setSelectedBody] = useState(
      (bodies.find((block) => block.type === "dialogue") ?? bodies[0])?.id,
    ),
    [revealed, setRevealed] = useState<Set<string>>(new Set()),
    [term, setTerm] = useState<Vocabulary | null>(null);
  const [grammar, setGrammar] = useState<Grammar | null>(null);
  const pending = usePendingOwnedWrites(learning.profile?.id),
    heading = useRef<HTMLHeadingElement>(null);
  const showTerm = (value: Vocabulary) => {
    setGrammar(null);
    setTerm(value);
  };
  const showGrammar = (value: Grammar) => {
    setTerm(null);
    setGrammar(value);
  };
  const closeNote = () => {
    setTerm(null);
    setGrammar(null);
  };
  const body = bodies.find((block) => block.id === selectedBody) ?? bodies[0];
  const mode = body?.type;
  const dialogue = body?.type === "dialogue" ? body : undefined,
    article = body?.type === "article" ? body : undefined;
  const entries =
    mode === "dialogue" && dialogue?.type === "dialogue"
      ? dialogue.turns
      : article?.type === "article"
        ? article.paragraphs
        : [];
  const bodyId = useId();
  const units =
    body && (body.type === "dialogue" || body.type === "article")
      ? readingUnits(lesson, body)
      : [];
  function changeMode(next: string) {
    learning.stop();
    setSelectedBody(next);
    closeNote();
  }
  return (
    <section className="page-arrive">
      <PendingNavigation
        active={pending}
        onStay={() => heading.current?.focus({ preventScroll: true })}
      />
      <div className="lesson-header">
        <div className="crumb">
          {lesson.levelId.toUpperCase()} / {lesson.title.zh}
        </div>
        <h1 lang="fr" ref={heading} tabIndex={-1}>
          {lesson.title.fr}
        </h1>
        {lesson.blocks
          .filter((b) => b.type === "scene")
          .map((b) => (
            <TeachingBlock key={b.id} block={b} lesson={lesson} />
          ))}
        <Player units={units} />
      </div>
      <div className="reading-layout">
        <div className="reading">
          {bodies.length > 1 && (
            <div className="reading-tabs" role="tablist" aria-label="正文">
              {bodies.map((value, index) => (
                <button
                  key={value.id}
                  id={`${bodyId}-${value.id}`}
                  role="tab"
                  aria-selected={body?.id === value.id}
                  aria-controls={bodyId}
                  tabIndex={body?.id === value.id ? 0 : -1}
                  onKeyDown={(event) => {
                    let next = index;
                    if (event.key === "ArrowRight")
                      next = (index + 1) % bodies.length;
                    else if (event.key === "ArrowLeft")
                      next = (index + bodies.length - 1) % bodies.length;
                    else if (event.key === "Home") next = 0;
                    else if (event.key === "End") next = bodies.length - 1;
                    else return;
                    event.preventDefault();
                    changeMode(bodies[next].id);
                    const tab =
                      event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>(
                        '[role="tab"]',
                      )[next];
                    tab?.focus({ preventScroll: true });
                    tab?.scrollIntoView({
                      block: "nearest",
                      inline: "nearest",
                    });
                  }}
                  onClick={() => changeMode(value.id)}
                >
                  {bodies.filter((block) => block.type === value.type).length >
                  1
                    ? value.titleZh
                    : value.type === "dialogue"
                      ? "对话"
                      : "短文"}
                </button>
              ))}
            </div>
          )}
          <div
            id={bodyId}
            role={bodies.length > 1 ? "tabpanel" : undefined}
            aria-labelledby={
              bodies.length > 1 ? `${bodyId}-${body?.id}` : undefined
            }
          >
            {mode === "dialogue" && dialogue?.type === "dialogue" && (
              <ul className="reading-characters">
                {dialogue.speakers.map((s) => (
                  <li key={s.id}>
                    <img
                      src={avatar(s.avatarId, lesson)}
                      alt=""
                      ref={avatarReady}
                      onError={avatarFallback}
                    />
                    <div>
                      <span lang="fr">{s.displayName}</span>
                      <small>{s.labelZh}</small>
                    </div>
                  </li>
                ))}
              </ul>
            )}
            {entries.map((entry, index) => {
              const speaker =
                "speakerId" in entry && dialogue?.type === "dialogue"
                  ? dialogue.speakers.find((s) => s.id === entry.speakerId)
                  : null;
              return (
                <div
                  key={entry.id}
                  className={
                    (speaker ? "dialogue-turn" : "article-paragraph") +
                    (learning.player.id === units[index]?.id
                      ? " is-speaking"
                      : "")
                  }
                >
                  {speaker && (
                    <button
                      className="speaker"
                      aria-label={speaker.displayName + "：译文与朗读"}
                      onClick={() => {
                        setRevealed(
                          (old) => new Set([...old, body!.id + ":" + entry.id]),
                        );
                        learning.play([units[index]]);
                      }}
                    >
                      <img
                        src={avatar(speaker.avatarId, lesson)}
                        alt=""
                        ref={avatarReady}
                        onError={avatarFallback}
                      />
                    </button>
                  )}
                  <div>
                    <Sentence
                      segments={entry.segments}
                      lesson={lesson}
                      blockId={body!.id}
                      entryId={entry.id}
                      onTerm={showTerm}
                      onGrammar={showGrammar}
                    />
                    {(learning.translation ||
                      revealed.has(body!.id + ":" + entry.id)) && (
                      <p className="translation">{entry.translationZh}</p>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
          <div className="lesson-explore">
            {lesson.steps
              .filter((s) => s.kind === "explore")
              .flatMap((s) => s.blockIds)
              .map((id) => {
                const block = lesson.blocks.find((b) => b.id === id)!;
                if (block.type === "dialogue" || block.type === "article")
                  return null;
                if (block.type === "exercise")
                  return (
                    <div className="lesson-note" key={id}>
                      <p>{block.promptZh}</p>
                      <LessonLearningEntry lessonId={lesson.id} demo={demo} />
                    </div>
                  );
                return <TeachingBlock key={id} block={block} lesson={lesson} />;
              })}
          </div>
          <div className="reading-footer">
            <LessonLearningEntry lessonId={lesson.id} demo={demo} />
          </div>
        </div>
        <ResponsiveKnowledge
          open={!!(term || grammar)}
          onDismiss={closeNote}
          labelledBy={`${bodyId}-knowledge-title`}
        >
          <button
            className="icon-button note-close"
            aria-label="关闭解释"
            onClick={closeNote}
          >
            <Icon name="close" />
          </button>
          {grammar ? (
            <>
              <span className="knowledge-label">语法</span>
              <h2 id={`${bodyId}-knowledge-title`}>{grammar.titleZh}</h2>
              <p className="explain">{grammar.bodyZh}</p>
              {grammar.examples.map((e, i) => (
                <div className="grammar-example" key={i}>
                  <button
                    lang="fr"
                    onClick={() =>
                      learning.play([
                        knowledgeUnit(grammar.id + ":" + i, e.fr, e.recording),
                      ])
                    }
                  >
                    {e.fr}
                  </button>
                  <p>{e.zh}</p>
                </div>
              ))}
            </>
          ) : term ? (
            <>
              <span className="knowledge-label">表达与词汇</span>
              <h2 id={`${bodyId}-knowledge-title`} lang="fr">
                {term.lemma}
              </h2>
              <p className="meaning">{term.meaningZh}</p>
              <p className="explain">{term.noteZh}</p>
              <Bookmark
                key={term.id}
                knowledgeId={term.id}
                lessonId={lesson.id}
                revision={lesson.revision}
              />
              <Enroll
                key={"enroll-" + term.id}
                knowledgeId={term.id}
                lessonId={lesson.id}
                revision={lesson.revision}
              />
              {lesson.knowledge.grammar
                .filter((g) =>
                  entries.some((e) =>
                    e.segments.some(
                      (s) => s.vocabularyId === term.id && s.grammarId === g.id,
                    ),
                  ),
                )
                .map((g) => (
                  <button
                    className="text-button"
                    key={g.id}
                    onClick={() => showGrammar(g)}
                  >
                    {g.titleZh}
                  </button>
                ))}
              <div className="example" lang="fr">
                {entries
                  .find((e) =>
                    e.segments.some((s) => s.vocabularyId === term.id),
                  )
                  ?.segments.map((s) => s.text)
                  .join("")}
              </div>
            </>
          ) : (
            <>
              <span className="knowledge-label">本课表达</span>
              <h2 id={`${bodyId}-knowledge-title`} lang="fr">
                {lesson.knowledge.vocabulary[1]?.lemma}
              </h2>
              <p className="meaning">
                {lesson.knowledge.vocabulary[1]?.meaningZh}
              </p>
            </>
          )}
        </ResponsiveKnowledge>
      </div>
    </section>
  );
}
import { Bookmark } from "../components/bookmark";
import { Enroll } from "../components/enroll";

function LessonLearningEntry({
  lessonId,
  demo,
}: {
  lessonId: string;
  demo: boolean;
}) {
  const learning = useLearning();
  return learning.profile ? (
    <StartLearning lessonId={lessonId}>开始或继续学习</StartLearning>
  ) : (
    <Link
      className="primary"
      to={
        demo
          ? "/practice/" + lessonId
          : "/login?next=" + encodeURIComponent("/lessons/" + lessonId)
      }
    >
      {demo ? "练习" : "登录后开始学习"}
    </Link>
  );
}
