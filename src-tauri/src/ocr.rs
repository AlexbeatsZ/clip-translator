use std::path::Path;

use image::RgbImage;
use rapidocr_core::{
    RapidOcr,
    config::{PipelineConfig, RapidOcrConfig},
    model::{ModelCache, ModelDownloadMode, PPOCRV6_SMALL},
    types::OcrLine,
};

pub fn load_engine(model_dir: &Path) -> Result<RapidOcr, String> {
    let pipeline = PipelineConfig::without_cls();
    let cache = ModelCache::new(model_dir);
    cache
        .ensure_model_set_for_pipeline(&PPOCRV6_SMALL, pipeline, ModelDownloadMode::Never)
        .map_err(|error| format!("本地 OCR 模型文件不完整或校验失败：{error}"))?;

    let config = RapidOcrConfig::ppocr_v6_small(model_dir).with_pipeline(pipeline);
    RapidOcr::new(config).map_err(|error| format!("初始化本地 OCR 引擎失败：{error}"))
}

pub fn recognize(engine: &mut RapidOcr, image: &RgbImage) -> Result<Vec<OcrLine>, String> {
    let output = engine
        .run_image(image)
        .map_err(|error| format!("本地 OCR 识别失败：{error}"))?;
    Ok(output.lines)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ab_glyph::FontVec;
    use ab_glyph::PxScale;
    use image::{Rgb, RgbImage};
    use imageproc::drawing::draw_text_mut;

    use super::{load_engine, recognize};

    #[test]
    #[ignore = "loads the bundled ONNX model and exercises local OCR"]
    fn local_ppocr_recognizes_sample_english_text() {
        let model_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ocr-models");
        let mut engine = load_engine(&model_dir).expect("load local PP-OCRv6 model");
        let font_bytes = std::fs::read("/System/Library/Fonts/Supplemental/Arial.ttf")
            .expect("read system Arial font");
        let font = FontVec::try_from_vec(font_bytes).expect("parse Arial font");
        let mut image = RgbImage::from_pixel(760, 180, Rgb([255, 255, 255]));
        draw_text_mut(
            &mut image,
            Rgb([15, 20, 30]),
            30,
            52,
            PxScale::from(44.0),
            &font,
            "A quick brown fox jumps over the lazy dog.",
        );
        let lines = recognize(&mut engine, &image).expect("run local OCR");
        let recognized = lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            recognized.to_ascii_lowercase().contains("quick"),
            "OCR output: {recognized}"
        );
    }
}
