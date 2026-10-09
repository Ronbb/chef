// Public v1 fixtures converted to the v2 shape used by the real preview API.
export function neutralFixture(lesson: any) {
  const reading = (text: string) => ({ text, words: [] });
  const convert = (value: any): any => {
    if (Array.isArray(value)) return value.map(convert);
    if (!value || typeof value !== "object") return value;
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => {
        if (key === "fr") return ["reading", reading(child as string)];
        if (key === "lemma") return [key, reading(child as string)];
        if (key === "templateFr") return ["templateTarget", child];
        return [key, convert(child)];
      }),
    );
  };
  const result = convert(lesson);
  result.schemaVersion = "2.0";
  result.targetLanguage = "fr-FR";
  result.explanationLanguage = "zh-CN";
  result.title = { target: lesson.title.fr, zh: lesson.title.zh };
  result.knowledge.grammar.forEach((grammar: any, i: number) => {
    grammar.examples.forEach((example: any, j: number) => {
      example.target = reading(lesson.knowledge.grammar[i].examples[j].fr);
      delete example.reading;
    });
  });
  return result;
}
