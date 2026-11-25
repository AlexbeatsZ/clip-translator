import { ArrowLeftRight, Check, ChevronRight, Image as ImageIcon, Info, Languages, Play, SlidersHorizontal } from "lucide-react";
import { useState, type ReactNode } from "react";
import appIcon from "../../src-tauri/icons/128x128@2x.png";
import { Keycaps, Select, Toggle } from "../components/controls";
import { api } from "../lib/ipc";
import { AUTO_DETECT, LANGUAGES, providerMeta } from "../lib/languages";
import { shortcutKeys } from "../lib/shortcut";
import { ProvidersPage } from "./ProvidersPage";
import { ShortcutRecorder } from "./ShortcutRecorder";
import { Group, PageHeader, Row } from "./layout";
import { useSettings, type SettingsState } from "./useSettings";
import "../styles/settings.css";

type Page = "general" | "providers" | "image" | "about";

const NAV: { id: Page; label: string; icon: ReactNode }[] = [
  { id: "general", label: "通用", icon: <SlidersHorizontal size={15} /> },
  { id: "providers", label: "翻译服务", icon: <Languages size={15} /> },
  { id: "image", label: "图片翻译", icon: <ImageIcon size={15} /> },
  { id: "about", label: "关于", icon: <Info size={15} /> },
];

export function SettingsApp() {
  const state = useSettings();
  const [page, setPage] = useState<Page>("general");
  const providerReady = state.providerStatus[state.preferences.provider];

  return (
    <div className={`settings platform-${state.info.platform}`}>
      <aside className="sidebar">
        <div className="sidebar-drag" data-tauri-drag-region />
        <nav aria-label="设置分类">
          {NAV.map((item) => (
            <button
              key={item.id}
              type="button"
              className="nav-item"
              aria-current={page === item.id ? "page" : undefined}
              onClick={() => setPage(item.id)}
            >
              {item.icon}
              <span>{item.label}</span>
              {item.id === "providers" && state.loaded && !providerReady && <span className="nav-badge" aria-label="需要配置" />}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          <Keycaps keys={shortcutKeys(state.hotkey || "Alt+Shift+KeyT", state.info.platform)} />
          <span>翻译选区或剪贴板</span>
        </div>
      </aside>
      <main className="content">
        <div className="content-inner" style={{ visibility: state.loaded ? "visible" : "hidden" }}>
          {page === "general" && <GeneralPage state={state} goTo={setPage} />}
          {page === "providers" && <ProvidersPage state={state} />}
          {page === "image" && <ImagePage state={state} />}
          {page === "about" && <AboutPage state={state} />}
          {state.saveError && <div className="inline-status error">{state.saveError}</div>}
        </div>
      </main>
    </div>
  );
}

function GeneralPage({ state, goTo }: { state: SettingsState; goTo: (page: Page) => void }) {
  const { preferences, updatePreferences, info, permission } = state;
  const providerReady = state.providerStatus[preferences.provider];
  const needsPermission = info.platform === "macos" && permission.status === "denied";
  const ready = providerReady && !needsPermission;
  const languageOptions = LANGUAGES.map((language) => ({ value: language.code, label: language.label }));
  const canSwap = preferences.sourceLanguage !== "auto" && preferences.sourceLanguage !== preferences.targetLanguage;

  return (
    <>
      <PageHeader title="通用" />

      <section className={`status-card ${ready ? "ready" : "todo"}`}>
        {ready ? (
          <div className="status-ready">
            <span className="status-icon ok">
              <Check size={16} strokeWidth={2.6} />
            </span>
            <span className="row-text">
              <span className="status-title">已就绪</span>
              <span className="row-desc">
                选中文字或复制截图后，按 <Keycaps keys={shortcutKeys(state.hotkey, info.platform)} /> 翻译。
              </span>
            </span>
            <button type="button" className="btn" onClick={() => void api.triggerTranslation()}>
              <Play size={12} /> 翻译剪贴板
            </button>
          </div>
        ) : (
          <>
            <div className="status-head">
              <span className="status-title">开始使用前还需要完成</span>
            </div>
            {!providerReady && (
              <TodoRow
                title={`配置 ${providerMeta(preferences.provider).shortName} 密钥`}
                description="翻译文字需要你自己的 API Key"
                action="去配置"
                onClick={() => goTo("providers")}
              />
            )}
            {needsPermission && (
              <TodoRow
                title="允许辅助功能权限"
                description="用于直接读取其他应用中选中的文字。未授权时只能翻译剪贴板。"
                action="打开系统设置"
                onClick={() => void api.openAccessibilitySettings()}
              />
            )}
          </>
        )}
      </section>

      <Group title="快捷键">
        <Row title="翻译" description="有选中文字时翻译选区；否则翻译剪贴板中的图片或文字。">
          <ShortcutRecorder value={state.hotkey} platform={info.platform} onChange={state.setHotkey} />
        </Row>
      </Group>

      <Group title="语言">
        <Row title="源语言">
          <Select
            label="源语言"
            value={preferences.sourceLanguage}
            options={[{ value: AUTO_DETECT.code, label: AUTO_DETECT.label }, ...languageOptions]}
            onChange={(sourceLanguage) => updatePreferences({ sourceLanguage })}
          />
        </Row>
        <Row title="目标语言">
          <button
            type="button"
            className="icon-btn"
            title="互换源语言和目标语言"
            aria-label="互换源语言和目标语言"
            disabled={!canSwap}
            onClick={() =>
              updatePreferences({ sourceLanguage: preferences.targetLanguage, targetLanguage: preferences.sourceLanguage })
            }
          >
            <ArrowLeftRight size={14} />
          </button>
          <Select
            label="目标语言"
            value={preferences.targetLanguage}
            options={languageOptions}
            onChange={(targetLanguage) => updatePreferences({ targetLanguage })}
          />
        </Row>
      </Group>

      {info.platform === "macos" && !needsPermission && (
        <Group title="权限">
          <Row
            title="辅助功能"
            description={permission.status === "granted" ? "已授权，可以直接读取选中的文字。" : "用于读取其他应用中选中的文字。"}
          >
            {permission.status === "granted" ? (
              <span className="pill ok">已允许</span>
            ) : (
              <button type="button" className="btn" onClick={() => void api.openAccessibilitySettings()}>
                打开系统设置
              </button>
            )}
          </Row>
        </Group>
      )}
    </>
  );
}

function TodoRow({ title, description, action, onClick }: { title: string; description: string; action: string; onClick: () => void }) {
  return (
    <div className="todo-row">
      <span className="status-icon warn" aria-hidden="true" />
      <span className="row-text">
        <span className="row-title">{title}</span>
        <span className="row-desc">{description}</span>
      </span>
      <button type="button" className="btn btn-primary" onClick={onClick}>
        {action} <ChevronRight size={13} />
      </button>
    </div>
  );
}

function ImagePage({ state }: { state: SettingsState }) {
  const { preferences, updatePreferences, info } = state;
  const screenshotKeys = info.platform === "macos" ? ["⌃", "⇧", "⌘", "4"] : ["Win", "Shift", "S"];
  return (
    <>
      <PageHeader title="图片翻译" description="剪贴板里是图片时，先在本机识别文字，再把译文排回原来的位置。" />

      <Group title="输出">
        <Row title="把译文绘制回图片" description="关闭后只显示识别出的文字和译文。">
          <Toggle label="把译文绘制回图片" checked={preferences.imageReplacement} onChange={(imageReplacement) => updatePreferences({ imageReplacement })} />
        </Row>
        <Row title="保留原图" description="在结果中可以切换查看原图。">
          <Toggle label="保留原图" checked={preferences.keepOriginal} onChange={(keepOriginal) => updatePreferences({ keepOriginal })} />
        </Row>
      </Group>

      <Group title="本地识别">
        <Row title="识别引擎" description="PaddleOCR PP-OCRv6 Small，支持中英文。">
          <span className="value-text">本机运行</span>
        </Row>
        <Row title="隐私" description="截图不会上传。只有识别出的文字会发送给翻译服务。" />
      </Group>

      <Group title="使用方法">
        <ol className="steps">
          <li>
            <span className="step-num">1</span>
            <span>
              截图到剪贴板：<Keycaps keys={screenshotKeys} />
            </span>
          </li>
          <li>
            <span className="step-num">2</span>
            <span>
              按 <Keycaps keys={shortcutKeys(state.hotkey, info.platform)} />，几秒后会弹出翻译好的图片
            </span>
          </li>
        </ol>
      </Group>
    </>
  );
}

function AboutPage({ state }: { state: SettingsState }) {
  return (
    <>
      <div className="about-hero" data-tauri-drag-region>
        <img src={appIcon} alt="" width={72} height={72} />
        <div>
          <h1>ClipLingo</h1>
          <p>版本 {state.info.version || "—"}</p>
        </div>
      </div>
      <Group title="工作方式">
        <Row title="一个快捷键" description="自动判断：选中的文字 → 剪贴板图片 → 剪贴板文字。" />
        <Row title="本地优先" description="OCR 和图片重绘都在本机完成，不需要账号，也没有自己的服务器。" />
        <Row title="不打扰" description="只在按下快捷键时读取剪贴板，结果浮窗不会抢走当前应用的焦点。" />
      </Group>
      <Group title="致谢">
        <Row title="OCR 模型" description="PP-OCRv6（PaddleOCR，Apache-2.0），经 RapidOCR 在本地推理。" />
      </Group>
    </>
  );
}
