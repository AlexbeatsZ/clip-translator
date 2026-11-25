use std::io::Cursor;

use arboard::Clipboard;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat, RgbaImage};
use on_selected_text::{
    GetTextError, GetTextOptions, PermissionStatus, check_permissions,
    get_selected_text_with_options,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputPayload {
    pub kind: String,
    pub source: String,
    pub text: Option<String>,
    pub image_data_url: Option<String>,
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub bounds: Option<SelectionBounds>,
}

pub fn capture_current_input() -> Result<InputPayload, String> {
    match get_selected_text_with_options(GetTextOptions::default().with_bounds()) {
        Ok(selection) if !selection.text.trim().is_empty() => {
            let bounds = selection.bounds.map(|rect| SelectionBounds {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            });
            return Ok(InputPayload {
                kind: "text".into(),
                source: "selection".into(),
                text: Some(selection.text),
                image_data_url: None,
                width: None,
                height: None,
                bounds,
            });
        }
        Ok(_) | Err(GetTextError::NoTextSelected) => {}
        Err(_) => {
            // Selection access can be unavailable in some applications. The clipboard path
            // remains useful and must still be attempted; the selection error is not fatal.
        }
    }

    let mut clipboard = Clipboard::new().map_err(|error| format!("无法读取系统剪贴板：{error}"))?;

    // Screenshot workflows should prefer image data when a clipboard item exposes both
    // image and text representations. Selection always has higher priority above.
    if let Ok(image) = clipboard.get_image() {
        if image.width == 0 || image.height == 0 {
            return Err("剪贴板图片尺寸无效".into());
        }
        if image.width.saturating_mul(image.height) > 50_000_000 {
            return Err("剪贴板图片超过 5,000 万像素，暂不处理".into());
        }
        let rgba = RgbaImage::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.into_owned(),
        )
        .ok_or_else(|| "无法解析剪贴板图片像素".to_string())?;
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(rgba)
            .write_to(&mut png, ImageFormat::Png)
            .map_err(|error| format!("编码剪贴板图片失败：{error}"))?;
        let data_url = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png.into_inner())
        );
        return Ok(InputPayload {
            kind: "image".into(),
            source: "clipboard".into(),
            text: None,
            image_data_url: Some(data_url),
            width: Some(image.width),
            height: Some(image.height),
            bounds: None,
        });
    }

    if let Ok(text) = clipboard.get_text() {
        if !text.trim().is_empty() {
            return Ok(InputPayload {
                kind: "text".into(),
                source: "clipboard".into(),
                text: Some(text),
                image_data_url: None,
                width: None,
                height: None,
                bounds: None,
            });
        }
    }

    Err("没有检测到文字选区，也没有可翻译的剪贴板文字或图片".into())
}

pub fn selection_permission_status() -> serde_json::Value {
    match check_permissions() {
        PermissionStatus::Granted => serde_json::json!({ "status": "granted" }),
        PermissionStatus::NotRequired => serde_json::json!({ "status": "not-required" }),
        PermissionStatus::Unsupported => serde_json::json!({ "status": "unsupported" }),
        PermissionStatus::Denied {
            platform,
            permission,
            guidance,
        } => serde_json::json!({
            "status": "denied",
            "platform": platform,
            "permission": permission,
            "guidance": guidance,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::SelectionBounds;

    #[test]
    fn selection_bounds_keep_fractional_screen_coordinates() {
        let bounds = SelectionBounds {
            x: 12.5,
            y: 18.25,
            width: 80.0,
            height: 19.5,
        };
        assert_eq!(bounds.x, 12.5);
        assert_eq!(bounds.height, 19.5);
    }
}
