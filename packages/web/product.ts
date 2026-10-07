export interface Product {
  id: string;
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
  theme: Record<`--${string}`, string>;
}
