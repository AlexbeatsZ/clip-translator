use std::{fs, path::PathBuf};

use ab_glyph::{FontArc, FontVec, PxScale};
use image::{GrayImage, Luma, Rgb, RgbImage};
use imageproc::{
    distance_transform::Norm,
    drawing::{draw_filled_rect_mut, draw_text_mut, text_size},
    morphology::dilate,
    rect::Rect,
};
use oximedia_restoration::inpaint::{InpaintConfig, inpaint_patchmatch};
use rapidocr_core::types::OcrLine;

#[derive(Debug, Clone)]
pub struct TextBlock {
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Number of OCR lines merged into this block; used to estimate the original font size.
    pub line_count: u32,
}

pub fn group_ocr_lines(lines: &[OcrLine], image_width: u32, image_height: u32) -> Vec<TextBlock> {
    let mut items: Vec<TextBlock> = lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .filter_map(|line| {
            let min_x = line
                .bbox
                .points
                .iter()
                .map(|point| point[0])
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.0) as u32;
            let min_y = line
                .bbox
                .points
                .iter()
                .map(|point| point[1])
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.0) as u32;
            let max_x = line
                .bbox
                .points
                .iter()
                .map(|point| point[0])
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil()
                .max(0.0) as u32;
            let max_y = line
                .bbox
                .points
                .iter()
                .map(|point| point[1])
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil()
                .max(0.0) as u32;
            let x = min_x.min(image_width.saturating_sub(1));
            let y = min_y.min(image_height.saturating_sub(1));
            let right = max_x.min(image_width).max(x + 1);
            let bottom = max_y.min(image_height).max(y + 1);
            let width = right.saturating_sub(x);
            let height = bottom.saturating_sub(y);
            if width == 0 || height == 0 {
                return None;
            }
            Some(TextBlock {
                text: line.text.trim().to_owned(),
                x,
                y,
                width,
                height,
                line_count: 1,
            })
        })
        .collect();

    items.sort_by_key(|item| (item.y, item.x));
    let mut groups: Vec<TextBlock> = Vec::new();
    for item in items {
        if let Some(last) = groups.last_mut() {
            let last_bottom = last.y + last.height;
            let vertical_gap = item.y.saturating_sub(last_bottom);
            let max_gap = (last.height as f32 * 0.9).max(7.0) as u32;
            let left_delta = item.x.abs_diff(last.x);
            let max_left_delta = (last.height as f32 * 1.8).max(18.0) as u32;
            if vertical_gap <= max_gap && left_delta <= max_left_delta {
                last.text.push('\n');
                last.line_count += 1;
                last.text.push_str(&item.text);
                let right = (last.x + last.width).max(item.x + item.width);
                let bottom = (last.y + last.height).max(item.y + item.height);
                last.x = last.x.min(item.x);
                last.y = last.y.min(item.y);
                last.width = right.saturating_sub(last.x);
                last.height = bottom.saturating_sub(last.y);
                continue;
            }
        }
        groups.push(item);
    }
    groups
}

pub fn compose_translated_image(
    original: &RgbImage,
    blocks: &[TextBlock],
    translations: &[String],
) -> Result<RgbImage, String> {
    if blocks.is_empty() || blocks.len() != translations.len() {
        return Err("OCR 文本块与译文数量不一致".into());
    }

    let (width, height) = original.dimensions();
    if width == 0 || height == 0 {
        return Err("图片尺寸无效".into());
    }

    // Erase the source text. Screenshots mostly have flat backgrounds behind text: when the
    // ring around a block is uniform, fill it with that colour (instant, artefact-free) and
    // only send textured regions to the much slower PatchMatch inpainting.
    let mut restored = original.clone();
    let mut mask = GrayImage::from_pixel(width, height, Luma([0]));
    let mut needs_inpaint = false;
    let mut styles = Vec::with_capacity(blocks.len());
    for block in blocks {
        let line_height = block.height as f32 / block.line_count.max(1) as f32;
        let pad = (line_height * 0.14).round().max(2.0) as u32;
        let x = block.x.saturating_sub(pad);
        let y = block.y.saturating_sub(pad);
        let right = (block.x + block.width + pad).min(width);
        let bottom = (block.y + block.height + pad).min(height);
        if right <= x || bottom <= y {
            styles.push((Rgb([128, 128, 128]), Rgb([20, 20, 20])));
            continue;
        }
        let (background, spread) = ring_statistics(original, x, y, right, bottom);
        let text_color = estimate_text_color(original, block, background);
        styles.push((background, text_color));
        let rect = Rect::at(x as i32, y as i32).of_size(right - x, bottom - y);
        if spread < 14.0 {
            draw_filled_rect_mut(&mut restored, rect, background);
        } else {
            draw_filled_rect_mut(&mut mask, rect, Luma([255]));
            needs_inpaint = true;
        }
    }

    if needs_inpaint {
        let mask = dilate(&mask, Norm::LInf, 2);
        let mask_values: Vec<bool> = mask.pixels().map(|pixel| pixel[0] > 0).collect();
        let masked_pixels = mask_values.iter().filter(|&&value| value).count();
        if masked_pixels > (width as usize * height as usize * 7 / 10) {
            return Err("识别区域覆盖了图片的大部分内容，无法可靠修复背景".into());
        }
        let restored_pixels = inpaint_patchmatch(
            restored.as_raw(),
            &mask_values,
            width,
            height,
            &InpaintConfig {
                patch_size: 3,
                iterations: 3,
                blend_radius: 1,
            },
        );
        restored = RgbImage::from_raw(width, height, restored_pixels)
            .ok_or_else(|| "图片背景修复结果尺寸无效".to_string())?;
    }

    let font = load_cjk_font()?;
    for ((block, translated), (background, text_color)) in
        blocks.iter().zip(translations).zip(styles)
    {
        let color = if contrast(text_color, background) < 2.2 {
            if luminance(background) > 140.0 {
                Rgb([28, 34, 43])
            } else {
                Rgb([248, 249, 247])
            }
        } else {
            text_color
        };
        draw_fitted_text(&mut restored, block, translated, &font, color);
    }
    Ok(restored)
}

fn luminance(pixel: Rgb<u8>) -> f32 {
    0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32
}

fn contrast(a: Rgb<u8>, b: Rgb<u8>) -> f32 {
    let (la, lb) = (luminance(a) + 12.0, luminance(b) + 12.0);
    la.max(lb) / la.min(lb)
}

/// Mean colour and luminance standard deviation of a 2px ring just outside the rectangle.
fn ring_statistics(image: &RgbImage, x0: u32, y0: u32, x1: u32, y1: u32) -> (Rgb<u8>, f32) {
    let (width, height) = image.dimensions();
    let outer_x0 = x0.saturating_sub(2);
    let outer_y0 = y0.saturating_sub(2);
    let outer_x1 = (x1 + 2).min(width);
    let outer_y1 = (y1 + 2).min(height);
    let mut sum = [0f64; 3];
    let mut lum_sum = 0f64;
    let mut lum_sq = 0f64;
    let mut count = 0f64;
    for y in outer_y0..outer_y1 {
        for x in outer_x0..outer_x1 {
            if x >= x0 && x < x1 && y >= y0 && y < y1 {
                continue;
            }
            let pixel = image.get_pixel(x, y);
            for channel in 0..3 {
                sum[channel] += pixel[channel] as f64;
            }
            let lum = luminance(*pixel) as f64;
            lum_sum += lum;
            lum_sq += lum * lum;
            count += 1.0;
        }
    }
    if count == 0.0 {
        return (*image.get_pixel(x0.min(width - 1), y0.min(height - 1)), 0.0);
    }
    let mean = Rgb([
        (sum[0] / count).round() as u8,
        (sum[1] / count).round() as u8,
        (sum[2] / count).round() as u8,
    ]);
    let variance = (lum_sq / count - (lum_sum / count).powi(2)).max(0.0);
    (mean, variance.sqrt() as f32)
}

/// Average colour of the pixels inside the block that differ most from the background,
/// i.e. the glyph strokes, so translated text keeps the original text colour.
fn estimate_text_color(image: &RgbImage, block: &TextBlock, background: Rgb<u8>) -> Rgb<u8> {
    let distance = |pixel: &Rgb<u8>| -> u32 {
        (0..3)
            .map(|channel| (pixel[channel] as i32 - background[channel] as i32).unsigned_abs())
            .sum()
    };
    let right = (block.x + block.width).min(image.width());
    let bottom = (block.y + block.height).min(image.height());
    let mut max_distance = 0;
    for y in block.y..bottom {
        for x in block.x..right {
            max_distance = max_distance.max(distance(image.get_pixel(x, y)));
        }
    }
    let threshold = max_distance * 6 / 10;
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    for y in block.y..bottom {
        for x in block.x..right {
            let pixel = image.get_pixel(x, y);
            if distance(pixel) >= threshold && max_distance > 40 {
                for channel in 0..3 {
                    sum[channel] += pixel[channel] as u64;
                }
                count += 1;
            }
        }
    }
    if count == 0 {
        return background;
    }
    Rgb([
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
    ])
}

fn load_cjk_font() -> Result<FontArc, String> {
    let candidates = system_font_candidates();
    for path in candidates {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(font) = FontVec::try_from_vec_and_index(bytes, 0) {
                return Ok(FontArc::from(font));
            }
        }
    }
    Err("没有找到可用的中文字体。请安装 Noto Sans CJK、苹方或微软雅黑".into())
}

#[cfg(target_os = "macos")]
fn system_font_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
        PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
        PathBuf::from("/System/Library/Fonts/STHeiti Medium.ttc"),
        PathBuf::from("/System/Library/Fonts/STHeiti Light.ttc"),
        PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
    ]
}

#[cfg(target_os = "windows")]
fn system_font_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Windows\Fonts\msyh.ttc"),
        PathBuf::from(r"C:\Windows\Fonts\simhei.ttf"),
        PathBuf::from(r"C:\Windows\Fonts\simsun.ttc"),
        PathBuf::from(r"C:\Windows\Fonts\arial.ttf"),
    ]
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn system_font_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"),
    ]
}

fn draw_fitted_text(
    image: &mut RgbImage,
    block: &TextBlock,
    text: &str,
    font: &FontArc,
    color: Rgb<u8>,
) {
    let (image_width, image_height) = image.dimensions();
    let lines_in_source = block.line_count.max(1) as f32;
    let source_line_height = block.height as f32 / lines_in_source;
    let x = block.x.min(image_width.saturating_sub(1));
    // Translations may need a little more room than the source; allow a small overflow
    // to the right instead of shrinking the text to an unreadable size.
    let max_width = ((block.width as f32) * 1.08)
        .min((image_width - x) as f32)
        .max(4.0) as u32;
    let max_height = block.height.max(6);

    // OCR boxes hug the glyphs, so the line box height is roughly the font's em size.
    let mut font_size = (source_line_height * 0.92).clamp(8.0, 160.0);
    // Keep the source paragraph's line pitch when it is plausible for this font size.
    let line_height_for =
        |size: f32| (size * 1.22).max(source_line_height.min(size * 1.9)).ceil() as u32;
    let mut lines = wrap_text(text, font, PxScale::from(font_size), max_width);
    loop {
        let total_height = line_height_for(font_size).saturating_mul(lines.len() as u32);
        let too_wide = lines
            .iter()
            .any(|line| text_size(PxScale::from(font_size), font, line).0 > max_width);
        if (!too_wide && total_height <= max_height + (font_size * 0.2) as u32) || font_size <= 8.0
        {
            break;
        }
        font_size = (font_size * 0.94).max(8.0);
        lines = wrap_text(text, font, PxScale::from(font_size), max_width);
    }

    let scale = PxScale::from(font_size);
    let line_height = line_height_for(font_size) as i32;
    let total_height = line_height * lines.len() as i32;
    // Centre vertically inside the original block; glyphs are drawn from the line top.
    let mut draw_y =
        block.y as i32 + ((block.height as i32 - total_height) / 2).max(-(line_height / 4));
    for line in lines {
        if draw_y + line_height > image_height as i32 + line_height / 4 {
            break;
        }
        draw_text_mut(image, color, x as i32, draw_y, scale, font, &line);
        draw_y += line_height;
    }
}

fn wrap_text(text: &str, font: &FontArc, scale: PxScale, max_width: u32) -> Vec<String> {
    let mut output = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for character in paragraph.chars() {
            if character == '\r' {
                continue;
            }
            let mut candidate = line.clone();
            candidate.push(character);
            if !line.is_empty() && text_size(scale, font, &candidate).0 > max_width {
                if let Some(space) = line.rfind(' ') {
                    let carry = line[space + 1..].to_owned();
                    line.truncate(space);
                    if !line.is_empty() {
                        output.push(line);
                    }
                    line = carry;
                    line.push(character);
                } else {
                    output.push(std::mem::take(&mut line));
                    line.push(character);
                }
            } else {
                line.push(character);
            }
        }
        if !line.is_empty() {
            output.push(line);
        }
    }
    if output.is_empty() {
        output.push(String::new());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{TextBlock, compose_translated_image, group_ocr_lines};
    use rapidocr_core::types::{OcrLine, Quad};

    #[test]
    fn groups_adjacent_ocr_lines_into_a_paragraph() {
        let lines = vec![
            OcrLine::new(Quad::from_xyxy(10.0, 10.0, 80.0, 30.0), "First line", 0.99),
            OcrLine::new(Quad::from_xyxy(11.0, 34.0, 90.0, 54.0), "Second line", 0.98),
        ];
        let blocks = group_ocr_lines(&lines, 120, 80);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "First line\nSecond line");
        assert_eq!(blocks[0].height, 44);
    }

    #[test]
    #[ignore = "exercises local inpainting and system CJK font rendering"]
    fn composes_translated_chinese_text_locally() {
        use image::{Rgb, RgbImage};

        let original = RgbImage::from_pixel(420, 180, Rgb([248, 248, 244]));
        let block = TextBlock {
            text: "Original heading".into(),
            x: 28,
            y: 48,
            width: 260,
            height: 42,
            line_count: 1,
        };
        let translated =
            compose_translated_image(&original, &[block], &["本地图片翻译测试".into()])
                .expect("compose translated image");
        assert_eq!(translated.dimensions(), original.dimensions());
        assert_ne!(translated.as_raw(), original.as_raw());
    }

    /// End-to-end local pipeline on a real screenshot: OCR → grouping → inpaint → typeset.
    /// `CLIPLINGO_SAMPLE=in.png CLIPLINGO_OUT=out.png cargo test -- --ignored ocr_and_compose`
    #[test]
    #[ignore = "needs CLIPLINGO_SAMPLE / CLIPLINGO_OUT and the bundled OCR model"]
    fn ocr_and_compose_sample_image() {
        let sample = std::env::var("CLIPLINGO_SAMPLE").expect("CLIPLINGO_SAMPLE");
        let out = std::env::var("CLIPLINGO_OUT").expect("CLIPLINGO_OUT");
        let model_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ocr-models");
        let original = image::open(sample).expect("open sample").to_rgb8();
        let mut engine = crate::ocr::load_engine(&model_dir).expect("load OCR");
        let lines = crate::ocr::recognize(&mut engine, &original).expect("run OCR");
        let blocks = group_ocr_lines(&lines, original.width(), original.height());
        assert!(!blocks.is_empty(), "no text recognized");
        let fake = [
            "季度路线图",
            "提升离线识别速度\n在每块显示器上支持浮窗\n发布 Windows 预览版",
        ];
        let translations: Vec<String> = blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                eprintln!("block {index}: {:?}", block.text);
                fake.get(index)
                    .map(|text| text.to_string())
                    .unwrap_or_else(|| format!("译文 {index}"))
            })
            .collect();
        let started = std::time::Instant::now();
        let composed =
            compose_translated_image(&original, &blocks, &translations).expect("compose");
        eprintln!("compose took {:?}", started.elapsed());
        composed.save(out).expect("save output");
    }

    #[test]
    fn does_not_group_distant_text_blocks() {
        let lines = vec![
            OcrLine::new(Quad::from_xyxy(10.0, 10.0, 80.0, 30.0), "Header", 0.99),
            OcrLine::new(Quad::from_xyxy(10.0, 90.0, 90.0, 110.0), "Footer", 0.98),
        ];
        let blocks = group_ocr_lines(&lines, 120, 140);
        assert_eq!(blocks.len(), 2);
        let _ = TextBlock {
            text: String::new(),
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            line_count: 1,
        };
    }
}
