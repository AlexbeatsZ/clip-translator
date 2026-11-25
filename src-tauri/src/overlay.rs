//! Floating result panel.
//!
//! macOS: a non-activating `NSPanel` that joins the active full-screen Space, so showing a
//! result never switches desktops or steals focus. Windows: a top-most tool window.
//! The panel opens next to the mouse cursor, sizes itself to the rendered content
//! (`fit_result_panel`), and closes when the user clicks elsewhere unless it is pinned.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    App, AppHandle, LogicalSize, Manager, PhysicalPosition, Position, Runtime, Size, WebviewUrl,
    WebviewWindow,
};

#[cfg(target_os = "macos")]
use tauri_nspanel::{
    CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask, tauri_panel,
};

pub const PANEL_LABEL: &str = "translation-panel";
const PANEL_WIDTH: f64 = 420.0;
const PANEL_INITIAL_HEIGHT: f64 = 180.0;
const CURSOR_GAP: f64 = 16.0;
const SCREEN_MARGIN: f64 = 12.0;

static PANEL_VISIBLE: AtomicBool = AtomicBool::new(false);
static PANEL_PINNED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(TranslationPanel {
        config: {
            can_become_key_window: true,
            is_floating_panel: true
        }
    })
}

pub fn create_result_panel<R: Runtime>(app: &mut App<R>) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let handle = app.handle().clone();
        PanelBuilder::<_, TranslationPanel<R>>::new(&handle, PANEL_LABEL)
            .url(WebviewUrl::App("index.html?view=result".into()))
            .title("ClipLingo")
            .size(Size::Logical(LogicalSize::new(
                PANEL_WIDTH,
                PANEL_INITIAL_HEIGHT,
            )))
            .level(PanelLevel::Floating)
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary()
                    .ignores_cycle(),
            )
            .add_style_mask(StyleMask::empty().nonactivating_panel())
            .no_activate(true)
            .hides_on_deactivate(false)
            .becomes_key_only_if_needed(true)
            .resizable(false)
            .has_shadow(true)
            .corner_radius(12.0)
            .with_window(|window| {
                window
                    .decorations(false)
                    .visible(false)
                    .focused(false)
                    .skip_taskbar(true)
            })
            .build()?;
        install_outside_click_monitor(&handle);
    }

    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{WebviewWindowBuilder, WindowEvent};
        let window = WebviewWindowBuilder::new(
            app,
            PANEL_LABEL,
            WebviewUrl::App("index.html?view=result".into()),
        )
        .title("ClipLingo")
        .inner_size(PANEL_WIDTH, PANEL_INITIAL_HEIGHT)
        .decorations(false)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()?;
        let handle = app.handle().clone();
        // Windows has no non-activating panel; once the user has clicked into the panel,
        // losing focus is the equivalent of "clicked elsewhere".
        window.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                if PANEL_VISIBLE.load(Ordering::SeqCst) && !PANEL_PINNED.load(Ordering::SeqCst) {
                    let _ = hide_result_panel(&handle);
                }
            }
        });
    }

    Ok(())
}

/// Closes the panel when the user clicks in another application. A global monitor only
/// receives events delivered to other processes, so clicks inside the panel are ignored.
/// Mouse-down monitoring does not require the Accessibility permission.
#[cfg(target_os = "macos")]
fn install_outside_click_monitor<R: Runtime>(app: &AppHandle<R>) {
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask};
    use std::ptr::NonNull;

    let handle = app.clone();
    let block = RcBlock::new(move |_event: NonNull<NSEvent>| {
        if PANEL_VISIBLE.load(Ordering::SeqCst) && !PANEL_PINNED.load(Ordering::SeqCst) {
            let _ = hide_result_panel(&handle);
        }
    });
    let mask =
        NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &block);
    // The monitor lives for the whole process.
    std::mem::forget(monitor);
}

fn panel_window<R: Runtime>(app: &AppHandle<R>) -> Result<WebviewWindow<R>, String> {
    app.get_webview_window(PANEL_LABEL)
        .ok_or_else(|| "翻译浮窗尚未创建".to_string())
}

pub fn is_visible() -> bool {
    PANEL_VISIBLE.load(Ordering::SeqCst)
}

pub fn set_pinned(pinned: bool) {
    PANEL_PINNED.store(pinned, Ordering::SeqCst);
}

/// Shows the panel. When `reposition` is true it is moved next to the mouse cursor;
/// otherwise it stays where it is (used when a result replaces a loading state).
pub fn show_result_panel<R: Runtime>(app: &AppHandle<R>, reposition: bool) -> Result<(), String> {
    let window = panel_window(app)?;
    if reposition {
        if let Err(error) = place_near_cursor(app, &window) {
            eprintln!("[ClipLingo] could not place panel near cursor: {error}");
            let _ = window.center();
        }
    }
    PANEL_VISIBLE.store(true, Ordering::SeqCst);

    #[cfg(target_os = "macos")]
    {
        let panel = app
            .get_webview_panel(PANEL_LABEL)
            .map_err(|error| format!("无法获取翻译浮窗：{error:?}"))?;
        app.run_on_main_thread(move || panel.show())
            .map_err(|error| format!("无法在主线程显示翻译浮窗：{error}"))?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        window
            .set_always_on_top(true)
            .map_err(|error| format!("设置浮窗置顶失败：{error}"))?;
        window
            .show()
            .map_err(|error| format!("显示翻译浮窗失败：{error}"))?;
    }
    Ok(())
}

pub fn hide_result_panel<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    PANEL_VISIBLE.store(false, Ordering::SeqCst);
    PANEL_PINNED.store(false, Ordering::SeqCst);

    #[cfg(target_os = "macos")]
    {
        let panel = app
            .get_webview_panel(PANEL_LABEL)
            .map_err(|error| format!("无法获取翻译浮窗：{error:?}"))?;
        app.run_on_main_thread(move || panel.hide())
            .map_err(|error| format!("无法在主线程隐藏翻译浮窗：{error}"))?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        panel_window(app)?
            .hide()
            .map_err(|error| format!("隐藏翻译浮窗失败：{error}"))?;
    }
    Ok(())
}

/// Resizes the panel to the content size reported by the web view, keeping the top-left
/// corner in place and the whole panel inside the monitor's work area.
pub fn fit_result_panel<R: Runtime>(
    app: &AppHandle<R>,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let window = panel_window(app)?;
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let scale = window.scale_factor().unwrap_or(1.0);
    let (max_width, max_height) = monitor
        .as_ref()
        .map(|monitor| {
            let area = monitor.work_area();
            (
                area.size.width as f64 / scale - SCREEN_MARGIN * 2.0,
                area.size.height as f64 / scale - SCREEN_MARGIN * 2.0,
            )
        })
        .unwrap_or((1200.0, 900.0));
    let width = width.clamp(280.0, max_width.max(280.0)).round();
    let height = height.clamp(80.0, max_height.max(80.0)).round();

    let before = window.outer_position().ok();
    window
        .set_size(Size::Logical(LogicalSize::new(width, height)))
        .map_err(|error| format!("调整浮窗尺寸失败：{error}"))?;

    if let (Some(before), Some(monitor)) = (before, monitor) {
        let area = monitor.work_area();
        let margin = (SCREEN_MARGIN * scale).round() as i32;
        let physical_width = (width * scale).round() as i32;
        let physical_height = (height * scale).round() as i32;
        let min_x = area.position.x + margin;
        let min_y = area.position.y + margin;
        let max_x = area.position.x + area.size.width as i32 - physical_width - margin;
        let max_y = area.position.y + area.size.height as i32 - physical_height - margin;
        let x = before.x.clamp(min_x, max_x.max(min_x));
        let y = before.y.clamp(min_y, max_y.max(min_y));
        window
            .set_position(Position::Physical(PhysicalPosition::new(x, y)))
            .map_err(|error| format!("定位翻译浮窗失败：{error}"))?;
    }
    Ok(())
}

fn place_near_cursor<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> Result<(), String> {
    let cursor = app.cursor_position().map_err(|error| error.to_string())?;
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| "找不到显示器".to_string())?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let (width, height) = (size.width as f64, size.height as f64);
    let gap = CURSOR_GAP * scale;
    let margin = SCREEN_MARGIN * scale;

    let left = area.position.x as f64 + margin;
    let top = area.position.y as f64 + margin;
    let right = area.position.x as f64 + area.size.width as f64 - margin;
    let bottom = area.position.y as f64 + area.size.height as f64 - margin;

    let mut x = cursor.x - width / 2.0;
    let mut y = cursor.y + gap;
    if y + height > bottom {
        y = cursor.y - gap - height;
    }
    x = x.clamp(left, (right - width).max(left));
    y = y.clamp(top, (bottom - height).max(top));
    window
        .set_position(Position::Physical(PhysicalPosition::new(
            x.round() as i32,
            y.round() as i32,
        )))
        .map_err(|error| error.to_string())
}
