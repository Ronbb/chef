import type { ReadingText } from "@brioche/contracts/ReadingText";
import { readingTokens } from "../lib/reading-model";

export function ReadingTextLabel({
  reading,
}: {
  reading: string | ReadingText;
}) {
  if (typeof reading === "string") return <>{reading}</>;
  const tokens = readingTokens({
    id: "label",
    reading,
    vocabularyId: null,
    grammarId: null,
  });
  return (
    <>
      {tokens.map((token) => (
        <span key={token.start}>
          {token.pronunciation ? (
            <ruby>
              {token.text}
              <rp>（</rp>
              <rt>{token.pronunciation}</rt>
              <rp>）</rp>
            </ruby>
          ) : (
            token.text
          )}
        </span>
      ))}
    </>
  );
}
