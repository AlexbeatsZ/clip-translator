import { RotateCcw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Keycaps } from "../components/controls";
import { api, errorMessage } from "../lib/ipc";
import { recordShortcut, shortcutKeys } from "../lib/shortcut";
import type { Platform } from "../lib/types";

const DEFAULT_HOTKEY = "Alt+Shift+KeyT";

export function ShortcutRecorder({
  value,
  platform,
  onChange,
}: {
  value: string;
  platform: Platform;
  onChange: (value: string) => void;
}) {
  const [recording, setRecording] = useState(false);
  const [pendingKeys, setPendingKeys] = useState<string[]>([]);
  const [message, setMessage] = useState<{ tone: "error" | "ok"; text: string } | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const apply = async (accelerator: string) => {
    try {
      await api.setHotkey(accelerator);
      onChange(accelerator);
      setMessage({ tone: "ok", text: "已更新" });
    } catch (error) {
      await api.suspendHotkey(false).catch(() => undefined);
      setMessage({ tone: "error", text: errorMessage(error) });
    }
  };

  useEffect(() => {
    if (!recording) return;
    void api.suspendHotkey(true).catch(() => undefined);
    let finished = false;

    const stop = (resume: boolean) => {
      finished = true;
      setRecording(false);
      setPendingKeys([]);
      if (resume) void api.suspendHotkey(false).catch(() => undefined);
    };

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.code === "Escape" && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) {
        stop(true);
        return;
      }
      const recorded = recordShortcut(event, platform);
      if (recorded.kind === "pending") {
        setPendingKeys(recorded.keys);
      } else if (recorded.kind === "invalid") {
        setMessage({ tone: "error", text: recorded.message });
      } else {
        stop(false);
        void apply(recorded.accelerator);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (!event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) setPendingKeys([]);
    };
    const onBlur = () => stop(true);

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", onBlur);
      if (!finished) void api.suspendHotkey(false).catch(() => undefined);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording, platform]);

  useEffect(() => {
    if (message?.tone !== "ok") return;
    const timer = window.setTimeout(() => setMessage(null), 1600);
    return () => window.clearTimeout(timer);
  }, [message]);

  const keys = recording ? pendingKeys : shortcutKeys(value || DEFAULT_HOTKEY, platform);

  return (
    <div className="shortcut-recorder">
      {message && <span className={`inline-status ${message.tone}`}>{message.text}</span>}
      <button
        ref={buttonRef}
        type="button"
        className={`shortcut-field ${recording ? "recording" : ""}`}
        onClick={() => {
          setMessage(null);
          setRecording((current) => !current);
        }}
        aria-label={recording ? "正在录制快捷键，按 Esc 取消" : "点击录制新的快捷键"}
      >
        {recording && keys.length === 0 ? <span className="shortcut-hint">按下新快捷键…</span> : <Keycaps keys={keys} />}
      </button>
      {value && shortcutKeys(value, platform).join() !== shortcutKeys(DEFAULT_HOTKEY, platform).join() && !recording && (
        <button type="button" className="icon-btn" title="恢复默认" aria-label="恢复默认快捷键" onClick={() => void apply(DEFAULT_HOTKEY)}>
          <RotateCcw size={14} />
        </button>
      )}
    </div>
  );
}
