use std::{collections::VecDeque, ffi::OsString, io::Read, time::Duration};

use crate::{
    provider::{run_provider_chain, TextProvider},
    GetTextError, GetTextOptions, ScreenRect, SelectionResult,
};

use super::clipboard_guard::{create_enigo, ClipboardTransaction};

const ATSPI_MAX_VISITED_NODES: usize = 256;

/// Linux AT-SPI2 文本直读 provider。
///
/// AT-SPI2 通过 D-Bus 读取当前焦点文本对象的 selection，不需要模拟按键，
/// 也不会触碰 PRIMARY/CLIPBOARD。若当前应用未暴露可用文本接口，则交给后续 provider。
struct LinuxAccessibilityProvider;

impl TextProvider for LinuxAccessibilityProvider {
    fn name(&self) -> &'static str {
        "linux.atspi2"
    }

    fn try_get_text(&self, opts: &GetTextOptions) -> Result<SelectionResult, GetTextError> {
        read_atspi_selection(opts)
    }
}

/// Linux PRIMARY selection provider。
///
/// PRIMARY selection 是 Linux 桌面的“划选即拥有”选区，不需要模拟 Ctrl+C，
/// 因此它不受 `allow_clipboard_fallback` 限制。
struct LinuxPrimaryProvider;

impl TextProvider for LinuxPrimaryProvider {
    fn name(&self) -> &'static str {
        "linux.primary"
    }

    fn try_get_text(&self, _opts: &GetTextOptions) -> Result<SelectionResult, GetTextError> {
        let text = read_linux_primary_selection(Duration::from_millis(50))?;
        Ok(SelectionResult::new(text))
    }
}

/// Linux 当前实现的剪贴板降级 provider。
struct LinuxClipboardProvider;

impl TextProvider for LinuxClipboardProvider {
    fn name(&self) -> &'static str {
        "linux.clipboard"
    }

    fn try_get_text(&self, opts: &GetTextOptions) -> Result<SelectionResult, GetTextError> {
        if !opts.allow_clipboard_fallback {
            return Err(GetTextError::FallbackDisabled);
        }

        let mut enigo = create_enigo()?;
        let transaction = ClipboardTransaction::begin()?;
        let text = transaction.simulate_copy_and_read(&mut enigo, opts.max_wait_ms, false)?;

        Ok(SelectionResult::new(text))
    }
}

/// Linux 平台入口。
pub(crate) fn get_selected_text_with_options(
    opts: &GetTextOptions,
) -> Result<SelectionResult, GetTextError> {
    let accessibility = LinuxAccessibilityProvider;
    let primary = LinuxPrimaryProvider;
    let clipboard = LinuxClipboardProvider;
    run_provider_chain(
        [
            &accessibility as &dyn TextProvider,
            &primary as &dyn TextProvider,
            &clipboard as &dyn TextProvider,
        ],
        opts,
    )
}

/// 通过 AT-SPI2 读取当前焦点文本对象的选区。
fn read_atspi_selection(opts: &GetTextOptions) -> Result<SelectionResult, GetTextError> {
    let result = async_io::block_on(read_atspi_selection_async(
        ATSPI_MAX_VISITED_NODES,
        opts.fetch_bounds,
    ));

    // AT-SPI2 在不同 toolkit 中覆盖率差异较大。这里把“没有可读选区”视为
    // provider 不适用，让 PRIMARY selection 继续兜底并决定是否真的没有选区。
    result.map_err(|error| match error {
        GetTextError::NoTextSelected => {
            GetTextError::fallback_failed("AT-SPI2 focused text selection is empty")
        }
        other => other,
    })
}

/// 异步遍历 AT-SPI2 树，找到当前 focused 且实现 Text 接口的对象。
async fn read_atspi_selection_async(
    max_nodes: usize,
    fetch_bounds: bool,
) -> Result<SelectionResult, GetTextError> {
    use atspi::{
        proxy::{accessible::ObjectRefExt, proxy_ext::ProxyExt},
        AccessibilityConnection, Interface, State,
    };

    let atspi = AccessibilityConnection::new()
        .await
        .map_err(|error| GetTextError::fallback_failed(format!("connect AT-SPI2: {error}")))?;
    let conn = atspi.connection();
    let root = atspi.root_accessible_on_registry().await.map_err(|error| {
        GetTextError::fallback_failed(format!("open AT-SPI2 registry: {error}"))
    })?;
    let mut queue = VecDeque::from(root.get_children().await.map_err(|error| {
        GetTextError::fallback_failed(format!("read AT-SPI2 applications: {error}"))
    })?);
    let mut visited = 0usize;

    while let Some(object_ref) = queue.pop_front() {
        if visited >= max_nodes {
            break;
        }
        visited += 1;

        let Ok(accessible) = object_ref.into_accessible_proxy(conn).await else {
            continue;
        };
        let Ok(states) = accessible.get_state().await else {
            continue;
        };
        let Ok(interfaces) = accessible.get_interfaces().await else {
            continue;
        };

        if states.contains(State::Focused) && interfaces.contains(Interface::Text) {
            let proxies = accessible.proxies().await.map_err(|error| {
                GetTextError::fallback_failed(format!("open AT-SPI2 focused proxies: {error}"))
            })?;
            let text = proxies.text().await.map_err(|error| {
                GetTextError::fallback_failed(format!("open AT-SPI2 focused text proxy: {error}"))
            })?;
            return read_atspi_text_selection(&text, fetch_bounds).await;
        }

        if let Ok(children) = accessible.get_children().await {
            queue.extend(children);
        }
    }

    Err(GetTextError::fallback_failed(
        "AT-SPI2 focused text object not found",
    ))
}

/// 从 AT-SPI2 Text proxy 中读取第一段选区。
async fn read_atspi_text_selection(
    text: &atspi::proxy::text::TextProxy<'_>,
    fetch_bounds: bool,
) -> Result<SelectionResult, GetTextError> {
    let selection_count = text.get_n_selections().await.map_err(|error| {
        GetTextError::fallback_failed(format!("read AT-SPI2 selection count: {error}"))
    })?;
    if selection_count <= 0 {
        return Err(GetTextError::NoTextSelected);
    }

    let (start, end) = text.get_selection(0).await.map_err(|error| {
        GetTextError::fallback_failed(format!("read AT-SPI2 selection: {error}"))
    })?;
    let (start, end) = normalize_atspi_selection_range(start, end)?;
    let selected_text = text
        .get_text(start, end)
        .await
        .map_err(|error| GetTextError::fallback_failed(format!("read AT-SPI2 text: {error}")))?;

    let text_value = normalize_atspi_selected_text(selected_text)?;
    let mut selection = SelectionResult::new(text_value);
    if fetch_bounds {
        if let Some(bounds) = read_atspi_text_bounds(text, start, end).await {
            selection = selection.with_bounds(bounds);
        }
    }

    Ok(selection)
}

/// 读取 AT-SPI2 选区的屏幕坐标；坐标失败不影响文本结果。
async fn read_atspi_text_bounds(
    text: &atspi::proxy::text::TextProxy<'_>,
    start: i32,
    end: i32,
) -> Option<ScreenRect> {
    let extents = text
        .get_range_extents(start, end, atspi::CoordType::Screen)
        .await
        .ok()?;
    atspi_extents_to_bounds(extents)
}

/// 将 AT-SPI2 `(x, y, width, height)` 坐标转换为公开屏幕矩形。
fn atspi_extents_to_bounds((x, y, width, height): (i32, i32, i32, i32)) -> Option<ScreenRect> {
    if width <= 0 || height <= 0 {
        return None;
    }

    Some(ScreenRect {
        x: f64::from(x),
        y: f64::from(y),
        width: f64::from(width),
        height: f64::from(height),
    })
}

/// 读取 Linux 主选区，优先使用当前会话原生后端，避免普通剪贴板污染。
fn read_linux_primary_selection(timeout: Duration) -> Result<String, GetTextError> {
    let mut failures = Vec::new();
    let mut primary_was_empty = false;

    if env_value_present(std::env::var_os("WAYLAND_DISPLAY")) {
        match read_wayland_primary_selection() {
            Ok(text) => return Ok(text),
            Err(GetTextError::NoTextSelected) => primary_was_empty = true,
            Err(error) => failures.push(format!("Wayland PRIMARY: {error}")),
        }
    }

    if env_value_present(std::env::var_os("DISPLAY")) {
        match read_x11_primary_selection(timeout) {
            Ok(text) => return Ok(text),
            Err(GetTextError::NoTextSelected) => primary_was_empty = true,
            Err(error) => failures.push(format!("X11 PRIMARY: {error}")),
        }
    }

    if primary_was_empty {
        Err(GetTextError::NoTextSelected)
    } else if failures.is_empty() {
        Err(GetTextError::fallback_failed(
            "no Linux primary selection backend is available",
        ))
    } else {
        Err(GetTextError::fallback_failed(failures.join("; ")))
    }
}

/// 规范化 AT-SPI2 返回的 selection range。
fn normalize_atspi_selection_range(start: i32, end: i32) -> Result<(i32, i32), GetTextError> {
    if start < 0 || end < 0 || start == end {
        return Err(GetTextError::NoTextSelected);
    }

    Ok(if start < end {
        (start, end)
    } else {
        (end, start)
    })
}

/// 规范化 AT-SPI2 返回的选中文本。
fn normalize_atspi_selected_text(text: String) -> Result<String, GetTextError> {
    let text = text.trim_matches('\0').to_string();
    if text.is_empty() {
        Err(GetTextError::NoTextSelected)
    } else {
        Ok(text)
    }
}

/// 读取 Wayland primary selection。
fn read_wayland_primary_selection() -> Result<String, GetTextError> {
    use wl_clipboard_rs::paste::{get_contents, ClipboardType, MimeType, Seat};

    let (mut reader, _mime_type) =
        get_contents(ClipboardType::Primary, Seat::Unspecified, MimeType::Text)
            .map_err(map_wayland_primary_error)?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| GetTextError::fallback_failed(format!("read Wayland PRIMARY: {error}")))?;

    normalize_primary_text(bytes)
}

/// 读取 X11 PRIMARY selection。
fn read_x11_primary_selection(timeout: Duration) -> Result<String, GetTextError> {
    let clipboard = x11_clipboard::Clipboard::new()
        .map_err(|error| GetTextError::fallback_failed(format!("open X11 clipboard: {error}")))?;
    let bytes = clipboard
        .load(
            clipboard.getter.atoms.primary,
            clipboard.getter.atoms.utf8_string,
            clipboard.getter.atoms.property,
            timeout,
        )
        .map_err(|error| GetTextError::fallback_failed(format!("read X11 PRIMARY: {error}")))?;

    normalize_primary_text(bytes)
}

/// 将 Linux primary selection 返回的 UTF-8 字节规范化为 Rust 字符串。
fn normalize_primary_text(bytes: Vec<u8>) -> Result<String, GetTextError> {
    if bytes.is_empty() {
        return Err(GetTextError::NoTextSelected);
    }

    let text = String::from_utf8(bytes)
        .map_err(|error| {
            GetTextError::fallback_failed(format!("Linux PRIMARY utf8 decode: {error}"))
        })?
        .trim_matches('\0')
        .to_string();

    if text.is_empty() {
        Err(GetTextError::NoTextSelected)
    } else {
        Ok(text)
    }
}

/// 判断环境变量是否存在且非空。
fn env_value_present(value: Option<OsString>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// 将 Wayland primary selection 错误映射到 provider 链可理解的语义。
fn map_wayland_primary_error(error: wl_clipboard_rs::paste::Error) -> GetTextError {
    use wl_clipboard_rs::paste::Error;

    match error {
        Error::ClipboardEmpty | Error::NoMimeType => GetTextError::NoTextSelected,
        other => GetTextError::fallback_failed(format!("read Wayland PRIMARY: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        atspi_extents_to_bounds, env_value_present, map_wayland_primary_error,
        normalize_atspi_selected_text, normalize_atspi_selection_range, normalize_primary_text,
    };
    use crate::{GetTextError, ScreenRect};
    use std::ffi::OsString;
    use wl_clipboard_rs::paste::Error as WaylandPasteError;

    #[test]
    fn normalize_primary_text_strips_nul_padding_only() {
        let text = normalize_primary_text(b"  hello  \0\0".to_vec()).unwrap();

        assert_eq!(text, "  hello  ");
    }

    #[test]
    fn normalize_primary_text_reports_empty_selection() {
        let error = normalize_primary_text(Vec::new()).unwrap_err();

        assert_eq!(error, GetTextError::NoTextSelected);
    }

    #[test]
    fn normalize_atspi_selection_range_accepts_reversed_offsets() {
        let range = normalize_atspi_selection_range(9, 2).unwrap();

        assert_eq!(range, (2, 9));
    }

    #[test]
    fn normalize_atspi_selection_range_rejects_empty_or_invalid_offsets() {
        assert_eq!(
            normalize_atspi_selection_range(4, 4).unwrap_err(),
            GetTextError::NoTextSelected
        );
        assert_eq!(
            normalize_atspi_selection_range(-1, 4).unwrap_err(),
            GetTextError::NoTextSelected
        );
    }

    #[test]
    fn normalize_atspi_selected_text_strips_nul_padding_only() {
        let text = normalize_atspi_selected_text("  hello  \0".to_string()).unwrap();

        assert_eq!(text, "  hello  ");
    }

    #[test]
    fn atspi_extents_to_bounds_accepts_screen_rectangles() {
        let bounds = atspi_extents_to_bounds((-10, 20, 30, 40));

        assert_eq!(
            bounds,
            Some(ScreenRect {
                x: -10.0,
                y: 20.0,
                width: 30.0,
                height: 40.0,
            })
        );
    }

    #[test]
    fn atspi_extents_to_bounds_rejects_empty_rectangles() {
        assert_eq!(atspi_extents_to_bounds((1, 2, 0, 4)), None);
        assert_eq!(atspi_extents_to_bounds((1, 2, 3, -4)), None);
    }

    #[test]
    fn env_value_present_requires_non_empty_value() {
        assert!(!env_value_present(None));
        assert!(!env_value_present(Some(OsString::new())));
        assert!(env_value_present(Some(OsString::from("wayland-0"))));
    }

    #[test]
    fn wayland_empty_primary_maps_to_no_text_selected() {
        let error = map_wayland_primary_error(WaylandPasteError::ClipboardEmpty);

        assert_eq!(error, GetTextError::NoTextSelected);
    }

    #[test]
    fn wayland_unsupported_primary_allows_next_provider() {
        let error = map_wayland_primary_error(WaylandPasteError::PrimarySelectionUnsupported);

        assert!(matches!(error, GetTextError::FallbackFailed { .. }));
    }
}
