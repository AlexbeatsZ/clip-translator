import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, events } from "../lib/ipc";
import type { AppInfo, Preferences, ProviderStatus, SelectionPermission } from "../lib/types";

const DEFAULT_PREFERENCES: Preferences = {
  provider: "deepl",
  sourceLanguage: "auto",
  targetLanguage: "zh",
  imageReplacement: true,
  keepOriginal: true,
};

export interface SettingsState {
  loaded: boolean;
  info: AppInfo;
  preferences: Preferences;
  providerStatus: ProviderStatus;
  permission: SelectionPermission;
  hotkey: string;
  saveError: string;
  updatePreferences: (patch: Partial<Preferences>) => void;
  refreshProviders: () => Promise<void>;
  refreshPermission: () => Promise<void>;
  setHotkey: (value: string) => void;
}

export function useSettings(): SettingsState {
  const [loaded, setLoaded] = useState(false);
  const [info, setInfo] = useState<AppInfo>({ version: "", platform: navigator.userAgent.includes("Windows") ? "windows" : "macos" });
  const [preferences, setPreferences] = useState<Preferences>(DEFAULT_PREFERENCES);
  const [providerStatus, setProviderStatus] = useState<ProviderStatus>({ deepl: false, google: false, yandex: true });
  const [permission, setPermission] = useState<SelectionPermission>({ status: "unknown" });
  const [hotkey, setHotkey] = useState("");
  const [saveError, setSaveError] = useState("");

  const refreshProviders = useCallback(async () => {
    try {
      setProviderStatus(await api.providerStatus());
    } catch {
      /* keychain may be locked; keep the last known state */
    }
  }, []);

  const refreshPermission = useCallback(async () => {
    try {
      setPermission(await api.selectionPermission());
    } catch {
      setPermission({ status: "unknown" });
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    Promise.allSettled([api.appInfo(), api.preferences(), api.hotkey()]).then(([infoResult, prefResult, hotkeyResult]) => {
      if (disposed) return;
      if (infoResult.status === "fulfilled") setInfo(infoResult.value);
      if (prefResult.status === "fulfilled") setPreferences({ ...DEFAULT_PREFERENCES, ...prefResult.value });
      if (hotkeyResult.status === "fulfilled") setHotkey(hotkeyResult.value);
      setLoaded(true);
    });
    void refreshProviders();
    void refreshPermission();

    // Returning from System Settings (Accessibility) or reopening from the tray.
    const onFocus = () => {
      void refreshPermission();
      void refreshProviders();
    };
    window.addEventListener("focus", onFocus);
    const unlisten = events.settingsShown(onFocus);
    return () => {
      disposed = true;
      window.removeEventListener("focus", onFocus);
      void unlisten.then((stop) => stop());
    };
  }, [refreshPermission, refreshProviders]);

  const updatePreferences = useCallback((patch: Partial<Preferences>) => {
    setPreferences((current) => {
      const next = { ...current, ...patch };
      api
        .savePreferences(next)
        .then(() => setSaveError(""))
        .catch((error) => setSaveError(errorMessage(error)));
      return next;
    });
  }, []);

  return {
    loaded,
    info,
    preferences,
    providerStatus,
    permission,
    hotkey,
    saveError,
    updatePreferences,
    refreshProviders,
    refreshPermission,
    setHotkey,
  };
}
