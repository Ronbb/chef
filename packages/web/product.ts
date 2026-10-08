export interface Product {
  id: string;
  /** Trusted namespace for cookies, private drafts and identity notices.
   * Defaults to Brioche for compatibility; Hargow must explicitly use hargow. */
  sessionNamespace?: "brioche" | "hargow";
  name: string;
  wordmark: string;
  tagline: string;
  greeting: string;
  heroLines: [string, string];
  defaultUnit: string;
  targetLanguage: string;
  explanationLanguage: string;
  themeColor: string;
  brandIcon: string;
  avatar: string;
  /** Product copy, not an assessed level of the current learner. */
  learnerLabel?: string;
  courseLevelLabel?: string;
  theme: Record<`--${string}`, string>;
}
