/**
 * Browser-only stand-in for the Tauri backend so the UI can be previewed with `pnpm dev`.
 * Never loaded inside the desktop app (see `isTauri` in ipc.ts).
 *
 * Result panel scenarios: `?view=result&mock=text|long|image|loading|ocr|missing-key|no-input`.
 */
import type { Preferences, Progress, TranslationResult } from "./types";

type Handler = (payload: unknown) => void;
const listeners = new Map<string, Set<Handler>>();
const params = new URLSearchParams(window.location.search);

let preferences: Preferences = {
  provider: "deepl",
  sourceLanguage: "auto",
  targetLanguage: "zh",
  imageReplacement: true,
  keepOriginal: true,
};
let providerStatus = { deepl: params.get("configured") !== "0", google: false, yandex: true };
let hotkey = "Alt+Shift+KeyT";

function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((handler) => handler(payload));
}

export function mockListen<T>(event: string, handler: (payload: T) => void) {
  const set = listeners.get(event) ?? new Set<Handler>();
  set.add(handler as Handler);
  listeners.set(event, set);
  return Promise.resolve(() => set.delete(handler as Handler));
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function sampleImage(translated: boolean): string {
  const title = translated ? "季度路线图" : "Quarterly Roadmap";
  const body = translated
    ? ["改进离线识别速度", "支持多显示器浮窗", "发布 Windows 预览版"]
    : ["Improve offline OCR speed", "Multi-monitor floating panel", "Ship Windows preview"];
  const rows = body
    .map((line, index) => `<text x="40" y="${132 + index * 34}" font-size="18" fill="#3b4250">• ${line}</text>`)
    .join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="560" height="260"><rect width="560" height="260" fill="#fbfaf7"/><rect x="0" y="0" width="560" height="6" fill="#e0b84f"/><text x="40" y="72" font-size="30" font-weight="700" fill="#1e2430" font-family="-apple-system,PingFang SC,sans-serif">${title}</text>${rows}</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function scenario(requestId: number): TranslationResult {
  const base = {
    requestId,
    provider: preferences.provider,
    sourceLanguage: preferences.sourceLanguage,
    targetLanguage: preferences.targetLanguage,
    segments: [],
  };
  switch (params.get("mock")) {
    case "long":
      return {
        ...base,
        kind: "text",
        source: "clipboard",
        sourceText:
          "Local-first software keeps the primary copy of your data on your own device. Network access becomes optional: you can keep working on a plane, and the app stays fast because it never waits on a round trip. Sync, when it exists, is a background convenience rather than a dependency.\n\nThis design also changes who owns the data. Instead of renting access from a server, users hold it.",
        translatedText:
          "本地优先软件把数据的主副本保存在你自己的设备上。网络访问变成了可选项：在飞机上也能继续工作，而且应用始终很快，因为它从不等待网络往返。即使有同步功能，它也只是后台的便利，而不是依赖。\n\n这种设计也改变了数据归属。用户不再向服务器租用访问权，而是自己持有数据。",
      };
    case "image":
    case "ocr":
      return {
        ...base,
        kind: "image",
        source: "clipboard",
        imageDataUrl: sampleImage(false),
        translatedImageDataUrl: params.get("mock") === "image" ? sampleImage(true) : null,
        width: 560,
        height: 260,
        sourceText: "Quarterly Roadmap\n\nImprove offline OCR speed",
        translatedText: "季度路线图\n\n改进离线识别速度",
        segments: [
          { source: "Quarterly Roadmap", translated: "季度路线图" },
          { source: "Improve offline OCR speed", translated: "改进离线识别速度" },
          { source: "Multi-monitor floating panel", translated: "支持多显示器浮窗" },
        ],
        notice: params.get("mock") === "ocr" ? "图片重绘失败，已改为显示文字译文：背景过于复杂" : null,
      };
    case "missing-key":
      return {
        ...base,
        kind: "text",
        source: "selection",
        sourceText: "The quick brown fox jumps over the lazy dog.",
        error: "尚未配置 DeepL API Key",
        errorCode: "missing-key",
      };
    case "no-input":
      return { ...base, kind: "error", error: "没有检测到文字选区，也没有可翻译的剪贴板文字或图片", errorCode: "no-input" };
    default:
      return {
        ...base,
        kind: "text",
        source: "selection",
        sourceText: "Shipping is a feature. A really important feature.",
        translatedText: "按时交付本身就是一项功能，而且是非常重要的功能。",
      };
  }
}

let requestSeq = 0;
async function runScenario() {
  const requestId = ++requestSeq;
  const image = ["image", "ocr"].includes(params.get("mock") ?? "");
  const stages: Progress["stage"][] = image ? ["capture", "ocr", "translate", "compose"] : ["capture", "translate"];
  for (const stage of stages) {
    emit("translation-progress", { requestId, stage });
    await sleep(params.get("mock") === "loading" ? 60_000 : 450);
  }
  emit("translation-result", scenario(requestId));
}

export async function mockInvoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  await sleep(80);
  const result = (() => {
    switch (command) {
      case "get_app_info":
        return { version: "0.2.0", platform: params.get("platform") ?? "macos" };
      case "get_preferences":
        return preferences;
      case "save_preferences":
        preferences = args.preferences as Preferences;
        return null;
      case "get_provider_status":
        return providerStatus;
      case "save_api_key":
        providerStatus = { ...providerStatus, [args.provider as string]: true };
        return null;
      case "delete_api_key":
        providerStatus = { ...providerStatus, [args.provider as string]: false };
        return null;
      case "test_provider":
        return sleep(600).then(() => "你好，世界！");
      case "get_selection_permission":
        return { status: params.get("ax") === "denied" ? "denied" : "granted" };
      case "get_hotkey":
        return hotkey;
      case "set_hotkey":
        hotkey = args.shortcut as string;
        return null;
      case "get_pending_result":
        void runScenario();
        return null;
      case "trigger_translation":
        void runScenario();
        return null;
      case "translate_text":
        return sleep(500).then(() => `【${args.targetLanguage}】${String(args.text).slice(0, 40)}…`);
      case "save_image":
        return "/Users/demo/Downloads/ClipLingo-1764000000.png";
      default:
        return null;
    }
  })();
  return (await result) as T;
}
