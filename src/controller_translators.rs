//! Preferences > Translators: the list of translators with their state, the translator chosen for each place of the
//! app, and the test that translates a sample sentence with one translator or all of them (time and result side by
//! side, to choose). Part of `controller`.
use super::*;

/// The last test of each translator (id → state).
#[derive(Clone)]
struct Test {
    /// "running", "ok" or "error"
    state: &'static str,
    /// The translation, or the error.
    text: String,
    ms: u64,
    /// The translator's own name after the test (the free chain says which service answered).
    via: String,
}

/// Kept outside `Core` because the worker threads write the answers straight here; the Info event that follows
/// makes the preferences read them again.
static TESTS: std::sync::Mutex<Vec<(String, Test)>> = std::sync::Mutex::new(Vec::new());

fn set_test(id: &str, t: Test) {
    let mut v = TESTS.lock().unwrap_or_else(|e| e.into_inner());
    match v.iter_mut().find(|(k, _)| k == id) {
        Some((_, x)) => *x = t,
        None => v.push((id.to_string(), t)),
    }
}

/// The local AI a translation waits for (None: another translator). See `Core::local_ai_gate`.
pub(crate) type LocalGate = Option<(crate::ollama_local::Local, String)>;
/// A status line to show while the local AI gets ready.
pub(crate) type Note<'a> = Box<dyn Fn(&str) + Send + Sync + 'a>;

/// A translator that first waits until the local AI is ready (the first time it is downloaded, which takes a while:
/// the translation waits instead of failing). `note` says so once, for a status line.
pub(crate) struct Gated<'a> {
    inner: Box<dyn translate::Translator + 'a>,
    gate: LocalGate,
    note: Option<Note<'a>>,
}

impl Gated<'_> {
    fn wait(&self) -> Result<(), String> {
        let Some((local, model)) = &self.gate else { return Ok(()) };
        let st = local.status();
        if !(st.phase == "ready" && &st.model == model) {
            if let Some(n) = &self.note {
                n("Getting the local AI ready (the first time it is downloaded)");
            }
        }
        local.wait_ready(model, std::time::Duration::from_secs(8 * 3600)).map_err(|e| format!("Local AI: {}", e))
    }
}

impl translate::Translator for Gated<'_> {
    fn name(&self) -> String {
        self.inner.name()
    }

    fn translate(&self, texts: &[String], source: &str, target: &str) -> Result<Vec<String>, String> {
        self.wait()?;
        self.inner.translate(texts, source, target)
    }

    fn translate_hinted(&self, texts: &[String], source: &str, target: &str, hint: &str) -> Result<Vec<String>, String> {
        self.wait()?;
        self.inner.translate_hinted(texts, source, target, hint)
    }
}

/// `tr` behind the gate of the local AI (as it is when the gate is None).
pub(crate) fn gated<'a>(tr: Box<dyn translate::Translator + 'a>, gate: &LocalGate, note: Option<Note<'a>>) -> Box<dyn translate::Translator + 'a> {
    if gate.is_none() {
        return tr;
    }
    Box::new(Gated { inner: tr, gate: gate.clone(), note })
}

impl Core {
    /// When a translation uses the local AI (`needed`): its setup starts in the background if it is not ready (the
    /// same one as Conversation's, shown by OllamaPanel) and the gate to wait on is returned.
    /// Never on Android and iOS, where the local AI does not exist (`platform::DESKTOP_AI`).
    pub(crate) fn local_ai_gate(&self, needed: bool) -> LocalGate {
        if !needed || !crate::platform::DESKTOP_AI {
            return None;
        }
        let model = translate::local_model(&self.settings);
        if self.ollama.begin(&model) {
            let local = self.ollama.clone();
            let dir = self.data_dir.clone();
            self.spawn(move |tx, http| {
                local.run(&*http, &dir, &|| send(tx, Msg::Kaiwa(controller_kaiwa::KaiwaMsg::Ollama)));
            });
        }
        Some((self.ollama.clone(), model))
    }

    /// The language the sample is translated into: the one asked, else the subtitles' one; English (the language of
    /// the sample) becomes Portuguese, or the test would translate nothing.
    fn sample_target(&self, target: &str) -> String {
        let t = util::lang_base(target);
        let t = if t.is_empty() { self.settings.target_lang(&self.system_lang) } else { t };
        if t == "en" { "pt".to_string() } else { t }
    }

    /// Translates the sample (or `text`) with the translator `id` and keeps the time and the result.
    pub fn translator_test(&mut self, id: &str, text: &str, target: &str) {
        self.translator_tests(&[id.to_string()], text, target);
    }

    /// The same with every translator that is ready (keys filled in), all at the same time.
    pub fn translator_test_all(&mut self, text: &str, target: &str) {
        // the local AI only once it is set up: "Test all" must not start a download of gigabytes
        let local_ready = self.ollama.status().phase == "ready";
        let ids: Vec<String> = translate::CATALOGUE
            .iter()
            .filter(|i| translate::configured(i.id, &self.settings) && (i.id != "ollama" || local_ready))
            .map(|i| i.id.to_string())
            .collect();
        self.translator_tests(&ids, text, target);
    }

    fn translator_tests(&mut self, ids: &[String], text: &str, target: &str) {
        let (text, source) = if text.trim().is_empty() { (translate::SAMPLE.to_string(), "en") } else { (text.trim().to_string(), "auto") };
        let target = self.sample_target(target);
        for id in ids {
            set_test(id, Test { state: "running", text: String::new(), ms: 0, via: String::new() });
            let (id, text, target, st) = (id.clone(), text.clone(), target.clone(), self.settings.clone());
            let gate = self.local_ai_gate(id == "ollama");
            self.spawn(move |tx, http| {
                let r = translate::build(&id, &st, &*http).map(|tr| gated(tr, &gate, None)).and_then(|tr| translate::timed_sample(tr.as_ref(), &text, source, &target).map(|(t, ms)| (t, ms, tr.name())));
                let line = match &r {
                    Ok((_, ms, via)) => format!("{}: {} ms", via, ms),
                    Err(e) => format!("{}: {}", translate::name_of(&id), e),
                };
                set_test(&id, match r {
                    Ok((t, ms, via)) => Test { state: "ok", text: t, ms, via },
                    Err(e) => Test { state: "error", text: e, ms: 0, via: String::new() },
                });
                // the Info event makes the preferences read the results again
                send(tx, Msg::Info(MsgKind::Translator, line));
            });
        }
        self.events.push(Event::Info(MsgKind::Translator, "Testing...".to_string()));
    }

    /// What Preferences > Translators shows: the translators (ready or not, last test) and each place with its choice.
    pub fn translators_json(&self) -> Json {
        let tests = TESTS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let s = &self.settings;
        let list: Vec<Json> = translate::CATALOGUE
            .iter()
            // the local AI is desktop only: Android and iOS do not list it
            .filter(|i| i.id != "ollama" || crate::platform::DESKTOP_AI)
            .map(|i| {
                let t = tests.iter().find(|(k, _)| k == i.id).map(|(_, t)| t.clone());
                Json::obj(vec![
                    ("id", i.id.into()),
                    ("name", i.name.into()),
                    ("free", i.free.into()),
                    ("note", i.note.into()),
                    ("ready", translate::configured(i.id, s).into()),
                    ("state", t.as_ref().map(|t| t.state).unwrap_or("").into()),
                    ("text", t.as_ref().map(|t| t.text.clone()).unwrap_or_default().into()),
                    ("ms", (t.as_ref().map(|t| t.ms).unwrap_or(0) as f64).into()),
                    ("via", t.map(|t| t.via).unwrap_or_default().into()),
                ])
            })
            .collect();
        let uses: Vec<Json> = translate::USES
            .iter()
            .map(|(id, name, detail)| {
                Json::obj(vec![
                    ("id", (*id).into()),
                    ("name", (*name).into()),
                    ("detail", (*detail).into()),
                    ("choice", s.translate.choice(id).into()),
                    ("auto", translate::auto_name(id, s).into()),
                ])
            })
            .collect();
        Json::obj(vec![
            ("sample", translate::SAMPLE.into()),
            ("target", self.sample_target("").into()),
            ("list", Json::Arr(list)),
            ("uses", Json::Arr(uses)),
        ])
    }
}
