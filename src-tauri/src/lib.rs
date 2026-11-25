mod image_translation;
mod input;
mod ocr;
mod overlay;
mod settings;
mod translation;

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use arboard::{Clipboard, ImageData};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::ImageFormat;
use rapidocr_core::RapidOcr;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, State, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use settings::Preferences;

const MAIN_WINDOW: &str = "main";

struct AppState {
    current_hotkey: Mutex<String>,
    pending_result: Mutex<Option<TranslationResult>>,
    ocr_engine: Arc<Mutex<Option<RapidOcr>>>,
    /// Monotonic id of the latest shortcut invocation; older results are dropped.
    request_seq: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Segment {
    source: String,
    translated: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct TranslationResult {
    request_id: u64,
    /// "text" | "image" | "error"
    kind: String,
    /// "selection" | "clipboard"
    source: Option<String>,
    provider: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
    source_text: Option<String>,
    translated_text: Option<String>,
    image_data_url: Option<String>,
    translated_image_data_url: Option<String>,
    segments: Vec<Segment>,
    width: Option<usize>,
    height: Option<usize>,
    /// Non-fatal information, e.g. "图片重绘失败，已显示文字译文".
    notice: Option<String>,
    error: Option<String>,
    /// "no-input" | "missing-key" | "provider" | "ocr" | "compose" | "internal"
    error_code: Option<String>,
}

impl TranslationResult {
    fn failure(request_id: u64, code: &str, message: impl Into<String>) -> Self {
        Self {
            request_id,
            kind: "error".into(),
            error: Some(message.into()),
            error_code: Some(code.into()),
            ..Default::default()
        }
    }

    fn set_error(&mut self, code: &str, message: impl Into<String>) {
        self.error = Some(message.into());
        self.error_code = Some(code.into());
    }
}

fn provider_error_code(error: &str) -> &'static str {
    if translation::is_missing_key_error(error) {
        "missing-key"
    } else {
        "provider"
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    request_id: u64,
    /// "capture" | "ocr" | "translate" | "compose"
    stage: &'static str,
}

fn emit_progress(app: &AppHandle, request_id: u64, stage: &'static str) {
    let _ = app.emit_to(
        overlay::PANEL_LABEL,
        "translation-progress",
        Progress { request_id, stage },
    );
}

fn is_latest(app: &AppHandle, request_id: u64) -> bool {
    app.state::<AppState>().request_seq.load(Ordering::SeqCst) == request_id
}

// ---------------------------------------------------------------------------
// Result panel commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_pending_result(state: State<'_, AppState>) -> Result<Option<TranslationResult>, String> {
    state
        .pending_result
        .lock()
        .map(|value| value.clone())
        .map_err(|_| "翻译结果状态不可用".into())
}

#[tauri::command]
fn dismiss_result(app: AppHandle) -> Result<(), String> {
    overlay::hide_result_panel(&app)
}

#[tauri::command]
fn set_panel_pinned(pinned: bool) {
    overlay::set_pinned(pinned);
}

#[tauri::command]
fn fit_result_panel(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    overlay::fit_result_panel(&app, width, height)
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|error| format!("无法打开剪贴板：{error}"))?;
    clipboard
        .set_text(text)
        .map_err(|error| format!("复制失败：{error}"))
}

fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let encoded = data_url
        .split_once(',')
        .map(|(_, data)| data)
        .ok_or_else(|| "图片数据格式无效".to_string())?;
    STANDARD
        .decode(encoded)
        .map_err(|error| format!("解码图片失败：{error}"))
}

#[tauri::command]
fn copy_image(image_data_url: String) -> Result<(), String> {
    let bytes = decode_data_url(&image_data_url)?;
    let image = image::load_from_memory(&bytes)
        .map_err(|error| format!("读取图片失败：{error}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let mut clipboard = Clipboard::new().map_err(|error| format!("无法打开剪贴板：{error}"))?;
    clipboard
        .set_image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: std::borrow::Cow::Owned(image.into_raw()),
        })
        .map_err(|error| format!("复制图片失败：{error}"))
}

/// Saves a PNG data URL into the user's Downloads folder and returns the file path.
#[tauri::command]
fn save_image(app: AppHandle, image_data_url: String) -> Result<String, String> {
    let bytes = decode_data_url(&image_data_url)?;
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().picture_dir())
        .map_err(|error| format!("找不到下载文件夹：{error}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default();
    let mut path = dir.join(format!("ClipLingo-{stamp}.png"));
    let mut suffix = 1;
    while path.exists() {
        path = dir.join(format!("ClipLingo-{stamp}-{suffix}.png"));
        suffix += 1;
    }
    std::fs::write(&path, bytes).map_err(|error| format!("保存图片失败：{error}"))?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(PathBuf::from(path))
        .map_err(|error| format!("无法在文件管理器中显示：{error}"))
}

#[tauri::command]
async fn translate_text(
    app: AppHandle,
    text: String,
    provider: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
) -> Result<String, String> {
    let preferences = settings::load(&app);
    translation::translate(
        provider.as_deref().unwrap_or(&preferences.provider),
        &text,
        source_language
            .as_deref()
            .unwrap_or(&preferences.source_language),
        target_language
            .as_deref()
            .unwrap_or(&preferences.target_language),
    )
    .await
}

// ---------------------------------------------------------------------------
// Settings commands
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    platform: &'static str,
}

#[tauri::command]
fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        platform: if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "linux"
        },
    }
}

#[tauri::command]
fn get_preferences(app: AppHandle) -> Preferences {
    settings::load(&app)
}

#[tauri::command]
fn save_preferences(app: AppHandle, preferences: Preferences) -> Result<(), String> {
    settings::save(&app, &preferences)
}

#[tauri::command]
fn get_selection_permission() -> serde_json::Value {
    input::selection_permission_status()
}

/// Opens the OS page where the Accessibility permission is granted. On macOS this also
/// registers the app in the list by asking the system for a trust prompt.
#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = macos_accessibility_client::accessibility::application_is_trusted_with_prompt();
        tauri_plugin_opener::open_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            None::<&str>,
        )
        .map_err(|error| format!("无法打开系统设置：{error}"))?;
    }
    Ok(())
}

#[tauri::command]
fn save_api_key(provider: String, api_key: String) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }
    translation::save_api_key(&provider, &api_key)
}

#[tauri::command]
fn delete_api_key(provider: String) -> Result<(), String> {
    translation::delete_api_key(&provider)
}

#[tauri::command]
fn get_provider_status() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "deepl": translation::has_api_key("deepl")?,
        "google": translation::has_api_key("google")?,
    }))
}

/// Verifies a stored key with a tiny real request and returns the sample translation.
#[tauri::command]
async fn test_provider(provider: String) -> Result<String, String> {
    translation::translate(&provider, "Hello, world!", "en", "zh").await
}

#[tauri::command]
fn get_hotkey(state: State<'_, AppState>) -> Result<String, String> {
    state
        .current_hotkey
        .lock()
        .map(|value| value.clone())
        .map_err(|_| "快捷键状态不可用".into())
}

#[tauri::command]
fn set_hotkey(app: AppHandle, state: State<'_, AppState>, shortcut: String) -> Result<(), String> {
    let shortcut = shortcut.trim().to_owned();
    if shortcut.is_empty() {
        return Err("快捷键不能为空".into());
    }
    let mut current = state
        .current_hotkey
        .lock()
        .map_err(|_| "快捷键状态不可用")?;
    let shortcuts = app.global_shortcut();
    // The recorder suspends the current hotkey; make sure it is not registered twice.
    if shortcuts.is_registered(current.as_str()) {
        shortcuts
            .unregister(current.as_str())
            .map_err(|error| format!("释放旧快捷键失败：{error}"))?;
    }
    if let Err(error) = shortcuts.register(shortcut.as_str()) {
        let restore = shortcuts.register(current.as_str());
        return Err(match restore {
            Ok(()) => format!("无法注册该快捷键，可能已被其他程序占用（{error}）"),
            Err(restore_error) => {
                format!("注册新快捷键失败：{error}；恢复旧快捷键也失败：{restore_error}")
            }
        });
    }
    if let Err(error) = settings::save_hotkey(&app, &shortcut) {
        let _ = shortcuts.unregister(shortcut.as_str());
        let _ = shortcuts.register(current.as_str());
        return Err(error);
    }
    *current = shortcut;
    Ok(())
}

/// Temporarily releases the global hotkey while the settings page records a new one, so
/// pressing the current combination is captured instead of starting a translation.
#[tauri::command]
fn suspend_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    suspended: bool,
) -> Result<(), String> {
    let current = state
        .current_hotkey
        .lock()
        .map_err(|_| "快捷键状态不可用")?;
    let shortcuts = app.global_shortcut();
    let registered = shortcuts.is_registered(current.as_str());
    if suspended && registered {
        shortcuts
            .unregister(current.as_str())
            .map_err(|error| format!("暂停快捷键失败：{error}"))?;
    } else if !suspended && !registered {
        shortcuts
            .register(current.as_str())
            .map_err(|error| format!("恢复快捷键失败：{error}"))?;
    }
    Ok(())
}

#[tauri::command]
fn trigger_translation(app: AppHandle) {
    process_shortcut(app);
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    show_main_window(&app)
}

fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or_else(|| "设置窗口不存在".to_string())?;
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    let _ = window.unminimize();
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    let _ = app.emit_to(MAIN_WINDOW, "settings-shown", ());
    Ok(())
}

// ---------------------------------------------------------------------------
// Translation pipeline
// ---------------------------------------------------------------------------

fn resolve_ocr_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("resources/ocr-models"));
        candidates.push(resource_dir.join("ocr-models"));
    }
    if cfg!(debug_assertions) {
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ocr-models"));
    }
    candidates
        .into_iter()
        .find(|path| path.join("PP-OCRv6_det_small.onnx").is_file())
        .ok_or_else(|| "找不到随软件安装的 OCR 模型文件".into())
}

async fn translate_selection_or_text(
    app: &AppHandle,
    request_id: u64,
    payload: input::InputPayload,
    preferences: &Preferences,
) -> TranslationResult {
    let source_text = payload.text.unwrap_or_default();
    let mut result = TranslationResult {
        request_id,
        kind: "text".into(),
        source: Some(payload.source),
        provider: Some(preferences.provider.clone()),
        source_language: Some(preferences.source_language.clone()),
        target_language: Some(preferences.target_language.clone()),
        source_text: Some(source_text.clone()),
        ..Default::default()
    };
    emit_progress(app, request_id, "translate");
    match translation::translate(
        &preferences.provider,
        &source_text,
        &preferences.source_language,
        &preferences.target_language,
    )
    .await
    {
        Ok(translated) => result.translated_text = Some(translated),
        Err(error) => result.set_error(provider_error_code(&error), error),
    }
    result
}

async fn translate_clipboard_image(
    app: &AppHandle,
    request_id: u64,
    payload: input::InputPayload,
    preferences: &Preferences,
) -> TranslationResult {
    let mut result = TranslationResult {
        request_id,
        kind: "image".into(),
        source: Some(payload.source),
        provider: Some(preferences.provider.clone()),
        source_language: Some(preferences.source_language.clone()),
        target_language: Some(preferences.target_language.clone()),
        image_data_url: payload.image_data_url.clone(),
        width: payload.width,
        height: payload.height,
        ..Default::default()
    };
    let Some(image_data_url) = payload.image_data_url else {
        return TranslationResult::failure(request_id, "internal", "剪贴板图片数据为空");
    };
    let png_bytes = match decode_data_url(&image_data_url) {
        Ok(bytes) => bytes,
        Err(error) => return TranslationResult::failure(request_id, "internal", error),
    };
    let model_dir = match resolve_ocr_model_dir(app) {
        Ok(path) => path,
        Err(error) => {
            result.set_error("ocr", error);
            return result;
        }
    };

    emit_progress(app, request_id, "ocr");
    let engine_cell = app.state::<AppState>().ocr_engine.clone();
    let ocr_output = tauri::async_runtime::spawn_blocking(move || {
        let original = image::load_from_memory(&png_bytes)
            .map_err(|error| format!("读取截图失败：{error}"))?
            .to_rgb8();
        let mut engine_slot = engine_cell
            .lock()
            .map_err(|_| "OCR 引擎状态不可用".to_string())?;
        if engine_slot.is_none() {
            *engine_slot = Some(ocr::load_engine(&model_dir)?);
        }
        let lines = ocr::recognize(engine_slot.as_mut().unwrap(), &original)?;
        Ok::<_, String>((original, lines))
    })
    .await;
    let (original, lines) = match ocr_output {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            result.set_error("ocr", error);
            return result;
        }
        Err(error) => {
            return TranslationResult::failure(
                request_id,
                "internal",
                format!("OCR 任务异常：{error}"),
            );
        }
    };
    let (width, height) = original.dimensions();
    result.width = Some(width as usize);
    result.height = Some(height as usize);
    let blocks = image_translation::group_ocr_lines(&lines, width, height);
    eprintln!(
        "[ClipLingo] local OCR recognized {} text blocks",
        blocks.len()
    );
    if blocks.is_empty() {
        result.set_error("ocr", "没有在图片中识别到文字");
        return result;
    }

    let source_texts: Vec<String> = blocks.iter().map(|block| block.text.clone()).collect();
    result.source_text = Some(source_texts.join("\n\n"));
    if !is_latest(app, request_id) {
        return result;
    }

    emit_progress(app, request_id, "translate");
    let translations = match translation::translate_many(
        &preferences.provider,
        &source_texts,
        &preferences.source_language,
        &preferences.target_language,
    )
    .await
    {
        Ok(values) => values,
        Err(error) => {
            result.set_error(provider_error_code(&error), error);
            return result;
        }
    };
    result.translated_text = Some(translations.join("\n\n"));
    result.segments = source_texts
        .iter()
        .zip(&translations)
        .map(|(source, translated)| Segment {
            source: source.clone(),
            translated: translated.clone(),
        })
        .collect();
    if !preferences.image_replacement || !is_latest(app, request_id) {
        if !preferences.keep_original {
            result.image_data_url = None;
        }
        return result;
    }

    emit_progress(app, request_id, "compose");
    let compose = tauri::async_runtime::spawn_blocking(move || {
        let image = image_translation::compose_translated_image(&original, &blocks, &translations)?;
        let mut png = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut png, ImageFormat::Png)
            .map_err(|error| format!("编码翻译图片失败：{error}"))?;
        Ok::<_, String>(png.into_inner())
    })
    .await;
    match compose {
        Ok(Ok(png)) => {
            result.translated_image_data_url =
                Some(format!("data:image/png;base64,{}", STANDARD.encode(png)));
        }
        Ok(Err(error)) => {
            result.notice = Some(format!("图片重绘失败，已改为显示文字译文：{error}"));
        }
        Err(error) => {
            result.notice = Some(format!("图片重绘任务异常，已改为显示文字译文：{error}"));
        }
    }
    if !preferences.keep_original && result.translated_image_data_url.is_some() {
        result.image_data_url = None;
    }
    result
}

fn process_shortcut(app: AppHandle) {
    let request_id = app
        .state::<AppState>()
        .request_seq
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    eprintln!("[ClipLingo] translation request #{request_id}");
    tauri::async_runtime::spawn(async move {
        if let Ok(mut pending) = app.state::<AppState>().pending_result.lock() {
            *pending = None;
        }
        emit_progress(&app, request_id, "capture");
        if let Err(error) = overlay::show_result_panel(&app, !overlay::is_visible()) {
            eprintln!("[ClipLingo] failed to show result panel: {error}");
        }

        let preferences = settings::load(&app);
        let captured = tauri::async_runtime::spawn_blocking(input::capture_current_input).await;
        let result = match captured {
            Ok(Ok(payload)) if payload.kind == "text" => {
                eprintln!(
                    "[ClipLingo] captured text from {} ({} chars)",
                    payload.source,
                    payload
                        .text
                        .as_deref()
                        .map(|text| text.chars().count())
                        .unwrap_or(0)
                );
                translate_selection_or_text(&app, request_id, payload, &preferences).await
            }
            Ok(Ok(payload)) if payload.kind == "image" => {
                eprintln!("[ClipLingo] captured image from clipboard");
                translate_clipboard_image(&app, request_id, payload, &preferences).await
            }
            Ok(Ok(_)) => {
                TranslationResult::failure(request_id, "no-input", "无法识别剪贴板内容类型")
            }
            Ok(Err(error)) => TranslationResult::failure(request_id, "no-input", error),
            Err(error) => TranslationResult::failure(
                request_id,
                "internal",
                format!("读取选区或剪贴板失败：{error}"),
            ),
        };

        if !is_latest(&app, request_id) {
            eprintln!("[ClipLingo] dropping stale result #{request_id}");
            return;
        }
        if let Ok(mut pending) = app.state::<AppState>().pending_result.lock() {
            *pending = Some(result.clone());
        }
        let _ = app.emit_to(overlay::PANEL_LABEL, "translation-result", result);
    });
}

// ---------------------------------------------------------------------------
// App setup
// ---------------------------------------------------------------------------

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let translate = MenuItem::with_id(app, "translate", "翻译选区或剪贴板", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "设置…", true, Some("CmdOrCtrl+,"))?;
    let quit = MenuItem::with_id(app, "quit", "退出 ClipLingo", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &translate,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &quit,
        ],
    )?;

    #[cfg(target_os = "macos")]
    let icon =
        tauri::image::Image::from_bytes(include_bytes!("../icons/tray/tray-template@2x.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("ClipLingo")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "translate" => process_shortcut(app.clone()),
            "settings" => {
                let _ = show_main_window(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(AppState {
            current_hotkey: Mutex::new(settings::DEFAULT_HOTKEY.into()),
            pending_result: Mutex::new(None),
            ocr_engine: Arc::new(Mutex::new(None)),
            request_seq: AtomicU64::new(0),
        })
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build());

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        process_shortcut(app.clone());
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let hotkey = settings::load_hotkey(&handle);
            if let Err(error) = app.global_shortcut().register(hotkey.as_str()) {
                eprintln!("[ClipLingo] failed to register hotkey {hotkey}: {error}");
            }
            *app.state::<AppState>().current_hotkey.lock().unwrap() = hotkey;

            overlay::create_result_panel(app)?;
            build_tray(app)?;

            // Closing the settings window keeps ClipLingo running in the menu bar / tray.
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let window_handle = handle.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(window) = window_handle.get_webview_window(MAIN_WINDOW) {
                            let _ = window.hide();
                        }
                        #[cfg(target_os = "macos")]
                        let _ =
                            window_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                    }
                });
            }

            // First run (no provider configured yet): open settings. Otherwise start quietly.
            let configured = translation::has_api_key("deepl").unwrap_or(false)
                || translation::has_api_key("google").unwrap_or(false);
            if configured {
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            } else {
                let _ = show_main_window(&handle);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_pending_result,
            dismiss_result,
            set_panel_pinned,
            fit_result_panel,
            copy_text,
            copy_image,
            save_image,
            reveal_path,
            translate_text,
            get_app_info,
            get_preferences,
            save_preferences,
            get_selection_permission,
            open_accessibility_settings,
            save_api_key,
            delete_api_key,
            get_provider_status,
            test_provider,
            get_hotkey,
            set_hotkey,
            suspend_hotkey,
            trigger_translation,
            open_settings,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ClipLingo")
        .run(|app, event| match event {
            // Keep running in the tray after the last window is hidden.
            tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
                api.prevent_exit();
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                let _ = show_main_window(app);
            }
            _ => {
                let _ = app;
            }
        });
}
