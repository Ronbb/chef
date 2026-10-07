// Compatibility fixture, not a published language product.
import type { Product } from "@chef/web/product";
export default {
  id: "chef-fixture",
  name: "Chef Fixture",
  wordmark: "chef.",
  tagline: "一点法语，一点生活",
  greeting: "Bonjour，今天从一件小事开始。",
  heroLines: ["把法语，", "放进每一天。"],
  defaultUnit: "日常法语",
  targetLanguage: "fr-FR",
  explanationLanguage: "zh-CN",
  themeColor: "#fffaef",
  brandIcon: "/icons/fixture-mark.svg",
  avatar: "/assets/avatars/learner.svg",
  theme: {
    "--paper": "#faf7ef",
    "--surface": "#fffdf7",
    "--ink": "#38291f",
    "--muted": "#695842",
    "--primary": "#ffa62f",
    "--accent": "#9c4e27",
    "--soft": "#fff0da",
    "--leaf": "#acd793",
    "--green": "#3b572b",
    "--line": "#e9d9bb",
    "--butter": "#ffc96f",
    "--pistachio": "#3b572b",
    "--sans":
      '-apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
    "--serif": 'ui-serif, "New York", Georgia, "Times New Roman", serif',
  },
} satisfies Product;
