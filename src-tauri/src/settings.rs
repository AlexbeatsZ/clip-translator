//! Non-secret preferences persisted through tauri-plugin-store (`settings.json`).
//! API keys never live here; they are kept in the OS credential vault (see `translation`).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";
pub const DEFAULT_HOTKEY: &str = "Alt+Shift+T";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub provider: String,
    pub source_language: String,
    pub target_language: String,
    /// Draw translated text back into clipboard images instead of returning text only.
    pub image_replacement: bool,
    /// Keep the original image in the result panel for comparison.
    pub keep_original: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            provider: "deepl".into(),
            source_language: "auto".into(),
            target_language: "zh".into(),
            image_replacement: true,
            keep_original: true,
        }
    }
}

pub fn load<R: Runtime>(app: &AppHandle<R>) -> Preferences {
    let defaults = Preferences::default();
    let Ok(store) = app.store(STORE_FILE) else {
        return defaults;
    };
    let text = |key: &str, fallback: &str| {
        store
            .get(key)
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| fallback.to_owned())
    };
    let flag = |key: &str, fallback: bool| {
        store
            .get(key)
            .and_then(|value| value.as_bool())
            .unwrap_or(fallback)
    };
    Preferences {
        provider: text("provider", &defaults.provider),
        source_language: text("sourceLanguage", &defaults.source_language),
        target_language: text("targetLanguage", &defaults.target_language),
        image_replacement: flag("imageReplacement", defaults.image_replacement),
        keep_original: flag("keepOriginal", defaults.keep_original),
    }
}

pub fn save<R: Runtime>(app: &AppHandle<R>, preferences: &Preferences) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|error| format!("打开设置存储失败：{error}"))?;
    store.set("provider", serde_json::json!(preferences.provider));
    store.set(
        "sourceLanguage",
        serde_json::json!(preferences.source_language),
    );
    store.set(
        "targetLanguage",
        serde_json::json!(preferences.target_language),
    );
    store.set(
        "imageReplacement",
        serde_json::json!(preferences.image_replacement),
    );
    store.set("keepOriginal", serde_json::json!(preferences.keep_original));
    store
        .save()
        .map_err(|error| format!("保存设置失败：{error}"))
}

pub fn load_hotkey<R: Runtime>(app: &AppHandle<R>) -> String {
    app.store(STORE_FILE)
        .ok()
        .and_then(|store| store.get("hotkey"))
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_HOTKEY.into())
}

pub fn save_hotkey<R: Runtime>(app: &AppHandle<R>, hotkey: &str) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|error| format!("打开设置存储失败：{error}"))?;
    store.set("hotkey", serde_json::json!(hotkey));
    store
        .save()
        .map_err(|error| format!("保存快捷键设置失败：{error}"))
}
