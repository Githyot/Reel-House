//! Translation back ends. Each one translates a list of subtitle lines and returns the same number of lines.

use crate::error::{Error, Result};
use crate::settings::SubtitleSettings;
use futures_util::future::BoxFuture;
use serde_json::{json, Value};

pub trait Translate: Send + Sync {
    fn name(&self) -> &str;
    fn translate<'a>(&'a self, texts: &'a [String], source: &'a str, target: &'a str) -> BoxFuture<'a, Result<Vec<String>>>;
}

pub struct LibreTranslate {
    pub http: reqwest::Client,
    pub url: String,
    pub api_key: String,
}

pub struct Claude {
    pub http: reqwest::Client,
    pub api_base: String,
    pub api_key: String,
    pub model: String,
}

impl Translate for LibreTranslate {
    fn name(&self) -> &str {
        "LibreTranslate"
    }

    fn translate<'a>(&'a self, texts: &'a [String], source: &'a str, target: &'a str) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let mut body = json!({"q": texts, "source": source, "target": target, "format": "html"});
            if !self.api_key.is_empty() {
                body["api_key"] = Value::String(self.api_key.clone());
            }
            let res = self.http.post(format!("{}/translate", self.url.trim_end_matches('/'))).json(&body).send().await?;
            if !res.status().is_success() {
                return Err(Error::msg(format!("LibreTranslate answered {}", res.status().as_u16())));
            }
            let v: Value = res.json().await?;
            match &v["translatedText"] {
                Value::Array(a) => Ok(a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()),
                Value::String(s) => Ok(vec![s.clone()]),
                _ => Err(Error::msg("LibreTranslate returned an unexpected reply.")),
            }
        })
    }
}

impl Translate for Claude {
    fn name(&self) -> &str {
        "Claude"
    }

    fn translate<'a>(&'a self, texts: &'a [String], source: &'a str, target: &'a str) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            let from = if source == "auto" { String::new() } else { format!(" from {source}") };
            let system = format!(
                "You translate movie and series subtitle lines{from} into the language with ISO code \"{target}\". \
                 The user message is a JSON array of strings. Reply with ONLY a JSON array of the same length, one translation per entry, in the same order. \
                 Keep line breaks, HTML tags such as <i>, music symbols and speaker dashes. Never merge, split or drop entries."
            );
            let res = self
                .http
                .post(format!("{}/v1/messages", self.api_base.trim_end_matches('/')))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&json!({"model": self.model, "max_tokens": 8000, "system": system,
                              "messages": [{"role": "user", "content": serde_json::to_string(texts)?}]}))
                .send()
                .await?;
            if !res.status().is_success() {
                return Err(Error::msg(format!("The Claude API answered {}", res.status().as_u16())));
            }
            let v: Value = res.json().await?;
            let text: String = v["content"].as_array().map(|a| a.iter().filter_map(|b| b["text"].as_str()).collect()).unwrap_or_default();
            let (a, b) = (text.find('['), text.rfind(']'));
            match (a, b) {
                (Some(a), Some(b)) if b > a => Ok(serde_json::from_str(&text[a..=b])?),
                _ => Err(Error::msg("The translation reply was not a JSON array.")),
            }
        })
    }
}

/// Builds the translator chosen in settings, or None when translation is off or not set up.
pub fn from_settings(s: &SubtitleSettings, http: &reqwest::Client) -> Option<Box<dyn Translate>> {
    if !s.enabled {
        return None;
    }
    match s.provider.as_str() {
        "libretranslate" if !s.libretranslate_url.trim().is_empty() => Some(Box::new(LibreTranslate {
            http: http.clone(),
            url: s.libretranslate_url.clone(),
            api_key: s.libretranslate_api_key.clone(),
        })),
        "anthropic" if !s.anthropic_api_key.trim().is_empty() => Some(Box::new(Claude {
            http: http.clone(),
            api_base: "https://api.anthropic.com".into(),
            api_key: s.anthropic_api_key.clone(),
            model: s.anthropic_model.clone(),
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn libretranslate_posts_the_batch_and_reads_the_array() {
        let server = MockServer::start();
        let m = server.mock(|w, t| {
            w.method(POST).path("/translate").json_body_partial(r#"{"source":"en","target":"pt","api_key":"k"}"#);
            t.status(200).json_body(json!({"translatedText": ["a", "b"]}));
        });
        let t = LibreTranslate { http: reqwest::Client::new(), url: format!("{}/", server.base_url()), api_key: "k".into() };
        let out = t.translate(&["x".into(), "y".into()], "en", "pt").await.unwrap();
        assert_eq!(out, ["a", "b"]);
        m.assert();
    }

    #[tokio::test]
    async fn claude_translator_sends_key_and_parses_the_json_array() {
        let server = MockServer::start();
        let m = server.mock(|w, t| {
            w.method(POST).path("/v1/messages").header("x-api-key", "sk").header("anthropic-version", "2023-06-01")
                .json_body_partial(r#"{"model":"claude-haiku-5-5"}"#);
            t.status(200).json_body(json!({"content":[{"type":"text","text":"Sure: [\"um\",\"dois\"]"}]}));
        });
        let t = Claude { http: reqwest::Client::new(), api_base: server.base_url(), api_key: "sk".into(), model: "claude-haiku-5-5".into() };
        assert_eq!(t.translate(&["one".into(), "two".into()], "en", "pt").await.unwrap(), ["um", "dois"]);
        m.assert();
    }

    #[test]
    fn settings_choose_the_translator() {
        let http = reqwest::Client::new();
        let mut s = SubtitleSettings::default();
        assert_eq!(from_settings(&s, &http).unwrap().name(), "LibreTranslate");
        s.provider = "anthropic".into();
        assert!(from_settings(&s, &http).is_none(), "no key yet");
        s.anthropic_api_key = "k".into();
        assert_eq!(from_settings(&s, &http).unwrap().name(), "Claude");
        s.provider = "none".into();
        assert!(from_settings(&s, &http).is_none());
        s.provider = "libretranslate".into();
        s.enabled = false;
        assert!(from_settings(&s, &http).is_none());
    }
}
