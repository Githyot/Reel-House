//! Microsoft Translator (Azure AI Translator, Text API v3) with the user's own key. Part of `translate`.
//!
//! One request carries many texts (up to 1000 items and 50 000 characters), so a whole subtitle file or chapter goes
//! in a few requests. The free tier (F0) gives two million characters a month.
use super::Translator;
use crate::http::{Http, Request};
use crate::json::Json;

/// The global address; a resource with its own address (".cognitiveservices.azure.com") can be typed instead.
pub const AZURE_ENDPOINT: &str = "https://api.cognitive.microsofttranslator.com";
/// Limits of one request of the Text API v3.
pub const AZURE_MAX_ITEMS: usize = 1000;
pub const AZURE_MAX_CHARS: usize = 50_000;
/// How many times a request refused with 429 ("too many requests") is sent again before giving up.
const AZURE_RETRIES: u32 = 4;

pub struct Azure<'a> {
    pub http: &'a dyn Http,
    pub key: String,
    /// The region of the resource ("westeurope", "brazilsouth"…). Empty for a global resource.
    pub region: String,
    /// Empty: the global address.
    pub endpoint: String,
    /// How a wait is done (milliseconds). The tests replace it so they do not sleep.
    pub wait: fn(u64),
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

/// The language codes Microsoft uses: Chinese needs its script, Portuguese of Portugal has its own code, and the
/// three-letter codes of the subtitles and OCR are turned into two letters. Empty for "find out by yourself".
pub fn azure_code(c: &str) -> String {
    let low = c.trim().to_lowercase().replace('_', "-");
    match low.as_str() {
        "" | "auto" => String::new(),
        "zh" | "zh-cn" | "zh-hans" | "chs" | "chi" | "zho" => "zh-Hans".to_string(),
        "zh-tw" | "zh-hk" | "zh-hant" | "cht" => "zh-Hant".to_string(),
        "pt-pt" => "pt-pt".to_string(),
        "no" | "nor" | "nob" => "nb".to_string(),
        _ => crate::util::lang_base(&low),
    }
}

impl<'a> Azure<'a> {
    pub fn new(http: &'a dyn Http, key: &str, region: &str, endpoint: &str) -> Azure<'a> {
        Azure { http, key: key.trim().to_string(), region: region.trim().to_string(), endpoint: endpoint.trim().to_string(), wait: sleep_ms }
    }

    /// The address of a translation request. A resource address of its own keeps the Translator path under it.
    pub fn url(&self, source: &str, target: &str) -> String {
        let base = if self.endpoint.is_empty() { AZURE_ENDPOINT } else { self.endpoint.trim_end_matches('/') };
        let path = if base.contains(".cognitiveservices.azure.com") && !base.contains("/translator/") { "/translator/text/v3.0/translate" } else { "/translate" };
        let from = azure_code(source);
        let from = if from.is_empty() { String::new() } else { format!("&from={}", from) };
        format!("{}{}?api-version=3.0&to={}{}", base, path, azure_code(target), from)
    }

    /// One request: the texts as [{"Text": …}] and the key (and region) in the headers.
    pub fn request(&self, texts: &[String], source: &str, target: &str) -> Request {
        let body = Json::Arr(texts.iter().map(|t| Json::obj(vec![("Text", t.as_str().into())])).collect()).dump();
        let mut req = Request::post(&self.url(source, target)).header("Ocp-Apim-Subscription-Key", &self.key).json_body(body).timeout(60);
        if !self.region.is_empty() {
            req = req.header("Ocp-Apim-Subscription-Region", &self.region);
        }
        req
    }

    /// One batch, sent again (after a growing wait) when the service says it got too many requests.
    fn send_batch(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        let req = self.request(texts, source, target);
        let mut tries = 0;
        loop {
            let r = self.http.send(&req)?;
            if (r.status == 429 || r.status == 503) && tries < AZURE_RETRIES {
                // Our HTTP layer does not hand over the Retry-After header, so the wait doubles: 1, 2, 4, 8 s.
                (self.wait)(1000 << tries);
                tries += 1;
                continue;
            }
            if !r.ok() {
                return Err(azure_error(r.status, &r.text()));
            }
            let j = r.json()?;
            let items = j.items();
            if items.len() != texts.len() {
                return Err("Microsoft Translator returned an unexpected answer.".to_string());
            }
            return Ok(items
                .iter()
                .zip(texts)
                .map(|(it, orig)| {
                    let t = it.get("translations").at(0).get("text").str();
                    if t.is_empty() { orig.clone() } else { t }
                })
                .collect());
        }
    }
}

/// A readable message for a refusal; the service explains itself in {"error":{"code":…,"message":…}}.
fn azure_error(status: u16, body: &str) -> String {
    let msg = Json::parse(body).map(|j| j.path("error.message").str()).unwrap_or_default();
    let msg = if msg.is_empty() { body.chars().take(140).collect::<String>() } else { msg };
    match status {
        401 => format!("Microsoft Translator refused the key (401). Check the key and the region in Preferences > Translators. {}", msg).trim().to_string(),
        403 => format!("Microsoft Translator refused the request (403), maybe the monthly quota is used up: {}", msg),
        429 => "Microsoft Translator is limiting the requests (429). Try again in a minute.".to_string(),
        _ => format!("Microsoft Translator answered {}: {}", status, msg),
    }
}

/// Splits the texts into requests within the limits of the service (items and characters), keeping the order.
pub fn azure_batches(texts: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < texts.len() {
        let mut end = at;
        let mut chars = 0;
        while end < texts.len() && end - at < AZURE_MAX_ITEMS {
            let n = texts[end].chars().count();
            if end > at && chars + n > AZURE_MAX_CHARS {
                break;
            }
            chars += n;
            end += 1;
        }
        out.push((at, end));
        at = end;
    }
    out
}

impl Translator for Azure<'_> {
    fn name(&self) -> String {
        "Microsoft Translator".to_string()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        if self.key.is_empty() {
            return Err("Microsoft Translator needs a key. Add it in Preferences > Translators.".to_string());
        }
        let mut out = Vec::with_capacity(texts.len());
        for (a, b) in azure_batches(texts) {
            out.extend(self.send_batch(&texts[a..b], source, target)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;
    use std::sync::atomic::{AtomicU64, Ordering};

    static WAITED: AtomicU64 = AtomicU64::new(0);
    fn no_wait(ms: u64) {
        WAITED.fetch_add(ms, Ordering::SeqCst);
    }

    fn azure<'a>(http: &'a dyn Http, region: &str, endpoint: &str) -> Azure<'a> {
        Azure { wait: no_wait, ..Azure::new(http, "KEY", region, endpoint) }
    }

    #[test]
    fn azure_builds_the_request_and_reads_the_answer() {
        let f = Fake::new().route(
            "POST https://api.cognitive.microsofttranslator.com/translate",
            200,
            r#"[{"translations":[{"text":"Olá","to":"pt"}]},{"detectedLanguage":{"language":"en","score":1},"translations":[{"text":"","to":"pt"}]}]"#,
        );
        let a = azure(&f, "westeurope", "");
        assert_eq!(a.translate(&["Hello".into(), "x".into()], "en", "pt-BR").unwrap(), vec!["Olá", "x"]);
        let r = &f.requests()[0];
        assert_eq!(r.url, "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=pt&from=en");
        assert!(r.headers.iter().any(|(k, v)| k == "Ocp-Apim-Subscription-Key" && v == "KEY"));
        assert!(r.headers.iter().any(|(k, v)| k == "Ocp-Apim-Subscription-Region" && v == "westeurope"));
        let body = Json::parse(&String::from_utf8_lossy(&r.body.as_ref().unwrap().1)).unwrap();
        assert_eq!(body.at(0).get("Text").str(), "Hello");
        assert_eq!(body.items().len(), 2);
    }

    #[test]
    fn azure_leaves_out_an_empty_region_and_auto_source_and_maps_codes() {
        let f = Fake::new().route("microsofttranslator", 200, r#"[{"translations":[{"text":"你好"}]}]"#);
        azure(&f, "", "").translate(&["Hi".into()], "auto", "zh").unwrap();
        let r = &f.requests()[0];
        assert!(r.url.ends_with("to=zh-Hans"));
        assert!(!r.headers.iter().any(|(k, _)| k == "Ocp-Apim-Subscription-Region"));
        assert_eq!(azure_code("jpn"), "ja");
        assert_eq!(azure_code("pt-PT"), "pt-pt");
        assert_eq!(azure_code("zh-TW"), "zh-Hant");
    }

    #[test]
    fn azure_uses_a_resource_endpoint() {
        let f = Fake::new();
        let a = azure(&f, "eastus", "https://my-res.cognitiveservices.azure.com/");
        assert_eq!(a.url("ja", "en"), "https://my-res.cognitiveservices.azure.com/translator/text/v3.0/translate?api-version=3.0&to=en&from=ja");
    }

    #[test]
    fn azure_batches_by_items_and_characters() {
        let many: Vec<String> = (0..2500).map(|i| format!("l{}", i)).collect();
        assert_eq!(azure_batches(&many), vec![(0, 1000), (1000, 2000), (2000, 2500)]);
        let long: Vec<String> = (0..5).map(|_| "a".repeat(20_000)).collect();
        assert_eq!(azure_batches(&long), vec![(0, 2), (2, 4), (4, 5)]);
        // a text longer than the limit still goes, alone
        let huge = vec!["b".repeat(60_000), "c".into()];
        assert_eq!(azure_batches(&huge), vec![(0, 1), (1, 2)]);
        assert!(azure_batches(&[]).is_empty());
    }

    #[test]
    fn azure_sends_every_batch_in_order() {
        let ans = |n: usize, p: &str| format!("[{}]", (0..n).map(|i| format!(r#"{{"translations":[{{"text":"{}{}"}}]}}"#, p, i)).collect::<Vec<_>>().join(","));
        let f = Fake::new().route("microsofttranslator", 200, &ans(1000, "a")).route("microsofttranslator", 200, &ans(200, "b"));
        let texts: Vec<String> = (0..1200).map(|i| i.to_string()).collect();
        let out = azure(&f, "r", "").translate(&texts, "en", "pt").unwrap();
        assert_eq!(out.len(), 1200);
        assert_eq!((out[0].as_str(), out[999].as_str(), out[1000].as_str(), out[1199].as_str()), ("a0", "a999", "b0", "b199"));
        assert_eq!(f.requests().len(), 2);
    }

    #[test]
    fn azure_waits_and_retries_on_429_then_gives_up() {
        let f = Fake::new()
            .route("microsofttranslator", 429, r#"{"error":{"code":429001,"message":"too many"}}"#)
            .route("microsofttranslator", 200, r#"[{"translations":[{"text":"ok"}]}]"#);
        WAITED.store(0, Ordering::SeqCst);
        assert_eq!(azure(&f, "r", "").translate(&["a".into()], "en", "pt").unwrap(), vec!["ok"]);
        assert_eq!(f.requests().len(), 2);
        assert!(WAITED.load(Ordering::SeqCst) >= 1000);
        // always 429: one try and four retries, then a clear error
        let busy = Fake::new().route("microsofttranslator", 429, "{}");
        let e = azure(&busy, "r", "").translate(&["a".into()], "en", "pt").unwrap_err();
        assert_eq!(busy.requests().len(), 5);
        assert!(e.contains("429"));
    }

    #[test]
    fn azure_explains_a_bad_key_and_needs_one() {
        let f = Fake::new().route("microsofttranslator", 401, r#"{"error":{"code":401000,"message":"The request is not authorized because credentials are missing or invalid."}}"#);
        let e = azure(&f, "r", "").translate(&["a".into()], "en", "pt").unwrap_err();
        assert!(e.contains("401") && e.contains("not authorized"));
        let none = Fake::new();
        assert!(Azure::new(&none, " ", "", "").translate(&["a".into()], "en", "pt").unwrap_err().contains("needs a key"));
        assert!(none.requests().is_empty());
    }
}
