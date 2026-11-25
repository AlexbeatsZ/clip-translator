import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import type {
  AppInfo,
  Preferences,
  Progress,
  ProviderId,
  ProviderStatus,
  SelectionPermission,
  TranslationResult,
} from "./types";

/** False when the UI is opened in a plain browser (vite dev preview with mock data). */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Mock = typeof import("./mock");
let mockModule: Promise<Mock> | null = null;
const mock = () =>
  (mockModule ??= import.meta.env.DEV ? import("./mock") : Promise.reject(new Error("ClipLingo 需要在桌面应用中运行")));

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return tauriInvoke<T>(command, args);
  return (await mock()).mockInvoke<T>(command, args);
}

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (isTauri) return tauriListen<T>(event, (message) => handler(message.payload));
  return (await mock()).mockListen<T>(event, handler);
}

/** Tauri command errors arrive as plain strings. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

export const api = {
  appInfo: () => call<AppInfo>("get_app_info"),
  preferences: () => call<Preferences>("get_preferences"),
  savePreferences: (preferences: Preferences) => call<void>("save_preferences", { preferences }),
  providerStatus: () => call<ProviderStatus>("get_provider_status"),
  saveApiKey: (provider: ProviderId, apiKey: string) => call<void>("save_api_key", { provider, apiKey }),
  deleteApiKey: (provider: ProviderId) => call<void>("delete_api_key", { provider }),
  testProvider: (provider: ProviderId) => call<string>("test_provider", { provider }),
  selectionPermission: () => call<SelectionPermission>("get_selection_permission"),
  openAccessibilitySettings: () => call<void>("open_accessibility_settings"),
  hotkey: () => call<string>("get_hotkey"),
  setHotkey: (shortcut: string) => call<void>("set_hotkey", { shortcut }),
  suspendHotkey: (suspended: boolean) => call<void>("suspend_hotkey", { suspended }),
  triggerTranslation: () => call<void>("trigger_translation"),
  openSettings: () => call<void>("open_settings"),

  pendingResult: () => call<TranslationResult | null>("get_pending_result"),
  dismissResult: () => call<void>("dismiss_result"),
  setPanelPinned: (pinned: boolean) => call<void>("set_panel_pinned", { pinned }),
  fitResultPanel: (width: number, height: number) => call<void>("fit_result_panel", { width, height }),
  copyText: (text: string) => call<void>("copy_text", { text }),
  copyImage: (imageDataUrl: string) => call<void>("copy_image", { imageDataUrl }),
  saveImage: (imageDataUrl: string) => call<string>("save_image", { imageDataUrl }),
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  translateText: (text: string, targetLanguage: string, sourceLanguage?: string) =>
    call<string>("translate_text", { text, targetLanguage, sourceLanguage }),

  openUrl: async (url: string) => {
    if (isTauri) return tauriOpenUrl(url);
    window.open(url, "_blank", "noopener");
  },
};

export const events = {
  progress: (handler: (payload: Progress) => void) => listen<Progress>("translation-progress", handler),
  result: (handler: (payload: TranslationResult) => void) => listen<TranslationResult>("translation-result", handler),
  settingsShown: (handler: () => void) => listen<null>("settings-shown", handler),
};
