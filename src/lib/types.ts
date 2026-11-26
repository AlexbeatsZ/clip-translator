export type ProviderId = "deepl" | "google" | "yandex";

export type Platform = "macos" | "windows" | "linux";

export interface Preferences {
  provider: ProviderId;
  sourceLanguage: string;
  targetLanguage: string;
  imageReplacement: boolean;
  keepOriginal: boolean;
}

export type ProviderStatus = Record<ProviderId, boolean>;

export interface AppInfo {
  version: string;
  platform: Platform;
}

export interface SelectionPermission {
  status: "granted" | "denied" | "not-required" | "unsupported" | "unknown";
  guidance?: string;
}

export type ProgressStage = "capture" | "ocr" | "translate" | "compose";

export interface Progress {
  requestId: number;
  stage: ProgressStage;
}

export type ErrorCode = "no-input" | "missing-key" | "provider" | "ocr" | "compose" | "internal";

export interface Segment {
  source: string;
  translated: string;
}

export interface TranslationResult {
  requestId: number;
  kind: "text" | "image" | "error";
  source?: "selection" | "clipboard" | null;
  provider?: ProviderId | null;
  sourceLanguage?: string | null;
  targetLanguage?: string | null;
  sourceText?: string | null;
  translatedText?: string | null;
  imageDataUrl?: string | null;
  translatedImageDataUrl?: string | null;
  segments: Segment[];
  width?: number | null;
  height?: number | null;
  notice?: string | null;
  error?: string | null;
  errorCode?: ErrorCode | null;
}
