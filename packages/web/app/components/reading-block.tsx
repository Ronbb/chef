import { useEffect, useId, useRef, useState } from "react";
import type { Block } from "@brioche/contracts/Block";
import type { NeutralBlock } from "@brioche/contracts/NeutralBlock";
import {
  lessonLanguage,
  segmentText,
  targetText,
  exampleText,
  type ReadingLesson,
  type ReadingVocabulary,
  type ReadingGrammar,
} from "../lib/reading-model";
import { ReadingTextLabel } from "./reading-text";
import {
  Sentence,
  avatar,
  avatarFallback,
  avatarReady,
} from "../routes/lesson";
import { Player, useLearning } from "./learning";
import { Icon } from "./icon";
import {
  readingScope,
  readingUnits,
  knowledgeUnit,
} from "../lib/recording-playback";
export function ReadingBlock({
  block,
  lesson,
  personalActions = true,
}: {
  block: Extract<Block | NeutralBlock, { type: "dialogue" | "article" }>;
  lesson: ReadingLesson;
  personalActions?: boolean;
}) {
  const learning = useLearning(),
    dialog = useRef<HTMLDialogElement>(null),
    knowledgeTitleId = useId();
  const [revealed, setRevealed] = useState(new Set<string>()),
    [term, setTerm] = useState<ReadingVocabulary | null>(null),
    [grammar, setGrammar] = useState<ReadingGrammar | null>(null);
  const entries = block.type === "dialogue" ? block.turns : block.paragraphs;
  function showTerm(value: ReadingVocabulary) {
    setTerm(value);
    setGrammar(null);
    dialog.current?.showModal();
  }
  function showGrammar(value: ReadingGrammar) {
    setGrammar(value);
    setTerm(null);
    if (!dialog.current?.open) dialog.current?.showModal();
  }
  const units = readingUnits(lesson, block);
  const currentLearning = useRef(learning);
  currentLearning.current = learning;
  const scope = readingScope(lesson, block.id);
  useEffect(
    () => () => {
      const value = currentLearning.current;
      if (
        value.player.owner?.startsWith(scope) ||
        value.player.id?.startsWith(scope)
      )
        value.stop();
    },
    [scope],
  );
  return (
    <div className="reading session-reading">
      <h3 className="reading-block-title">{block.titleZh}</h3>
      <Player units={units} />
      {block.type === "dialogue" && (
        <ul className="reading-characters">
          {block.speakers.map((speaker) => (
            <li key={speaker.id}>
              <img
                src={avatar(speaker.avatarId, lesson)}
                alt=""
                ref={avatarReady}
                onError={avatarFallback}
              />
              <div>
                <span lang={lessonLanguage(lesson)}>{speaker.displayName}</span>
                <small>{speaker.labelZh}</small>
              </div>
            </li>
          ))}
        </ul>
      )}
      {entries.map((entry, index) => {
        const speaker =
          block.type === "dialogue" && "speakerId" in entry
            ? block.speakers.find((speaker) => speaker.id === entry.speakerId)
            : null;
        const id = units[index].id;
        return (
          <div
            key={entry.id}
            className={
              (speaker ? "dialogue-turn" : "article-paragraph") +
              (learning.player.id === id ? " is-speaking" : "")
            }
          >
            {speaker && (
              <button
                className="speaker"
                aria-label={speaker.displayName + "：译文与朗读"}
                onClick={() => {
                  setRevealed((old) => new Set([...old, entry.id]));
                  learning.play([
                    {
                      ...units[index],
                      locale: lesson.cast.find(
                        (character) =>
                          character.characterId === speaker.characterId,
                      )?.speechLocale,
                    },
                  ]);
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
                blockId={block.id}
                entryId={entry.id}
                onTerm={showTerm}
                onGrammar={showGrammar}
              />
              {(learning.translation || revealed.has(entry.id)) && (
                <p className="translation">{entry.translationZh}</p>
              )}
            </div>
          </div>
        );
      })}
      <dialog
        className="knowledge-dialog"
        ref={dialog}
        aria-labelledby={knowledgeTitleId}
        onClick={(event) => {
          if (event.target !== dialog.current || !dialog.current) return;
          const rect = dialog.current.getBoundingClientRect();
          if (
            event.clientX < rect.left ||
            event.clientX > rect.right ||
            event.clientY < rect.top ||
            event.clientY > rect.bottom
          )
            dialog.current.close();
        }}
      >
        <div className="rate-heading">
          <span className="knowledge-label">
            {grammar ? "语法" : "表达与词汇"}
          </span>
          <button
            className="icon-button"
            aria-label="关闭解释"
            onClick={() => dialog.current?.close()}
          >
            <Icon name="close" />
          </button>
        </div>
        <h2
          id={knowledgeTitleId}
          lang={grammar ? "zh-CN" : lessonLanguage(lesson)}
        >
          {grammar
            ? grammar.titleZh
            : term && <ReadingTextLabel reading={term.lemma} />}
        </h2>
        {term && (
          <>
            <p className="meaning">{term.meaningZh}</p>
            <p className="explain">{term.noteZh}</p>
            {personalActions && (
              <Bookmark
                key={term.id}
                knowledgeId={term.id}
                lessonId={lesson.id}
                revision={lesson.revision}
              />
            )}
            {personalActions && (
              <Enroll
                key={"enroll-" + term.id}
                knowledgeId={term.id}
                lessonId={lesson.id}
                revision={lesson.revision}
              />
            )}
            <div className="example" lang={lessonLanguage(lesson)}>
              {entries
                .find((entry) =>
                  entry.segments.some(
                    (segment) => segment.vocabularyId === term.id,
                  ),
                )
                ?.segments.map(segmentText)
                .join("")}
            </div>
          </>
        )}
        {grammar && (
          <>
            <p className="explain">{grammar.bodyZh}</p>
            {grammar.examples.map((example, index) => (
              <div className="grammar-example" key={index}>
                <button
                  lang={lessonLanguage(lesson)}
                  onClick={() =>
                    learning.play([
                      knowledgeUnit(
                        scope + "grammar:" + grammar.id + ":" + index,
                        targetText(exampleText(example)),
                        example.recording,
                      ),
                    ])
                  }
                >
                  <ReadingTextLabel reading={exampleText(example)} />
                </button>
                <p>{example.zh}</p>
              </div>
            ))}
          </>
        )}
      </dialog>
    </div>
  );
}
import { Bookmark } from "./bookmark";
import { Enroll } from "./enroll";
