import { en } from "./en";
import { zhCN } from "./zhCN";
import type { TranslationResources } from "./translationKeys";

export type LanguageCode = "zh-CN" | "en";

export const SUPPORTED_LANGUAGES: LanguageCode[] = ["zh-CN", "en"];

export const resources: Record<LanguageCode, TranslationResources> = {
  "zh-CN": zhCN,
  en,
};
