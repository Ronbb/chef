import type { Block } from "@brioche/contracts/Block";
import type { NeutralBlock } from "@brioche/contracts/NeutralBlock";
import {
  lessonLanguage,
  targetText,
  exampleText,
  type ReadingLesson,
} from "../lib/reading-model";
import { ReadingTextLabel } from "./reading-text";
import { useLearning } from "./learning";
import { Illustration, illustration } from "./illustration";
import { LessonNote } from "./lesson-note";
import { knowledgeUnit } from "../lib/recording-playback";

type TeachingBlock = Exclude<
  Block | NeutralBlock,
  { type: "dialogue" | "article" | "exercise" }
>;
export function TeachingBlock({
  block,
  lesson,
}: {
  block: TeachingBlock;
  lesson: ReadingLesson;
}) {
  const learning = useLearning();
  switch (block.type) {
    case "scene":
      return (
        <div className="lesson-scene">
          {illustration(lesson, block.illustrationId) && (
            <Illustration
              className="scene-illustration"
              asset={illustration(lesson, block.illustrationId)!}
            />
          )}
          <p className="knowledge-label">{block.placeZh}</p>
          <p>{block.situationZh}</p>
        </div>
      );
    case "explanation":
      return (
        <LessonNote kind="explanation" title={block.titleZh}>
          <p>{block.bodyZh}</p>
        </LessonNote>
      );
    case "culture":
      return (
        <LessonNote kind="culture" title={block.titleZh}>
          <p>{block.bodyZh}</p>
          <p className="profile-note">{block.scopeZh}</p>
        </LessonNote>
      );
    case "vocabulary":
      return (
        <LessonNote kind="vocabulary" title="表达与词汇">
          <dl className="vocabulary-list">
            {block.entryIds.map((id) => {
              const word = lesson.knowledge.vocabulary.find(
                (w) => w.id === id,
              )!;
              return (
                <div key={id}>
                  <dt>
                    <button
                      type="button"
                      lang={lessonLanguage(lesson)}
                      onClick={() =>
                        learning.play([
                          knowledgeUnit(
                            id,
                            targetText(word.lemma),
                            word.recording,
                          ),
                        ])
                      }
                    >
                      <ReadingTextLabel reading={word.lemma} />
                    </button>
                  </dt>
                  <dd>
                    <strong>{word.meaningZh}</strong>
                    <p>{word.noteZh}</p>
                  </dd>
                </div>
              );
            })}
          </dl>
        </LessonNote>
      );
    case "grammar":
      return (
        <>
          {block.entryIds.map((id) => {
            const grammar = lesson.knowledge.grammar.find((g) => g.id === id)!;
            return (
              <LessonNote key={id} kind="grammar" title={grammar.titleZh}>
                <p>{grammar.bodyZh}</p>
                {grammar.examples.map((example, i) => (
                  <div className="grammar-example" key={i}>
                    <button
                      type="button"
                      lang={lessonLanguage(lesson)}
                      onClick={() =>
                        learning.play([
                          knowledgeUnit(
                            id + ":" + i,
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
              </LessonNote>
            );
          })}
        </>
      );
    case "habit":
      return (
        <LessonNote kind="habit" title="带进日常">
          <p>{block.taskZh}</p>
          <p className="profile-note">{block.alternativeZh}</p>
        </LessonNote>
      );
    case "summary":
      return (
        <div className="lesson-note">
          <h2>今天能做到</h2>
          <ul>
            {block.takeawaysZh.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
      );
    default: {
      const unsupported: never = block;
      throw Error("Unsupported teaching block: " + JSON.stringify(unsupported));
    }
  }
}
