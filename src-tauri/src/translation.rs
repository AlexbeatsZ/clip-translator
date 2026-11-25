use std::{sync::OnceLock, time::Duration};

use html_escape::decode_html_entities;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

const KEYCHAIN_SERVICE: &str = "ClipLingo";
const MISSING_KEY_PREFIX: &str = "尚未配置";

fn http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(25))
            .build()
            .unwrap_or_else(|_| Client::new())
    })
}

/// True when a translation error means the provider has no stored credential.
pub fn is_missing_key_error(error: &str) -> bool {
    error.starts_with(MISSING_KEY_PREFIX)
}

pub fn save_api_key(provider: &str, api_key: &str) -> Result<(), String> {
    let provider = normalize_provider(provider)?;
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, provider)
        .map_err(|error| format!("无法访问系统凭据库：{error}"))?;
    entry
        .set_password(api_key.trim())
        .map_err(|error| format!("保存 API Key 失败：{error}"))
}

pub fn has_api_key(provider: &str) -> Result<bool, String> {
    let provider = normalize_provider(provider)?;
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, provider)
        .map_err(|error| format!("无法访问系统凭据库：{error}"))?;
    match entry.get_password() {
        Ok(value) => Ok(!value.is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(format!("读取系统凭据库失败：{error}")),
    }
}

pub fn delete_api_key(provider: &str) -> Result<(), String> {
    let provider = normalize_provider(provider)?;
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, provider)
        .map_err(|error| format!("无法访问系统凭据库：{error}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("删除 API Key 失败：{error}")),
    }
}

pub async fn translate(
    provider: &str,
    text: &str,
    source_language: &str,
    target_language: &str,
) -> Result<String, String> {
    translate_many(
        provider,
        &[text.to_owned()],
        source_language,
        target_language,
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| "翻译服务没有返回译文".into())
}

pub async fn translate_many(
    provider: &str,
    texts: &[String],
    source_language: &str,
    target_language: &str,
) -> Result<Vec<String>, String> {
    let provider = normalize_provider(provider)?;
    if texts.is_empty() || texts.iter().all(|text| text.trim().is_empty()) {
        return Err("没有可翻译的文字".into());
    }
    let total_chars: usize = texts.iter().map(|text| text.chars().count()).sum();
    if total_chars > 25_000 {
        return Err("单次翻译内容超过 25,000 个字符，请缩短文本".into());
    }

    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, provider)
        .map_err(|error| format!("无法访问系统凭据库：{error}"))?;
    let api_key = entry
        .get_password()
        .map_err(|_| format!("{MISSING_KEY_PREFIX} {} API Key", provider_label(provider)))?;

    match provider {
        "deepl" => translate_deepl_many(&api_key, texts, source_language, target_language).await,
        "google" => translate_google_many(&api_key, texts, source_language, target_language).await,
        _ => unreachable!(),
    }
}

fn normalize_provider(provider: &str) -> Result<&'static str, String> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "deepl" => Ok("deepl"),
        "google" | "google-cloud" => Ok("google"),
        _ => Err(format!("不支持的翻译服务：{provider}")),
    }
}

pub fn provider_label(provider: &str) -> &'static str {
    match provider {
        "deepl" => "DeepL",
        "google" => "Google Cloud Translation",
        _ => "翻译服务",
    }
}

fn language_code(provider: &str, language: &str, target: bool) -> Option<String> {
    let language = language.trim().to_ascii_lowercase();
    if language.is_empty() || language == "auto" {
        return None;
    }
    if provider == "google" {
        return Some(match language.as_str() {
            "zh" => "zh-CN".into(),
            "zh-tw" => "zh-TW".into(),
            _ => language,
        });
    }
    let code = match language.as_str() {
        // DeepL distinguishes script variants only for targets; sources are plain "ZH".
        "zh" if target => "ZH-HANS",
        "zh-tw" if target => "ZH-HANT",
        "zh" | "zh-tw" => "ZH",
        "en" if target => "EN-US",
        "pt" if target => "PT-BR",
        "en" => "EN",
        "ja" => "JA",
        "ko" => "KO",
        "fr" => "FR",
        "de" => "DE",
        "es" => "ES",
        "it" => "IT",
        "pt" => "PT",
        "ru" => "RU",
        "ar" => "AR",
        "nl" => "NL",
        "pl" => "PL",
        "sv" => "SV",
        "tr" => "TR",
        "uk" => "UK",
        _ => return Some(language.to_uppercase()),
    };
    Some(code.into())
}

#[derive(Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Deserialize)]
struct DeepLTranslation {
    text: String,
}

async fn translate_deepl_many(
    api_key: &str,
    texts: &[String],
    source_language: &str,
    target_language: &str,
) -> Result<Vec<String>, String> {
    let endpoint = if api_key.trim_end().ends_with(":fx") {
        "https://api-free.deepl.com/v2/translate"
    } else {
        "https://api.deepl.com/v2/translate"
    };
    let target = language_code("deepl", target_language, true).unwrap_or_else(|| "ZH-HANS".into());
    let mut form = vec![("target_lang".to_owned(), target)];
    if let Some(source) = language_code("deepl", source_language, false) {
        form.push(("source_lang".to_owned(), source));
    }
    for text in texts {
        form.push(("text".to_owned(), text.clone()));
    }

    let response = http_client()
        .post(endpoint)
        .header(
            "Authorization",
            format!("DeepL-Auth-Key {}", api_key.trim()),
        )
        .form(&form)
        .send()
        .await
        .map_err(|error| format!("连接 DeepL 失败：{error}"))?;
    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            403 => "DeepL 拒绝了请求：API Key 无效或与 Free/Pro 端点不匹配".into(),
            456 => "DeepL 本月字符额度已用完".into(),
            429 => "DeepL 请求过于频繁，请稍后再试".into(),
            _ => format!("DeepL 返回 HTTP {}：{}", status, detail.trim()),
        });
    }
    let body: DeepLResponse = response
        .json()
        .await
        .map_err(|error| format!("解析 DeepL 响应失败：{error}"))?;
    if body.translations.len() != texts.len() {
        return Err("DeepL 返回的译文数量与原文不一致".into());
    }
    Ok(body
        .translations
        .into_iter()
        .map(|item| item.text)
        .collect())
}

#[derive(Deserialize)]
struct GoogleResponse {
    data: GoogleData,
}

#[derive(Deserialize)]
struct GoogleData {
    translations: Vec<GoogleTranslation>,
}

#[derive(Deserialize)]
struct GoogleTranslation {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

async fn translate_google_many(
    api_key: &str,
    texts: &[String],
    source_language: &str,
    target_language: &str,
) -> Result<Vec<String>, String> {
    let target = language_code("google", target_language, true).unwrap_or_else(|| "zh-CN".into());
    let mut body = json!({ "q": texts, "target": target, "format": "text" });
    if let Some(source) = language_code("google", source_language, false) {
        body["source"] = json!(source.to_ascii_lowercase());
    }

    let response = http_client()
        .post("https://translation.googleapis.com/language/translate/v2")
        .query(&[("key", api_key.trim())])
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("连接 Google Cloud Translation 失败：{error}"))?;
    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            400 if detail.contains("API key not valid") => {
                "Google Cloud 拒绝了请求：API Key 无效".into()
            }
            403 => "Google Cloud 拒绝了请求：请确认已启用 Cloud Translation API 并开通计费".into(),
            429 => "Google Cloud 请求过于频繁或超出配额".into(),
            _ => format!(
                "Google Cloud Translation 返回 HTTP {}：{}",
                status,
                detail.trim()
            ),
        });
    }
    let body: GoogleResponse = response
        .json()
        .await
        .map_err(|error| format!("解析 Google 响应失败：{error}"))?;
    if body.data.translations.len() != texts.len() {
        return Err("Google Cloud Translation 返回的译文数量与原文不一致".into());
    }
    Ok(body
        .data
        .translations
        .into_iter()
        .map(|item| decode_html_entities(&item.translated_text).to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{is_missing_key_error, language_code};

    #[test]
    fn deepl_uses_script_variants_only_for_targets() {
        assert_eq!(
            language_code("deepl", "zh", true).as_deref(),
            Some("ZH-HANS")
        );
        assert_eq!(
            language_code("deepl", "zh-TW", true).as_deref(),
            Some("ZH-HANT")
        );
        assert_eq!(language_code("deepl", "zh", false).as_deref(), Some("ZH"));
        assert_eq!(language_code("deepl", "en", true).as_deref(), Some("EN-US"));
        assert_eq!(language_code("deepl", "en", false).as_deref(), Some("EN"));
        assert_eq!(language_code("deepl", "auto", false), None);
    }

    #[test]
    fn google_uses_bcp47_chinese_codes() {
        assert_eq!(
            language_code("google", "zh", true).as_deref(),
            Some("zh-CN")
        );
        assert_eq!(
            language_code("google", "zh-TW", true).as_deref(),
            Some("zh-TW")
        );
        assert_eq!(language_code("google", "ja", true).as_deref(), Some("ja"));
    }

    #[test]
    fn detects_missing_key_errors() {
        assert!(is_missing_key_error("尚未配置 DeepL API Key"));
        assert!(!is_missing_key_error("DeepL 返回 HTTP 500"));
    }
}
