import type { PublicLesson } from "@brioche/contracts/PublicLesson";
import type { NeutralLesson } from "@brioche/contracts/NeutralLesson";
import type { Segment } from "@brioche/contracts/Segment";
import type { NeutralSegment } from "@brioche/contracts/NeutralSegment";
import type { Vocabulary } from "@brioche/contracts/Vocabulary";
import type { NeutralVocabulary } from "@brioche/contracts/NeutralVocabulary";
import type { Grammar } from "@brioche/contracts/Grammar";
import type { NeutralGrammar } from "@brioche/contracts/NeutralGrammar";
import type { ReadingText } from "@brioche/contracts/ReadingText";
import type { LearningState } from "@brioche/contracts/LearningState";

export type ReadingLesson = PublicLesson | NeutralLesson;
export type ReadingSession = { lesson: ReadingLesson; progress: LearningState };
export type ReadingSegment = Segment | NeutralSegment;
export type ReadingVocabulary = Vocabulary | NeutralVocabulary;
export type ReadingGrammar = Grammar | NeutralGrammar;
export const lessonLanguage = (lesson: ReadingLesson) =>
  "targetLanguage" in lesson ? lesson.targetLanguage : "fr-FR";
export const targetText = (value: string | ReadingText) =>
  typeof value === "string" ? value : value.text;
export const segmentText = (segment: ReadingSegment) =>
  "reading" in segment ? segment.reading.text : segment.text;
export const exampleText = (
  example: Grammar["examples"][number] | NeutralGrammar["examples"][number],
) => ("target" in example ? example.target : example.fr);

export type ReadingToken = {
  text: string;
  start: number;
  end: number;
  word: boolean;
  pronunciation?: string;
};
export function readingTokens(segment: ReadingSegment): ReadingToken[] {
  if (!("reading" in segment)) {
    return Array.from(
      new Intl.Segmenter("fr", { granularity: "word" }).segment(segment.text),
      (token) => {
        const start = Array.from(segment.text.slice(0, token.index)).length;
        return {
          text: token.segment,
          start,
          end: start + Array.from(token.segment).length,
          word: !!token.isWordLike,
        };
      },
    );
  }
  const { text, words, pronunciations = [] } = segment.reading;
  const scalars = Array.from(text);
  const tokens: ReadingToken[] = [];
  let offset = 0;
  for (const range of words) {
    if (
      !Number.isInteger(range.start) ||
      !Number.isInteger(range.end) ||
      range.start < offset ||
      range.start >= range.end ||
      range.end > scalars.length
    )
      throw new Error("Invalid authored reading range");
    if (range.start > offset)
      tokens.push({
        text: scalars.slice(offset, range.start).join(""),
        start: offset,
        end: range.start,
        word: false,
      });
    const pronunciation = pronunciations.find(
      (p) => p.range.start === range.start && p.range.end === range.end,
    );
    tokens.push({
      text: scalars.slice(range.start, range.end).join(""),
      ...range,
      word: true,
      ...(pronunciation ? { pronunciation: pronunciation.text } : {}),
    });
    offset = range.end;
  }
  if (offset < scalars.length)
    tokens.push({
      text: scalars.slice(offset).join(""),
      start: offset,
      end: scalars.length,
      word: false,
    });
  return tokens;
}
