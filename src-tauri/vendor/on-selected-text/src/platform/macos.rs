use std::{num::NonZeroUsize, thread, time::Duration};

use accessibility_ng::{AXAttribute, AXUIElement, AXValue, Error as AccessibilityError};
use accessibility_sys_ng::{
    kAXErrorCannotComplete, kAXErrorNoValue, kAXFocusedUIElementAttribute,
    kAXSelectedTextAttribute, kAXSelectedTextRangeAttribute,
};
use core_foundation::{
    base::{CFRange, TCFType},
    boolean::CFBoolean,
    string::CFString,
};
use core_graphics_types::geometry::CGRect;
use lru::LruCache;
use parking_lot::Mutex;

use crate::{GetTextError, GetTextOptions, PermissionStatus, ScreenRect, SelectionResult};

static GET_SELECTED_TEXT_METHOD: Mutex<Option<LruCache<String, ProviderKind>>> = Mutex::new(None);

const AX_MANUAL_ACCESSIBILITY_ATTRIBUTE: &str = "AXManualAccessibility";
const AX_FOCUSED_ELEMENT_RETRIES: usize = 2;
const AX_FOCUSED_ELEMENT_RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderKind {
    A11y,
    Clipboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedTextRange {
    location: isize,
    length: isize,
}

impl SelectedTextRange {
    fn as_cf_range(self) -> CFRange {
        CFRange {
            location: self.location,
            length: self.length,
        }
    }
}

/// macOS 平台入口，保留“按应用缓存最优策略”的旧行为。
pub(crate) fn get_selected_text_with_options(
    opts: &GetTextOptions,
) -> Result<SelectionResult, GetTextError> {
    ensure_cache();

    let app_name = active_app_name()?;
    if let Some(result) = try_cached_provider(&app_name, opts) {
        return result;
    }

    match try_a11y_provider(&app_name, opts) {
        Ok(result) => {
            cache_provider(app_name, ProviderKind::A11y);
            Ok(result)
        }
        Err(GetTextError::NoTextSelected) => Err(GetTextError::NoTextSelected),
        Err(a11y_error) => {
            if !opts.allow_clipboard_fallback {
                return Err(a11y_error);
            }

            match try_clipboard_provider(opts) {
                Ok(result) => {
                    cache_provider(app_name, ProviderKind::Clipboard);
                    Ok(result)
                }
                Err(clipboard_error) => Err(merge_provider_errors(a11y_error, clipboard_error)),
            }
        }
    }
}

/// 检查 macOS 辅助功能权限。
pub(crate) fn check_permissions() -> PermissionStatus {
    if macos_accessibility_client::accessibility::application_is_trusted() {
        PermissionStatus::Granted
    } else {
        PermissionStatus::Denied {
            platform: "macOS",
            permission: "Accessibility",
            guidance:
                "Enable this application in System Settings > Privacy & Security > Accessibility.",
        }
    }
}

/// 初始化按应用缓存。
fn ensure_cache() {
    let mut cache = GET_SELECTED_TEXT_METHOD.lock();
    if cache.is_none() {
        *cache = Some(LruCache::new(
            NonZeroUsize::new(100).expect("cache capacity is non-zero"),
        ));
    }
}

/// 获取当前活跃应用名称，用于复用历史最优策略。
fn active_app_name() -> Result<String, GetTextError> {
    // Vendored patch (ClipLingo): the upstream crate used active-win-pos-rs, whose
    // bindgen build script crashes with the Xcode 26 SDK. NSWorkspace gives the same name.
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    workspace
        .frontmostApplication()
        .and_then(|app| app.localizedName())
        .map(|name| name.to_string())
        .ok_or_else(|| GetTextError::fallback_failed("frontmost application lookup failed"))
}

/// 优先尝试缓存过的 provider；失败时返回 None，让完整链路重新判断。
fn try_cached_provider(
    app_name: &str,
    opts: &GetTextOptions,
) -> Option<Result<SelectionResult, GetTextError>> {
    let cached = {
        let mut cache = GET_SELECTED_TEXT_METHOD.lock();
        cache.as_mut()?.get(app_name).copied()
    }?;

    let result = match cached {
        ProviderKind::A11y => try_a11y_provider(app_name, opts),
        ProviderKind::Clipboard if opts.allow_clipboard_fallback => try_clipboard_provider(opts),
        ProviderKind::Clipboard => Err(GetTextError::FallbackDisabled),
    };

    match result {
        Ok(result) if !result.text.is_empty() => Some(Ok(result)),
        Ok(_) => Some(Err(GetTextError::NoTextSelected)),
        Err(_) => None,
    }
}

/// 记录某个应用可用的 provider。
fn cache_provider(app_name: String, provider: ProviderKind) {
    let mut cache = GET_SELECTED_TEXT_METHOD.lock();
    if let Some(cache) = cache.as_mut() {
        cache.put(app_name, provider);
    }
}

/// 通过 macOS A11y API 直接读取选中文本。
fn try_a11y_provider(
    app_name: &str,
    opts: &GetTextOptions,
) -> Result<SelectionResult, GetTextError> {
    let selected_element = get_focused_element()?;
    let range = get_selected_text_range_by_ax(&selected_element);
    let text = get_selected_text_by_ax(&selected_element).ok();
    let first_result = classify_ax_selection(range, text);

    if !should_retry_with_manual_accessibility(app_name, range, &first_result) {
        return attach_ax_bounds_if_requested(
            &selected_element,
            opts.fetch_bounds,
            range,
            first_result,
        );
    }

    if !try_enable_manual_accessibility(&selected_element) {
        return attach_ax_bounds_if_requested(
            &selected_element,
            opts.fetch_bounds,
            range,
            first_result,
        );
    }

    // Chromium/Electron 需要一点时间把内部 DOM 映射到 AX 树。
    thread::sleep(Duration::from_millis(50));
    let retry_range = get_selected_text_range_by_ax(&selected_element);
    let retry_text = get_selected_text_by_ax(&selected_element).ok();

    attach_ax_bounds_if_requested(
        &selected_element,
        opts.fetch_bounds,
        retry_range,
        classify_ax_selection(retry_range, retry_text),
    )
}

/// 通过 AppleScript 模拟 Cmd+C 读取选中文本。
fn try_clipboard_provider(opts: &GetTextOptions) -> Result<SelectionResult, GetTextError> {
    let text =
        super::macos_clipboard::copy_selected_text_with_native_transaction(opts.max_wait_ms)?;

    if text.is_empty() {
        Err(GetTextError::NoTextSelected)
    } else {
        Ok(SelectionResult::new(text))
    }
}

/// 获取当前焦点 AX 元素。
fn get_focused_element() -> Result<AXUIElement, GetTextError> {
    let system_element = AXUIElement::system_wide();
    let focused_attribute =
        AXAttribute::new(&CFString::from_static_string(kAXFocusedUIElementAttribute));

    for attempt in 0..=AX_FOCUSED_ELEMENT_RETRIES {
        match system_element.attribute(&focused_attribute) {
            Ok(value) => {
                return value.downcast_into::<AXUIElement>().ok_or_else(|| {
                    GetTextError::fallback_failed("AX focused element has unexpected type")
                });
            }
            Err(error)
                if attempt < AX_FOCUSED_ELEMENT_RETRIES
                    && is_retryable_focused_element_error(&error) =>
            {
                thread::sleep(AX_FOCUSED_ELEMENT_RETRY_DELAY);
            }
            Err(error) => {
                return Err(map_focused_element_error(error));
            }
        }
    }

    unreachable!("focused element retry loop must return")
}

fn map_focused_element_error(error: AccessibilityError) -> GetTextError {
    GetTextError::fallback_failed(format!("AX focused element: {error:?}"))
}

fn is_retryable_focused_element_error(error: &AccessibilityError) -> bool {
    match error {
        AccessibilityError::Ax(code) => is_retryable_focused_element_ax_error_code(*code),
        _ => false,
    }
}

fn is_retryable_focused_element_ax_error_code(code: i32) -> bool {
    code == kAXErrorCannotComplete || code == kAXErrorNoValue
}

/// 读取当前焦点元素的 AXSelectedTextRange 属性。
fn get_selected_text_range_by_ax(selected_element: &AXUIElement) -> Option<SelectedTextRange> {
    let range_value = selected_element
        .attribute(&AXAttribute::new(&CFString::from_static_string(
            kAXSelectedTextRangeAttribute,
        )))
        .ok()?
        .downcast_into::<AXValue>()?;
    let range = range_value.get_value::<CFRange>().ok()?;

    Some(SelectedTextRange {
        location: range.location,
        length: range.length,
    })
}

/// 读取当前焦点元素的 AXSelectedText 属性。
fn get_selected_text_by_ax(selected_element: &AXUIElement) -> Result<String, GetTextError> {
    let selected_text = selected_element
        .attribute(&AXAttribute::new(&CFString::from_static_string(
            kAXSelectedTextAttribute,
        )))
        .map_err(|error| GetTextError::fallback_failed(format!("AX selected text: {error:?}")))?
        .downcast_into::<CFString>()
        .ok_or_else(|| GetTextError::fallback_failed("AX selected text has unexpected type"))?;

    Ok(selected_text.to_string())
}

/// 在调用方请求时为 AX 文本结果附加选区屏幕坐标；失败时保留纯文本结果。
fn attach_ax_bounds_if_requested(
    selected_element: &AXUIElement,
    fetch_bounds: bool,
    range: Option<SelectedTextRange>,
    result: Result<SelectionResult, GetTextError>,
) -> Result<SelectionResult, GetTextError> {
    let mut selection = result?;
    if fetch_bounds {
        if let Some(bounds) = range
            .and_then(|range| get_bounds_for_selected_text_range_by_ax(selected_element, range))
        {
            selection = selection.with_bounds(bounds);
        }
    }

    Ok(selection)
}

/// 通过 AXBoundsForRange 读取当前选区屏幕坐标。
fn get_bounds_for_selected_text_range_by_ax(
    selected_element: &AXUIElement,
    range: SelectedTextRange,
) -> Option<ScreenRect> {
    if range.length <= 0 {
        return None;
    }

    let range_value = AXValue::from_CFRange(range.as_cf_range()).ok()?;
    let bounds_value = selected_element
        .parameterized_attribute::<AXValue, AXValue>(&AXAttribute::bounds_for_range(), &range_value)
        .ok()?;
    let rect = bounds_value.get_value::<CGRect>().ok()?;

    cg_rect_to_screen_rect(rect)
}

/// 将 CoreGraphics CGRect 转换为公开屏幕矩形。
fn cg_rect_to_screen_rect(rect: CGRect) -> Option<ScreenRect> {
    let x = rect.origin.x;
    let y = rect.origin.y;
    let width = rect.size.width;
    let height = rect.size.height;
    if ![x, y, width, height].iter().all(|value| value.is_finite()) || width <= 0.0 || height <= 0.0
    {
        return None;
    }

    Some(ScreenRect {
        x,
        y,
        width,
        height,
    })
}

/// 判断是否值得启用 AXManualAccessibility 后重试。
fn should_retry_with_manual_accessibility(
    app_name: &str,
    range: Option<SelectedTextRange>,
    result: &Result<SelectionResult, GetTextError>,
) -> bool {
    is_likely_chromium_accessibility_host(app_name)
        && matches!(result, Err(GetTextError::FallbackFailed(_)))
        && range.map(|value| value.length > 0).unwrap_or(false)
}

/// 判断当前应用是否属于常见 Chromium/Electron 无障碍宿主。
fn is_likely_chromium_accessibility_host(app_name: &str) -> bool {
    let app_name = app_name.to_ascii_lowercase();
    [
        "electron",
        "chrome",
        "chromium",
        "visual studio code",
        "code",
        "slack",
        "obsidian",
        "brave",
        "edge",
        "teams",
        "discord",
        "notion",
    ]
    .iter()
    .any(|name| app_name.contains(name))
}

/// 为 Chromium/Electron 类应用启用手动无障碍树映射。
fn try_enable_manual_accessibility(selected_element: &AXUIElement) -> bool {
    let Ok(pid) = selected_element.pid() else {
        return false;
    };
    let app_element = AXUIElement::application(pid);
    let attribute = AXAttribute::new(&CFString::from_static_string(
        AX_MANUAL_ACCESSIBILITY_ATTRIBUTE,
    ));
    let value = CFBoolean::true_value().into_CFType();

    app_element.set_attribute(&attribute, value).is_ok()
}

/// 根据 AX range + AX text 的组合结果判断是否成功、无选区或需要降级。
fn classify_ax_selection(
    range: Option<SelectedTextRange>,
    text: Option<String>,
) -> Result<SelectionResult, GetTextError> {
    if let Some(range) = range {
        if range.length <= 0 {
            return Err(GetTextError::NoTextSelected);
        }

        return match text.filter(|value| !value.is_empty()) {
            Some(text) => Ok(SelectionResult::new(text)),
            None => Err(GetTextError::fallback_failed(
                "AXSelectedTextRange indicates a non-empty selection but AXSelectedText is empty",
            )),
        };
    }

    match text.filter(|value| !value.is_empty()) {
        Some(text) => Ok(SelectionResult::new(text)),
        None => Err(GetTextError::fallback_failed(
            "AXSelectedTextRange is unavailable and AXSelectedText is empty",
        )),
    }
}

/// 合并两个 provider 的失败原因，优先保留权限错误。
fn merge_provider_errors(a11y_error: GetTextError, clipboard_error: GetTextError) -> GetTextError {
    if matches!(clipboard_error, GetTextError::PermissionDenied { .. }) {
        return clipboard_error;
    }

    if matches!(a11y_error, GetTextError::PermissionDenied { .. }) {
        return a11y_error;
    }

    GetTextError::fallback_failed(format!(
        "macOS a11y failed: {a11y_error}; macOS clipboard failed: {clipboard_error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        cg_rect_to_screen_rect, classify_ax_selection, is_likely_chromium_accessibility_host,
        is_retryable_focused_element_ax_error_code, should_retry_with_manual_accessibility,
        SelectedTextRange,
    };
    use crate::{GetTextError, ScreenRect};
    use accessibility_sys_ng::{kAXErrorCannotComplete, kAXErrorNoValue};
    use core_graphics_types::geometry::{CGPoint, CGRect, CGSize};

    #[test]
    fn focused_element_retry_handles_transient_ax_errors() {
        assert!(is_retryable_focused_element_ax_error_code(
            kAXErrorCannotComplete
        ));
        assert!(is_retryable_focused_element_ax_error_code(kAXErrorNoValue));
        assert!(!is_retryable_focused_element_ax_error_code(0));
    }

    #[test]
    fn manual_accessibility_retry_only_applies_to_non_empty_range_fallback() {
        let fallback = Err(GetTextError::FallbackFailed("empty text".into()));
        let no_text = Err(GetTextError::NoTextSelected);

        assert!(should_retry_with_manual_accessibility(
            "Visual Studio Code",
            Some(SelectedTextRange {
                location: 0,
                length: 1
            }),
            &fallback,
        ));
        assert!(!should_retry_with_manual_accessibility(
            "Visual Studio Code",
            Some(SelectedTextRange {
                location: 0,
                length: 0
            }),
            &fallback,
        ));
        assert!(!should_retry_with_manual_accessibility(
            "Visual Studio Code",
            Some(SelectedTextRange {
                location: 0,
                length: 1
            }),
            &no_text,
        ));
        assert!(!should_retry_with_manual_accessibility(
            "TextEdit",
            Some(SelectedTextRange {
                location: 0,
                length: 1
            }),
            &fallback,
        ));
    }

    #[test]
    fn chromium_accessibility_host_detection_is_case_insensitive() {
        assert!(is_likely_chromium_accessibility_host("Google Chrome"));
        assert!(is_likely_chromium_accessibility_host("Obsidian"));
        assert!(!is_likely_chromium_accessibility_host("TextEdit"));
    }

    #[test]
    fn ax_range_zero_maps_to_no_text_selected() {
        let error = classify_ax_selection(
            Some(SelectedTextRange {
                location: 0,
                length: 0,
            }),
            Some("".into()),
        )
        .unwrap_err();

        assert_eq!(error, GetTextError::NoTextSelected);
    }

    #[test]
    fn ax_non_empty_range_with_empty_text_requests_fallback() {
        let error = classify_ax_selection(
            Some(SelectedTextRange {
                location: 0,
                length: 3,
            }),
            Some("".into()),
        )
        .unwrap_err();

        assert!(matches!(error, GetTextError::FallbackFailed(_)));
    }

    #[test]
    fn ax_text_still_works_when_range_is_unavailable() {
        let result = classify_ax_selection(None, Some("hello".into())).unwrap();

        assert_eq!(result.text, "hello");
    }

    #[test]
    fn cg_rect_to_screen_rect_accepts_positive_rectangles() {
        let rect = CGRect::new(&CGPoint::new(10.0, 20.0), &CGSize::new(30.0, 40.0));

        assert_eq!(
            cg_rect_to_screen_rect(rect),
            Some(ScreenRect {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 40.0,
            })
        );
    }

    #[test]
    fn cg_rect_to_screen_rect_rejects_invalid_rectangles() {
        let empty = CGRect::new(&CGPoint::new(10.0, 20.0), &CGSize::new(0.0, 40.0));
        let infinite = CGRect::new(&CGPoint::new(f64::INFINITY, 20.0), &CGSize::new(30.0, 40.0));

        assert_eq!(cg_rect_to_screen_rect(empty), None);
        assert_eq!(cg_rect_to_screen_rect(infinite), None);
    }
}
