import type { ProviderId } from "./types";

export interface Language {
  code: string;
  label: string;
}

/** Languages supported by both DeepL and Google Cloud Translation. */
export const LANGUAGES: Language[] = [
  { code: "zh", label: "简体中文" },
  { code: "zh-TW", label: "繁體中文" },
  { code: "en", label: "英语" },
  { code: "ja", label: "日语" },
  { code: "ko", label: "韩语" },
  { code: "fr", label: "法语" },
  { code: "de", label: "德语" },
  { code: "es", label: "西班牙语" },
  { code: "it", label: "意大利语" },
  { code: "pt", label: "葡萄牙语" },
  { code: "ru", label: "俄语" },
  { code: "ar", label: "阿拉伯语" },
  { code: "nl", label: "荷兰语" },
  { code: "pl", label: "波兰语" },
  { code: "sv", label: "瑞典语" },
  { code: "tr", label: "土耳其语" },
  { code: "uk", label: "乌克兰语" },
];

export const AUTO_DETECT: Language = { code: "auto", label: "自动检测" };

export function languageLabel(code?: string | null): string {
  if (!code || code === "auto") return AUTO_DETECT.label;
  return LANGUAGES.find((language) => language.code.toLowerCase() === code.toLowerCase())?.label ?? code;
}

export interface ProviderMeta {
  id: ProviderId;
  name: string;
  shortName: string;
  description: string;
  keyLabel: string;
  keyPlaceholder: string;
  keyHelp: string;
  keyUrl: string;
}

export const PROVIDERS: ProviderMeta[] = [
  {
    id: "deepl",
    name: "DeepL API",
    shortName: "DeepL",
    description: "译文自然流畅，适合日常阅读",
    keyLabel: "Authentication Key",
    keyPlaceholder: "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx:fx",
    keyHelp: "Free 与 Pro 密钥均可使用，会按密钥自动选择对应端点。",
    keyUrl: "https://www.deepl.com/your-account/keys",
  },
  {
    id: "google",
    name: "Google Cloud Translation",
    shortName: "Google",
    description: "语种覆盖广，需要启用 API 并开通计费",
    keyLabel: "API Key",
    keyPlaceholder: "AIza…",
    keyHelp: "需在 Google Cloud 项目中启用 Cloud Translation API。",
    keyUrl: "https://console.cloud.google.com/apis/credentials",
  },
];

export function providerMeta(id?: string | null): ProviderMeta {
  return PROVIDERS.find((provider) => provider.id === id) ?? PROVIDERS[0];
}
