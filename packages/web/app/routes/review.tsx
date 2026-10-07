import { useRef, useState } from "react";
import { Link, redirect } from "react-router";
import { getCatalog, getLesson } from "../lib/api.server";
import { useLearning } from "../components/learning";
import { knowledgeUnit } from "../lib/recording-playback";
import { Icon } from "../components/icon";
import type { Route } from "./+types/review";
export async function loader({ params }: Route.LoaderArgs) {
  const catalog = await getCatalog();
  if (!catalog.developmentFixture) throw redirect("/reviews");
  return { lesson: await getLesson(params.lessonId) };
}
export default function Review({
  loaderData: { lesson },
}: Route.ComponentProps) {
  const all = lesson.knowledge.vocabulary.filter((v) =>
      lesson.reviewItemIds.includes(v.id),
    ),
    [queue, setQueue] = useState(all),
    [index, setIndex] = useState(0),
    [revealed, setRevealed] = useState(false),
    [results, setResults] = useState<{ id: string; grade: number }[]>([]),
    learning = useLearning(),
    card = useRef<HTMLButtonElement>(null),
    motion = useRef<Animation | null>(null);
  const term = queue[index],
    done = !term,
    labels = ["还不熟", "有印象", "记住了"],
    weak = results.filter((r) => r.grade < 2),
    title = term ? (term.gender === "feminine" ? "une " : "") + term.lemma : "";
  function restart(onlyWeak = false) {
    learning.stop();
    setQueue(
      onlyWeak ? all.filter((t) => weak.some((r) => r.id === t.id)) : all,
    );
    setIndex(0);
    setResults([]);
    setRevealed(false);
  }
  function reveal() {
    const start = card.current?.getBoundingClientRect().height;
    motion.current?.cancel();
    setRevealed(!revealed);
    if (!revealed)
      learning.play([knowledgeUnit(term.id, term.lemma, term.recording)]);
    else learning.stop();
    requestAnimationFrame(() => {
      if (
        card.current &&
        start &&
        !matchMedia("(prefers-reduced-motion:reduce)").matches
      )
        motion.current = card.current.animate(
          [
            { height: start + "px" },
            { height: card.current.getBoundingClientRect().height + "px" },
          ],
          { duration: 420, easing: "cubic-bezier(.22,.8,.25,1)" },
        );
    });
  }
  const dialogue = lesson.blocks.find((b) => b.type === "dialogue"),
    example =
      dialogue?.type === "dialogue"
        ? dialogue.turns.find((e) =>
            e.segments.some((s) => s.vocabularyId === term?.id),
          )
        : null;
  return (
    <section className="review-page page-arrive">
      <div className="review-session-header">
        <div>
          <h1>复习</h1>
          <p>{lesson.title.zh}</p>
        </div>
        <span className="small">
          {Math.min(index + 1, queue.length)} / {queue.length}
        </span>
      </div>
      <div className="review-progress">
        <span
          style={{
            width: queue.length ? (index / queue.length) * 100 + "%" : "100%",
          }}
        />
      </div>
      {done ? (
        <div className="review-summary">
          <h2>本轮回顾</h2>
          <p>本轮复习了 {results.length} 个表达。</p>
          <div className="review-stats">
            {labels.map((label, grade) => (
              <div key={label}>
                <b>{results.filter((r) => r.grade === grade).length}</b>
                <span>{label}</span>
              </div>
            ))}
          </div>
          <ul className="review-result-list">
            {results.map((r) => {
              const v = all.find((v) => v.id === r.id)!;
              return (
                <li key={r.id}>
                  <div>
                    <span className="result-expression" lang="fr">
                      {v.lemma}
                    </span>
                    <small>{v.meaningZh}</small>
                  </div>
                  <span className="review-result-grade">{labels[r.grade]}</span>
                </li>
              );
            })}
          </ul>
          <div className="review-summary-actions">
            {weak.length > 0 && (
              <button
                className="primary summary-main"
                onClick={() => restart(true)}
              >
                再练未熟项
              </button>
            )}
            <button
              className={weak.length ? "text-button" : "primary summary-main"}
              onClick={() => restart()}
            >
              再来一轮
            </button>
            <Link className="text-button" to={"/lessons/" + lesson.id}>
              回到课程
            </Link>
          </div>
          <small className="review-summary-note">
            本轮结果尚未保存到账号。
          </small>
        </div>
      ) : (
        <>
          <div className="review-context">
            <span>
              <Icon name="book" />
              {lesson.title.zh}
            </span>
          </div>
          <button
            ref={card}
            className="review-flashcard"
            aria-expanded={revealed}
            onClick={reveal}
          >
            <span className="review-kind">
              {term.partOfSpeech === "phrase" ? "常用表达" : "日常词汇"}
            </span>
            <span className="review-expression" lang="fr">
              {title}
            </span>
            {revealed && (
              <span className="review-solution">
                <span className="review-meaning">{term.meaningZh}</span>
                <span className="review-explanation">{term.noteZh}</span>
                {example && (
                  <span className="review-example">
                    <span className="review-example-fr" lang="fr">
                      {example.segments.map((s) => s.text).join("")}
                    </span>
                    <span className="review-example-zh">
                      {example.translationZh}
                    </span>
                  </span>
                )}
              </span>
            )}
          </button>
          {revealed && (
            <div
              className="review-ratings review-choices-enter"
              role="group"
              aria-label="这次回想的感觉"
            >
              {labels.map((label, grade) => (
                <button
                  key={grade}
                  data-grade={grade}
                  onClick={() => {
                    learning.stop();
                    setResults([...results, { id: term.id, grade }]);
                    setRevealed(false);
                    setIndex(index + 1);
                  }}
                >
                  <span className="rating-dot" aria-hidden="true" />
                  <span>{label}</span>
                  <Icon name="chevron" />
                </button>
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}
