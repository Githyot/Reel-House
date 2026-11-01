//! The JavaScript engine that runs LNReader / IReader plugins (QuickJS, through `rquickjs`).
//! A plugin is a script that scrapes a site; `assets/js/runtime.js` gives it fetch, cheerio, dayjs and the other
//! libraries it asks for. The only things that cross into Rust are `__http` and `__log`.
use crate::http::{Http, HttpRef, Request};
use crate::json::Json;

pub const RUNTIME_JS: &str = include_str!("../assets/js/runtime.js");

/// The answer of `__http`, as JSON text: `{"status":200,"url":"…","headers":{},"body":"…"}` or `{"error":"…"}`.
pub fn http_call(http: &dyn Http, method: &str, url: &str, headers: &str, body: &str) -> String {
    let method: &'static str = match method.to_ascii_uppercase().as_str() {
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DELETE",
        "PATCH" => "PATCH",
        "HEAD" => "HEAD",
        _ => "GET",
    };
    let mut req = Request { method, url: url.to_string(), headers: vec![], body: None, timeout_secs: 30 };
    let mut ctype = String::new();
    if let Ok(h) = Json::parse(headers) {
        for (k, v) in h.entries() {
            if k.eq_ignore_ascii_case("content-type") {
                ctype = v.str();
            } else {
                req.headers.push((k.clone(), v.str()));
            }
        }
    }
    // a site whose bot check was passed in the in-app browser: its cookies and that browser's User-Agent
    crate::sitepass::apply(url, &mut req.headers);
    if !body.is_empty() && method != "GET" && method != "HEAD" {
        req.body = Some((if ctype.is_empty() { "application/x-www-form-urlencoded".to_string() } else { ctype }, body.as_bytes().to_vec()));
    }
    match http.send(&req) {
        Ok(r) => Json::obj(vec![("status", (r.status as f64).into()), ("url", url.into()), ("headers", Json::obj(vec![])), ("body", r.text().into())]).dump(),
        Err(e) => Json::obj(vec![("error", e.into())]).dump(),
    }
}

#[cfg(feature = "js")]
fn describe(ctx: &rquickjs::Ctx<'_>, e: rquickjs::Error) -> String {
    if matches!(e, rquickjs::Error::Exception) {
        let v = ctx.catch();
        if let Some(ex) = v.as_exception() {
            if let Some(m) = ex.message() {
                return m;
            }
        }
        if let Some(s) = v.as_string().and_then(|s| s.to_string().ok()) {
            return s;
        }
    }
    e.to_string()
}

/// Loads the plugin and calls `method` with `args`. The answer is what the plugin returned, as JSON.
#[cfg(feature = "js")]
pub fn run(http: HttpRef, id: &str, code: &str, method: &str, args: &Json) -> Result<Json, String> {
    use rquickjs::{Context, Function, Runtime, Value};
    let rt = Runtime::new().map_err(|e| e.to_string())?;
    rt.set_memory_limit(768 << 20);
    rt.set_max_stack_size(4 << 20);
    let started = std::time::Instant::now();
    rt.set_interrupt_handler(Some(Box::new(move || started.elapsed().as_secs() > 120)));
    let ctx = Context::full(&rt).map_err(|e| e.to_string())?;
    let (id_lit, method_lit, args_lit) = (Json::Str(id.to_string()).dump(), Json::Str(method.to_string()).dump(), Json::Str(args.dump()).dump());
    let code_lit = Json::Str(code.to_string()).dump();
    ctx.with(|ctx| -> Result<(), String> {
        let g = ctx.globals();
        let log = Function::new(ctx.clone(), |s: String| {
            eprintln!("[plugin] {}", s);
        })
        .map_err(|e| describe(&ctx, e))?;
        g.set("__log", log).map_err(|e| describe(&ctx, e))?;
        let h = http.clone();
        let fetch = Function::new(ctx.clone(), move |m: String, u: String, hd: String, b: String| -> String { http_call(&*h, &m, &u, &hd, &b) }).map_err(|e| describe(&ctx, e))?;
        g.set("__http", fetch).map_err(|e| describe(&ctx, e))?;
        ctx.eval::<Value, _>(RUNTIME_JS).map_err(|e| describe(&ctx, e))?;
        ctx.eval::<Value, _>(format!("__load({}, {})", id_lit, code_lit)).map_err(|e| describe(&ctx, e))?;
        ctx.eval::<Value, _>(format!("__call({}, {}, {})", id_lit, method_lit, args_lit)).map_err(|e| describe(&ctx, e))?;
        Ok(())
    })?;
    // Every request is answered at once, so the whole plugin runs inside the job queue.
    let mut guard = 0u32;
    while rt.is_job_pending() && guard < 5_000_000 {
        let _ = rt.execute_pending_job();
        guard += 1;
    }
    let out: Option<String> = ctx.with(|ctx| ctx.globals().get::<_, Option<String>>("__out").map_err(|e| describe(&ctx, e)))?;
    let text = out.ok_or_else(|| "The plugin did not finish (it may be waiting for something this app cannot do).".to_string())?;
    let j = Json::parse(&text)?;
    if let Some(e) = j.get("err").as_str() {
        return Err(e.to_string());
    }
    Ok(j.get("ok").clone())
}

#[cfg(not(feature = "js"))]
pub fn run(_http: HttpRef, _id: &str, _code: &str, _method: &str, _args: &Json) -> Result<Json, String> {
    Err("This build of Reelhouse has no JavaScript engine.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    #[test]
    fn requests_are_translated() {
        let h = Fake::new().route("POST https://s.com/ajax", 200, "hello");
        let out = http_call(&h, "post", "https://s.com/ajax", r#"{"Content-Type":"application/x-www-form-urlencoded","X-A":"1"}"#, "a=1");
        let j = Json::parse(&out).unwrap();
        assert_eq!(j.get("status").as_u64(), Some(200));
        assert_eq!(j.get("body").str(), "hello");
        let sent = &h.requests()[0];
        assert_eq!(sent.method, "POST");
        assert_eq!(sent.headers, vec![("X-A".to_string(), "1".to_string())]);
        assert_eq!(sent.body.as_ref().unwrap().0, "application/x-www-form-urlencoded");
        assert_eq!(sent.body.as_ref().unwrap().1, b"a=1");
    }

    #[test]
    fn get_has_no_body() {
        let h = Fake::new().route("GET https://s.com/", 404, "no");
        let j = Json::parse(&http_call(&h, "GET", "https://s.com/", "{}", "ignored")).unwrap();
        assert_eq!(j.get("status").as_u64(), Some(404));
        assert!(h.requests()[0].body.is_none());
    }

    const DEMO: &str = r##"
        "use strict";
        Object.defineProperty(exports, "__esModule", { value: true });
        const { fetchApi } = require("@libs/fetch");
        const { load } = require("cheerio");
        const { NovelStatus } = require("@libs/novelStatus");
        class Demo {
            constructor() { this.id = "demo"; this.name = "Demo"; this.site = "https://demo.test"; this.version = "1.0.0"; }
            async popularNovels(page, opts) {
                const r = await fetchApi(this.site + "/list?page=" + page);
                const $ = load(await r.text());
                return $(".n").map((i, e) => ({ name: $(e).find("a").text().trim(), path: $(e).find("a").attr("href"), cover: $(e).find("img").attr("src") })).get();
            }
            async parseChapter(path) {
                const $ = load(await (await fetchApi(this.site + path)).text());
                $(".ad").remove();
                return $("#c").html();
            }
            async boom() { throw new Error("no way"); }
            status() { return NovelStatus.Ongoing; }
        }
        exports.default = new Demo();
    "##;

    #[cfg(feature = "js")]
    #[test]
    fn runs_a_plugin() {
        let http: HttpRef = std::sync::Arc::new(
            Fake::new()
                .route("GET https://demo.test/list?page=2", 200, r#"<div class="n"><a href="/a">Alpha &amp; Co</a><img src="a.jpg"></div><div class="n"><a href="/b">Beta</a></div>"#)
                .route("GET https://demo.test/c1", 200, r#"<div id="c"><p>One</p><div class="ad">buy</div><p>Two</p></div>"#),
        );
        let list = run(http.clone(), "demo", DEMO, "popularNovels", &Json::Arr(vec![2.0.into(), Json::obj(vec![("showLatestNovels", false.into())])])).unwrap();
        assert_eq!(list.items().len(), 2);
        assert_eq!(list.at(0).get("name").str(), "Alpha & Co");
        assert_eq!(list.at(0).get("path").str(), "/a");
        assert_eq!(list.at(0).get("cover").str(), "a.jpg");
        let html = run(http.clone(), "demo", DEMO, "parseChapter", &Json::Arr(vec!["/c1".into()])).unwrap();
        assert_eq!(html.str(), "<p>One</p><p>Two</p>");
        let e = run(http.clone(), "demo", DEMO, "boom", &Json::Arr(vec![])).unwrap_err();
        assert_eq!(e, "no way");
        let e = run(http.clone(), "demo", DEMO, "missing", &Json::Arr(vec![])).unwrap_err();
        assert!(e.contains("no missing"), "{}", e);
        assert_eq!(run(http, "demo", "throw new Error('bad script')", "x", &Json::Arr(vec![])).unwrap_err(), "bad script");
    }

    /// The readnovelfull plugins ask the htmlparser2 Parser whether a tag is void while copying a chapter.
    #[cfg(feature = "js")]
    #[test]
    fn parser_knows_void_elements() {
        let code = r#"var o = require("htmlparser2"); exports.default = { id: "v", parseChapter: async function(p) { var w = new o.Parser({}); return String(w.isVoidElement("br")) + String(w.isVoidElement("div")); } };"#;
        let http: HttpRef = std::sync::Arc::new(Fake::new());
        assert_eq!(run(http, "v", code, "parseChapter", &Json::Arr(vec!["/c".into()])).unwrap().str(), "truefalse");
    }

    /// Runs a real LNReader plugin when the plugin files are around (not part of the repository).
    #[cfg(feature = "js")]
    #[test]
    fn runs_a_real_plugin_if_present() {
        let Ok(code) = std::fs::read_to_string("/tmp/ext/lnp/.js/src/plugins/english/BoxNovel[madara].js") else { return };
        let http: HttpRef = std::sync::Arc::new(Fake::new().route("GET https://novelnice.com//page/1/", 200, r#"<div class="page-item-detail"><div class="post-title"><h3><a href="https://novelnice.com/novel/foo/">Foo Novel</a></h3></div><img src="https://x/c.jpg"></div>"#));
        let list = run(http, "boxnovel", &code, "popularNovels", &Json::Arr(vec![1.0.into(), Json::obj(vec![("showLatestNovels", false.into()), ("filters", "__default".into())])])).unwrap();
        let s = crate::novels::parse_list("boxnovel", &list);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].title, "Foo Novel");
        assert_eq!(s[0].cover, "https://x/c.jpg");
    }

    #[test]
    fn runtime_script_is_embedded() {
        assert!(RUNTIME_JS.contains("__load") && RUNTIME_JS.contains("__call"));
    }
}
