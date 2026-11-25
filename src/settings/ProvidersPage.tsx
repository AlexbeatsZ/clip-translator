import { Check, ExternalLink, Eye, EyeOff, KeyRound, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../lib/ipc";
import { PROVIDERS, providerMeta } from "../lib/languages";
import type { ProviderId } from "../lib/types";
import type { SettingsState } from "./useSettings";
import { Group, PageHeader } from "./layout";

type TestState = { status: "idle" } | { status: "running" } | { status: "ok"; sample: string } | { status: "error"; message: string };

export function ProvidersPage({ state }: { state: SettingsState }) {
  const { preferences, providerStatus, updatePreferences, info } = state;
  const active = preferences.provider;
  const vault = info.platform === "windows" ? "Windows 凭据管理器" : "macOS 钥匙串";

  return (
    <>
      <PageHeader title="翻译服务" description="选择用来翻译的服务。只有识别出的文字会发送给它，图片始终留在本机。" />

      <Group title="使用的服务">
        <div role="radiogroup" aria-label="翻译服务">
          {PROVIDERS.map((provider) => {
            const selected = provider.id === active;
            const configured = providerStatus[provider.id];
            return (
              <button
                key={provider.id}
                type="button"
                role="radio"
                aria-checked={selected}
                className="row row-button provider-row"
                onClick={() => updatePreferences({ provider: provider.id })}
              >
                <span className="radio" aria-hidden="true" />
                <span className={`provider-mark ${provider.id}`} aria-hidden="true">
                  {provider.shortName.slice(0, 1)}
                </span>
                <span className="row-text">
                  <span className="row-title">{provider.name}</span>
                  <span className="row-desc">{provider.description}</span>
                </span>
                <span className={`pill ${configured ? "ok" : "muted"}`}>{configured ? "已配置" : "未配置"}</span>
              </button>
            );
          })}
        </div>
      </Group>

      <ProviderKey key={active} provider={active} configured={providerStatus[active]} vault={vault} onChanged={state.refreshProviders} />
    </>
  );
}

function ProviderKey({
  provider,
  configured,
  vault,
  onChanged,
}: {
  provider: ProviderId;
  configured: boolean;
  vault: string;
  onChanged: () => Promise<void>;
}) {
  const meta = providerMeta(provider);
  const [editing, setEditing] = useState(!configured);
  const [value, setValue] = useState("");
  const [reveal, setReveal] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [test, setTest] = useState<TestState>({ status: "idle" });

  useEffect(() => setEditing(!configured), [configured]);

  const runTest = async () => {
    setTest({ status: "running" });
    try {
      const sample = await api.testProvider(provider);
      setTest({ status: "ok", sample });
    } catch (err) {
      setTest({ status: "error", message: errorMessage(err) });
    }
  };

  const save = async () => {
    const trimmed = value.trim();
    if (!trimmed) return;
    setSaving(true);
    setError("");
    try {
      await api.saveApiKey(provider, trimmed);
      setValue("");
      setReveal(false);
      await onChanged();
      setEditing(false);
      void runTest();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    try {
      await api.deleteApiKey(provider);
      setConfirmRemove(false);
      setTest({ status: "idle" });
      await onChanged();
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  return (
    <Group
      title={`${meta.shortName} 密钥`}
      footer={
        <>
          <ShieldCheck size={13} />
          <span>密钥只保存在 {vault}中，不会写入配置文件。</span>
        </>
      }
    >
      {editing ? (
        <form
          className="key-form"
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <label className="field-label" htmlFor={`key-${provider}`}>
            {meta.keyLabel}
          </label>
          <div className="key-input-row">
            <div className="text-field">
              <KeyRound size={14} aria-hidden="true" />
              <input
                id={`key-${provider}`}
                className="selectable"
                type={reveal ? "text" : "password"}
                value={value}
                autoFocus
                spellCheck={false}
                autoComplete="off"
                placeholder={meta.keyPlaceholder}
                onChange={(event) => setValue(event.target.value)}
              />
              <button type="button" className="icon-btn" aria-label={reveal ? "隐藏密钥" : "显示密钥"} onClick={() => setReveal((current) => !current)}>
                {reveal ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
            </div>
            {configured && (
              <button type="button" className="btn" onClick={() => setEditing(false)}>
                取消
              </button>
            )}
            <button type="submit" className="btn btn-primary" disabled={!value.trim() || saving}>
              {saving ? <span className="spinner" /> : "保存"}
            </button>
          </div>
          <div className="key-help">
            <span>{meta.keyHelp}</span>
            <button type="button" className="link" onClick={() => void api.openUrl(meta.keyUrl)}>
              获取密钥 <ExternalLink size={11} />
            </button>
          </div>
          {error && <div className="inline-status error">{error}</div>}
        </form>
      ) : (
        <>
          <div className="row">
            <span className="row-text">
              <span className="row-title">{meta.keyLabel}</span>
              <span className="row-desc mono">•••• •••• •••• ••••</span>
            </span>
            <div className="row-actions">
              <button type="button" className="btn" onClick={() => void runTest()} disabled={test.status === "running"}>
                {test.status === "running" ? <span className="spinner" /> : "测试连接"}
              </button>
              <button type="button" className="btn" onClick={() => setEditing(true)}>
                更换
              </button>
              {confirmRemove ? (
                <button type="button" className="btn btn-danger" onClick={() => void remove()} onBlur={() => setConfirmRemove(false)} autoFocus>
                  确认移除
                </button>
              ) : (
                <button type="button" className="btn btn-ghost" onClick={() => setConfirmRemove(true)}>
                  移除
                </button>
              )}
            </div>
          </div>
          {test.status === "ok" && (
            <div className="row-note ok">
              <Check size={13} /> 连接正常 · “Hello, world!” → <span className="selectable">{test.sample}</span>
            </div>
          )}
          {test.status === "error" && <div className="row-note error">{test.message}</div>}
          {error && <div className="row-note error">{error}</div>}
        </>
      )}
    </Group>
  );
}
