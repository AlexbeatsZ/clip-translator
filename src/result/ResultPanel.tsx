import {
  AlertCircle,
  Check,
  ChevronDown,
  ClipboardList,
  Copy,
  Download,
  FileText,
  Image as ImageIcon,
  Pin,
  RotateCw,
  Settings,
  TextCursorInput,
  X,
} from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Segmented } from "../components/controls";
import { api, errorMessage, events } from "../lib/ipc";
import { AUTO_DETECT, LANGUAGES, languageLabel, providerMeta } from "../lib/languages";
import type { ProgressStage, TranslationResult } from "../lib/types";
import "../styles/panel.css";

const TEXT_WIDTH = 420;
const MAX_CONTENT_HEIGHT = 640;

type View =
  | { state: "idle" }
  | { state: "loading"; requestId: number; stage: ProgressStage; image: boolean }
  | { state: "done"; result: TranslationResult };

export function ResultPanel() {
  const [view, setView] = useState<View>({ state: "idle" });
  const [pinned, setPinned] = useState(false);
  const latestRequest = useRef(0);

  useEffect(() => {
    let disposed = false;
    const unlistenProgress = events.progress(({ requestId, stage }) => {
      if (requestId < latestRequest.current) return;
      latestRequest.current = requestId;
      setView((current) => ({
        state: "loading",
        requestId,
        stage,
        image: stage === "ocr" || stage === "compose" || (current.state === "loading" && current.requestId === requestId && current.image),
      }));
    });
    const unlistenResult = events.result((result) => {
      if (result.requestId < latestRequest.current) return;
      latestRequest.current = result.requestId;
      setView({ state: "done", result });
    });
    // The panel web view may load after the first result was produced.
    void api.pendingResult().then((result) => {
      if (!disposed && result && result.requestId >= latestRequest.current) {
        latestRequest.current = result.requestId;
        setView({ state: "done", result });
      }
    });
    return () => {
      disposed = true;
      void unlistenProgress.then((stop) => stop());
      void unlistenResult.then((stop) => stop());
    };
  }, []);

  const close = useCallback(() => {
    setPinned(false);
    void api.dismissResult().catch(() => undefined);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [close]);

  const togglePin = () => {
    const next = !pinned;
    setPinned(next);
    void api.setPanelPinned(next).catch(() => undefined);
  };

  const result = view.state === "done" ? view.result : null;
  const width = result?.kind === "image" ? imagePanelWidth(result) : TEXT_WIDTH;
  const frameRef = useAutoFit(width, view);

  return (
    <div className="panel" ref={frameRef}>
      <header className="panel-bar" data-tauri-drag-region>
        <SourceChip view={view} />
        <span className="bar-spacer" data-tauri-drag-region />
        <button type="button" className="icon-btn" aria-pressed={pinned} title={pinned ? "取消固定" : "固定浮窗（点击外部不关闭）"} aria-label="固定浮窗" onClick={togglePin}>
          <Pin size={14} />
        </button>
        <button type="button" className="icon-btn" title="关闭 (Esc)" aria-label="关闭" onClick={close}>
          <X size={15} />
        </button>
      </header>
      <div className="panel-scroll">
        <div className="panel-content" data-measure>
          {view.state === "done" ? (
            <ResultView key={view.result.requestId} result={view.result} />
          ) : (
            <LoadingView stage={view.state === "loading" ? view.stage : "capture"} image={view.state === "loading" && view.image} />
          )}
        </div>
      </div>
    </div>
  );
}

/** Reports the natural content size to the backend so the native panel hugs its content. */
function useAutoFit(width: number, view: View) {
  const frameRef = useRef<HTMLDivElement>(null);
  const lastSize = useRef("");

  const fit = useCallback(() => {
    const frame = frameRef.current;
    if (!frame) return;
    const bar = frame.querySelector<HTMLElement>(".panel-bar");
    const content = frame.querySelector<HTMLElement>("[data-measure]");
    if (!bar || !content) return;
    const height = Math.ceil(bar.offsetHeight + Math.min(content.scrollHeight, MAX_CONTENT_HEIGHT));
    const key = `${width}x${height}`;
    if (key === lastSize.current) return;
    lastSize.current = key;
    void api.fitResultPanel(width, height).catch(() => undefined);
  }, [width]);

  useLayoutEffect(() => {
    fit();
  }, [fit, view]);

  useEffect(() => {
    const content = frameRef.current?.querySelector<HTMLElement>("[data-measure]");
    if (!content) return;
    const observer = new ResizeObserver(() => fit());
    observer.observe(content);
    return () => observer.disconnect();
  }, [fit]);

  return frameRef;
}

function imagePanelWidth(result: TranslationResult) {
  const scale = window.devicePixelRatio || 1;
  const natural = (result.width ?? 800) / scale + 24;
  return Math.round(Math.min(720, Math.max(400, natural)));
}

function SourceChip({ view }: { view: View }) {
  let icon: ReactNode = <TextCursorInput size={13} />;
  let label = "正在读取";
  if (view.state === "done") {
    const { result } = view;
    if (result.kind === "image") {
      icon = <ImageIcon size={13} />;
      label = "剪贴板图片";
    } else if (result.kind === "error") {
      icon = <AlertCircle size={13} />;
      label = "ClipLingo";
    } else if (result.source === "selection") {
      label = "选中文字";
    } else {
      icon = <ClipboardList size={13} />;
      label = "剪贴板文字";
    }
  } else if (view.state === "loading" && view.image) {
    icon = <ImageIcon size={13} />;
    label = "剪贴板图片";
  }
  return (
    <span className="source-chip" data-tauri-drag-region>
      {icon}
      <span data-tauri-drag-region>{label}</span>
    </span>
  );
}

const STAGE_LABEL: Record<ProgressStage, string> = {
  capture: "正在读取选区或剪贴板…",
  ocr: "正在本地识别图片中的文字…",
  translate: "正在翻译…",
  compose: "正在把译文绘制回图片…",
};

function LoadingView({ stage, image: imageFlow }: { stage: ProgressStage; image: boolean }) {
  return (
    <div className="loading" role="status" aria-live="polite">
      <div className="loading-label">
        <span className="spinner" />
        <span>{STAGE_LABEL[stage]}</span>
      </div>
      {imageFlow && (
        <ol className="stage-track" aria-hidden="true">
          {(["ocr", "translate", "compose"] as const).map((item, index) => {
            const current = ["ocr", "translate", "compose"].indexOf(stage);
            return <li key={item} className={index < current ? "done" : index === current ? "active" : ""} />;
          })}
        </ol>
      )}
      <div className="skeleton">
        <span style={{ width: "92%" }} />
        <span style={{ width: "78%" }} />
        <span style={{ width: "54%" }} />
      </div>
    </div>
  );
}

function ResultView({ result }: { result: TranslationResult }) {
  if (result.kind === "error") return <ErrorView result={result} />;
  if (result.kind === "image") return <ImageResult result={result} />;
  return <TextResult result={result} />;
}

function ErrorView({ result }: { result: TranslationResult }) {
  const noInput = result.errorCode === "no-input";
  return (
    <div className="empty-state">
      <span className="empty-icon">{noInput ? <ClipboardList size={18} /> : <AlertCircle size={18} />}</span>
      <div className="empty-title">{noInput ? "没有可翻译的内容" : "翻译失败"}</div>
      <p className="empty-desc">{noInput ? "先选中一段文字，或者复制文字、截图到剪贴板，再按快捷键。" : result.error}</p>
      {!noInput && <ErrorActions result={result} />}
    </div>
  );
}

function ErrorActions({ result }: { result: TranslationResult }) {
  if (result.errorCode === "missing-key") {
    return (
      <button type="button" className="btn btn-primary btn-sm" onClick={() => void api.openSettings()}>
        <Settings size={12} /> 配置 {providerMeta(result.provider).shortName} 密钥
      </button>
    );
  }
  return (
    <button type="button" className="btn btn-sm" onClick={() => void api.triggerTranslation()}>
      <RotateCw size={12} /> 重试
    </button>
  );
}

function useCopy() {
  const [copied, setCopied] = useState<string | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(null), 1400);
    return () => window.clearTimeout(timer);
  }, [copied]);
  const run = async (id: string, action: () => Promise<unknown>) => {
    try {
      await action();
      setError("");
      setCopied(id);
    } catch (err) {
      setError(errorMessage(err));
    }
  };
  return { copied, error, run };
}

function TextResult({ result }: { result: TranslationResult }) {
  const [target, setTarget] = useState(result.targetLanguage ?? "zh");
  const [translated, setTranslated] = useState(result.translatedText ?? "");
  const [error, setError] = useState(result.error ?? "");
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const { copied, error: copyError, run } = useCopy();
  const sourceText = result.sourceText ?? "";
  const longSource = sourceText.length > 220 || sourceText.split("\n").length > 4;

  const retranslate = async (language: string) => {
    setTarget(language);
    setBusy(true);
    setError("");
    try {
      setTranslated(await api.translateText(sourceText, language, result.sourceLanguage ?? undefined));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="text-result">
      <div className={`source-text selectable ${longSource && !expanded ? "clamped" : ""}`}>{sourceText}</div>
      {longSource && (
        <button type="button" className="link-btn" onClick={() => setExpanded((value) => !value)}>
          {expanded ? "收起原文" : "展开原文"}
        </button>
      )}

      <div className="translation-block">
        <div className="lang-row">
          <span className="lang-from">{languageLabel(result.sourceLanguage ?? AUTO_DETECT.code)}</span>
          <span className="lang-arrow">→</span>
          <span className="lang-select">
            <select aria-label="目标语言" value={target} disabled={busy} onChange={(event) => void retranslate(event.target.value)}>
              {LANGUAGES.map((language) => (
                <option key={language.code} value={language.code}>
                  {language.label}
                </option>
              ))}
            </select>
            <ChevronDown size={11} aria-hidden="true" />
          </span>
          {busy && <span className="spinner small" />}
        </div>
        {error ? (
          <div className="inline-error">
            <p>{error}</p>
            <ErrorActions result={{ ...result, errorCode: result.errorCode ?? "provider" }} />
          </div>
        ) : (
          <div className={`translated-text selectable ${busy ? "stale" : ""}`}>{translated}</div>
        )}
      </div>

      <footer className="result-foot">
        <span className="provider-tag">{providerMeta(result.provider).shortName}</span>
        {copyError && <span className="foot-error">{copyError}</span>}
        <span className="bar-spacer" />
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => void run("source", () => api.copyText(sourceText))}>
          {copied === "source" ? <Check size={12} /> : <Copy size={12} />}
          {copied === "source" ? "已复制" : "原文"}
        </button>
        {!error && (
          <button type="button" className="btn btn-primary btn-sm" disabled={!translated || busy} onClick={() => void run("translation", () => api.copyText(translated))}>
            {copied === "translation" ? <Check size={12} /> : <Copy size={12} />}
            {copied === "translation" ? "已复制" : "复制译文"}
          </button>
        )}
      </footer>
    </div>
  );
}

type ImageTab = "translated" | "original" | "text";

function ImageResult({ result }: { result: TranslationResult }) {
  const hasTranslatedImage = !!result.translatedImageDataUrl;
  const hasOriginal = !!result.imageDataUrl;
  // Without a translation (e.g. missing key) still let the user read and copy the OCR text.
  const segments = result.segments.length > 0 ? result.segments : (result.sourceText ? result.sourceText.split("\n\n").map((source) => ({ source, translated: "" })) : []);
  const hasText = segments.length > 0;
  const [tab, setTab] = useState<ImageTab>(hasTranslatedImage ? "translated" : hasText ? "text" : "original");
  const [savedPath, setSavedPath] = useState("");
  const { copied, error: actionError, run } = useCopy();

  const imageUrl = tab === "original" ? result.imageDataUrl : result.translatedImageDataUrl;
  const tabs: { value: ImageTab; label: string; disabled?: boolean }[] = [];
  if (hasTranslatedImage) tabs.push({ value: "translated", label: "译图" });
  if (hasOriginal) tabs.push({ value: "original", label: "原图" });
  if (hasText && (hasOriginal || hasTranslatedImage)) tabs.push({ value: "text", label: "文字" });

  const save = async () => {
    if (!result.translatedImageDataUrl) return;
    try {
      setSavedPath(await api.saveImage(result.translatedImageDataUrl));
    } catch (err) {
      void run("save-error", () => Promise.reject(err));
    }
  };

  const allTranslated = result.segments.map((segment) => segment.translated).join("\n\n");
  const copyTextValue = allTranslated || result.sourceText || "";

  return (
    <div className="image-result">
      {result.notice && <div className="notice">{result.notice}</div>}
      {result.error && (
        <div className="inline-error boxed">
          <p>{result.error}</p>
          <ErrorActions result={result} />
        </div>
      )}

      {tab === "text" ? (
        <ol className="segments selectable">
          {segments.map((segment, index) => (
            <li key={index}>
              <p className={segment.translated ? "segment-source" : "segment-translated"}>{segment.source}</p>
              {segment.translated && <p className="segment-translated">{segment.translated}</p>}
            </li>
          ))}
        </ol>
      ) : (
        imageUrl && (
          <div className="image-frame">
            <img src={imageUrl} alt={tab === "original" ? "原图" : "翻译后的图片"} draggable={false} />
          </div>
        )
      )}

      <footer className="result-foot">
        {tabs.length > 1 ? <Segmented label="显示内容" value={tab} options={tabs} onChange={setTab} /> : <span className="provider-tag">本地 OCR</span>}
        {(actionError || savedPath) && (
          <span className={actionError ? "foot-error" : "foot-note"}>
            {actionError ||
              (
                <>
                  已保存到下载
                  <button type="button" className="link-btn" onClick={() => void api.revealPath(savedPath)}>
                    显示
                  </button>
                </>
              )}
          </span>
        )}
        <span className="bar-spacer" />
        {tab === "text" || !hasTranslatedImage ? (
          <button type="button" className="btn btn-primary btn-sm" disabled={!copyTextValue} onClick={() => void run("text", () => api.copyText(copyTextValue))}>
            {copied === "text" ? <Check size={12} /> : <FileText size={12} />}
            {copied === "text" ? "已复制" : allTranslated ? "复制译文" : "复制识别文字"}
          </button>
        ) : (
          <>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => void save()} title="保存译图到“下载”文件夹">
              <Download size={12} /> 保存
            </button>
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={() => void run("image", () => api.copyImage((tab === "original" ? result.imageDataUrl : result.translatedImageDataUrl) ?? ""))}
            >
              {copied === "image" ? <Check size={12} /> : <Copy size={12} />}
              {copied === "image" ? "已复制" : tab === "original" ? "复制原图" : "复制译图"}
            </button>
          </>
        )}
      </footer>
    </div>
  );
}
