//! C interface for the Qt interface. Everything crosses as JSON text:
//!   * `rh_cmd`   runs a command (`{"cmd":"open_detail", ...}`),
//!   * `rh_poll`  collects what changed since the last call (`{"events":[...]}`),
//!   * `rh_state` returns the current data of one page (`home`, `search`, `library`, `detail`, `player`, `settings`),
//!   * `rh_video_*` let a Qt Quick item draw the video of the core's mpv into an OpenGL framebuffer.
//! All calls except `rh_video_init` and `rh_video_render` come from the interface thread. Those two come from the render thread
//! (with the OpenGL context current). Returned strings must be released with `rh_free_str`.
use reelhouse::controller::{Core, Event, MsgKind, Stage};
use reelhouse::json::Json;
use reelhouse::model::{MetaItem, Row, Video};
use reelhouse::mpv::Mpv;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::{Arc, Mutex};

mod ds4;

struct SendCore(Core);
// The core is only used while the mutex is held.
unsafe impl Send for SendCore {}

pub struct Bridge {
    core: Mutex<SendCore>,
    mpv: Mutex<Option<Arc<Mpv>>>,
    frame_cb: Mutex<Option<(usize, usize)>>,
    /// The "next episode" card the last poll asked for.
    next: Mutex<Option<reelhouse::controller::NextCard>>,
    /// When the bridge started and when the slow periodic jobs last ran (backup, new-episode check).
    started: std::time::Instant,
    last_minute: Mutex<Option<std::time::Instant>>,
    last_stats: Mutex<Option<std::time::Instant>>,
    first_check_done: std::sync::atomic::AtomicBool,
    /// The title page closed last, for the mouse "forward" button (GTK Ui::forward).
    forward: Mutex<Option<MetaItem>>,
    /// Title pages left by opening a related title: Back returns to them one by one.
    trail: Mutex<Vec<MetaItem>>,
}

type FrameCb = extern "C" fn(*mut c_void);

fn s(v: &str) -> Json {
    Json::Str(v.to_string())
}
fn n(v: f64) -> Json {
    Json::Num(v)
}
fn opt(v: &Option<String>) -> Json {
    match v {
        Some(x) => Json::Str(x.clone()),
        None => Json::Null,
    }
}

fn out(j: Json) -> *mut c_char {
    CString::new(j.dump().replace('\0', "")).unwrap_or_default().into_raw()
}

fn video_json(c: &Core, v: &Video) -> Json {
    Json::obj(vec![
        ("watched", Json::Bool(c.is_watched(&v.id))),
        ("id", s(&v.id)),
        ("season", v.season.map(|x| n(x as f64)).unwrap_or(Json::Null)),
        ("episode", v.episode.map(|x| n(x as f64)).unwrap_or(Json::Null)),
        ("title", s(&v.title)),
        ("thumbnail", opt(&v.thumbnail)),
        ("released", s(&v.released)),
    ])
}

fn item_json(c: &Core, m: &MetaItem, full: bool) -> Json {
    let mut o = vec![
        ("id", s(&m.id)),
        ("kind", s(&m.kind)),
        ("name", s(&m.name)),
        ("poster", opt(&c.poster_for(m))),
        ("background", opt(&c.background_for(m))),
        ("logo", opt(&m.logo)),
        ("rating", s(&m.rating)),
        ("releaseInfo", s(&m.release_info)),
        ("inLibrary", Json::Bool(c.in_library(&m.id))),
        ("watched", Json::Bool(c.watched.contains(&m.id))),
        ("isNew", Json::Bool(c.has_new(&m.id))),
    ];
    if full {
        o.push(("description", s(&m.description)));
        o.push(("runtime", s(&m.runtime)));
        o.push(("genres", Json::Arr(m.genres.iter().map(|g| s(g)).collect())));
        o.push(("cast", Json::Arr(m.cast.iter().map(|g| s(g)).collect())));
        o.push(("videos", Json::Arr(m.videos.iter().map(|v| video_json(c, v)).collect())));
        o.push(("favorite", Json::Bool(c.is_favorite(&m.id))));
        o.push(("malId", m.mal_id.map(|x| n(x as f64)).unwrap_or(Json::Null)));
        o.push(("imdbId", s(&m.imdb_id)));
    }
    Json::obj(o)
}

fn row_json(c: &Core, r: &Row) -> Json {
    Json::obj(vec![
        ("title", s(&r.title)),
        ("addon", s(&r.addon)),
        ("kind", s(&r.kind)),
        ("platform", s(&platform_of(&r.addon, &r.title))),
        ("items", Json::Arr(r.items.iter().map(|m| item_json(c, m, false)).collect())),
    ])
}

fn item_from_json(j: &Json) -> MetaItem {
    let st = |k: &str| j.get(k).str();
    let o = |k: &str| j.get(k).as_str().filter(|x| !x.is_empty()).map(|x| x.to_string());
    MetaItem {
        id: st("id"),
        kind: st("kind"),
        name: st("name"),
        poster: o("poster"),
        background: o("background"),
        logo: o("logo"),
        description: st("description"),
        release_info: st("releaseInfo"),
        runtime: st("runtime"),
        rating: st("rating"),
        genres: j.get("genres").strings(),
        imdb_id: st("imdbId"),
        mal_id: j.get("malId").as_u64(),
        ..Default::default()
    }
}

/// One featured title per catalog row (with a wallpaper), with the platform of its row so the page can filter them.
fn hero_picks(c: &Core) -> Vec<(MetaItem, String)> {
    let mut picks: Vec<(MetaItem, String)> = Vec::new();
    for (_, row) in c.rows.iter() {
        for m in row.items.iter() {
            if m.background.is_some() && !picks.iter().any(|p| p.0.id == m.id) {
                picks.push((m.clone(), platform_of(&row.addon, &row.title)));
                break;
            }
        }
        if picks.len() >= 40 {
            break;
        }
    }
    picks
}

fn state_home(c: &Core) -> Json {
    let cont: Vec<Json> = c
        .progress
        .iter()
        .map(|p| {
            let mut j = item_json(c, &p.meta, false);
            j.set("label", s(&p.label));
            j.set("videoId", s(&p.video_id));
            j.set("fraction", n(p.fraction()));
            j.set("poster", match c.poster_for(&p.meta).or_else(|| c.background_for(&p.meta)) { Some(u) => Json::Str(u), None => Json::Null });
            j.set("favorite", Json::Bool(c.is_favorite(&p.meta.id)));
            j
        })
        .collect();
    Json::obj(vec![
        (
            "hero",
            Json::Arr(
                hero_picks(c)
                    .iter()
                    .map(|(m, p)| {
                        let mut j = item_json(c, m, true);
                        j.set("platform", s(p));
                        j
                    })
                    .collect(),
            ),
        ),
        ("continue", Json::Arr(cont)),
        ("favorites", Json::Arr(c.favorites.iter().map(|m| item_json(c, m, false)).collect())),
        ("loading", Json::Bool(c.home_loading)),
        ("errors", Json::Arr(c.home_errors.iter().map(|e| s(e)).collect())),
        ("rows", Json::Arr(c.rows.iter().map(|(_, r)| row_json(c, r)).collect())),
        ("taste", Json::Arr(c.taste.iter().map(|r| row_json(c, r)).collect())),
    ])
}

fn state_search(c: &Core) -> Json {
    Json::obj(vec![
        ("query", s(c.search_query.as_deref().unwrap_or(""))),
        ("loading", Json::Bool(c.search_loading)),
        ("rows", Json::Arr(c.search_rows.iter().map(|(_, r)| row_json(c, r)).collect())),
    ])
}

fn state_library(c: &Core) -> Json {
    Json::obj(vec![("items", Json::Arr(c.library.iter().map(|m| item_json(c, m, false)).collect())), ("cards", state_library_cards(c))])
}

fn stream_json(x: &reelhouse::streams::Stream) -> Json {
    Json::obj(vec![
        ("headline", s(&x.headline())),
        ("lines", Json::Arr(x.detail_lines().iter().map(|l| s(l)).collect())),
        ("quality", s(&if x.res != 0 { reelhouse::streams::res_label(x.res) } else { String::new() })),
        ("sizeText", s(&x.size_text())),
        ("isTorrent", Json::Bool(x.is_torrent())),
        ("addon", s(&x.addon)),
        ("name", s(&x.name)),
        ("desc", s(&x.desc)),
        ("res", n(x.res as f64)),
        ("seeders", n(x.seeders as f64)),
        ("sizeMb", n(x.size_mb as f64)),
        ("pack", Json::Bool(x.pack)),
        ("packText", s(&x.pack_text())),
        ("sizeState", s(x.size_state())),
        ("torrent", Json::Bool(x.info_hash.is_some())),
        ("local", Json::Bool(x.local.is_some())),
    ])
}

fn tracks_json(m: &Mpv, kind: &str) -> Json {
    Json::Arr(
        m.tracks(kind)
            .iter()
            .map(|t| {
                let base = if t.lang.is_empty() { "und".to_string() } else { reelhouse::util::lang_base(&t.lang) };
                Json::obj(vec![
                    ("id", n(t.id as f64)),
                    // "ja - [Erai-raws]" reads "日本語 - [Erai-raws]" in the audio and subtitle menus
                    ("label", s(&reelhouse::controller::track_label(&t.label()))),
                    ("title", s(&reelhouse::controller::track_label(&t.title))),
                    ("lang", s(&t.lang)),
                    ("langKey", s(&base)),
                    ("langName", s(&reelhouse::util::lang_name(&t.lang))),
                    ("selected", Json::Bool(t.selected)),
                    ("external", Json::Bool(t.external)),
                    ("file", s(&t.file)),
                ])
            })
            .collect(),
    )
}


// ---------------------------------------------------------------- pages of the GTK version (shared shapes)

/// The streaming platform a catalog row belongs to (the Catalog menu), as in the GTK browse page.
fn platform_of(addon: &str, title: &str) -> String {
    let a = format!("{} {}", addon, title).to_lowercase();
    const PLATFORMS: &[(&str, &[&str])] = &[
        ("Netflix", &["netflix"]),
        ("HBO Max", &["hbo", "max "]),
        ("Disney+", &["disney"]),
        ("Prime Video", &["prime", "amazon"]),
        ("Apple TV+", &["apple"]),
        ("Paramount+", &["paramount"]),
        ("Hulu", &["hulu"]),
        ("Peacock", &["peacock"]),
        ("Crunchyroll", &["crunchyroll"]),
        ("Anime", &["anime", "myanimelist", "mal", "anisync", "kitsu"]),
        ("Cinemeta", &["cinemeta"]),
    ];
    for (name, keys) in PLATFORMS {
        if keys.iter().any(|k| a.contains(k)) {
            return name.to_string();
        }
    }
    if addon.is_empty() {
        "Other".to_string()
    } else {
        addon.to_string()
    }
}

fn card_json(c: &Core, k: &reelhouse::mal::Card) -> Json {
    let m = k.to_meta();
    Json::obj(vec![
        ("id", n(k.id as f64)),
        ("key", s(&k.key)),
        ("title", s(&k.title)),
        ("image", s(&k.image)),
        ("score", n(k.score)),
        ("kind", s(&k.kind)),
        ("year", n(k.year as f64)),
        ("votes", n(k.votes as f64)),
        ("synopsis", s(&k.synopsis)),
        ("genres", Json::Arr(k.genres.iter().map(|g| s(g)).collect())),
        ("isNew", Json::Bool(!k.key.is_empty() && c.has_new(&k.key))),
        ("item", item_json(c, &m, false)),
    ])
}

fn card_from_json(j: &Json) -> reelhouse::mal::Card {
    reelhouse::mal::Card {
        id: j.get("id").as_u64().unwrap_or(0),
        title: j.get("title").str(),
        image: j.get("image").str(),
        score: j.get("score").as_f64().unwrap_or(0.0),
        kind: j.get("kind").str(),
        year: j.get("year").as_i64().unwrap_or(0),
        votes: j.get("votes").as_u64().unwrap_or(0),
        key: j.get("key").str(),
        synopsis: j.get("synopsis").str(),
        genres: j.get("genres").strings(),
    }
}

/// The category chips of a title page: MyAnimeList categories for anime, catalog genres otherwise (GTK: malinfo).
fn genre_chips(meta: &MetaItem, mal: &reelhouse::mal::MalData) -> Json {
    let mut out = Vec::new();
    let from_mal = mal.info.as_ref().map(|i| i.provider.is_empty()).unwrap_or(false);
    if from_mal {
        if let Some(i) = mal.info.as_ref() {
            for g in i.genres.iter().chain(i.themes.iter()).chain(i.demographics.iter()) {
                if !g.name.is_empty() {
                    out.push(Json::obj(vec![("label", s(&g.name)), ("name", s(&g.name)), ("id", n(g.id as f64)), ("kind", s("anime"))]));
                }
            }
        }
    } else {
        let anime = mal.id.is_some() || meta.mal_id.is_some() || meta.id.starts_with("kitsu:") || meta.id.starts_with("mal:");
        for g in meta.genres.iter() {
            let (name, kind) = if anime {
                match reelhouse::anilist::genre_name(g) {
                    Some(x) => (x, "anime"),
                    None => continue,
                }
            } else if g.eq_ignore_ascii_case("anime") {
                continue;
            } else {
                (g.clone(), if meta.kind == "movie" { "movie" } else { "series" })
            };
            out.push(Json::obj(vec![("label", s(g)), ("name", s(&name)), ("id", n(0.0)), ("kind", s(kind))]));
        }
    }
    Json::Arr(out)
}

fn named_list(v: &[reelhouse::mal::Named]) -> Json {
    Json::Arr(v.iter().map(|x| Json::obj(vec![("id", n(x.id as f64)), ("name", s(&x.name))])).collect())
}

fn pairs(v: &[(String, String)]) -> Json {
    Json::Arr(v.iter().map(|(a, b)| Json::obj(vec![("name", s(a)), ("url", s(b))])).collect())
}

/// Everything the INFO page shows (GTK: malinfo::refresh).
fn mal_json(c: &Core, m: &reelhouse::mal::MalData) -> Json {
    let info = match &m.info {
        Some(i) => Json::obj(vec![
            ("provider", s(&i.provider)),
            ("id", n(i.id as f64)),
            ("url", s(&i.url)),
            ("title", s(&i.title)),
            ("titleEn", s(&i.title_en)),
            ("titleJa", s(&i.title_ja)),
            ("synonyms", Json::Arr(i.synonyms.iter().map(|x| s(x)).collect())),
            ("image", s(&i.image)),
            ("trailer", s(&i.trailer)),
            ("kind", s(&i.kind)),
            ("source", s(&i.source)),
            ("episodes", i.episodes.map(|e| n(e as f64)).unwrap_or(Json::Null)),
            ("status", s(&i.status)),
            ("aired", s(&i.aired)),
            ("premiered", s(&i.premiered)),
            ("broadcast", s(&i.broadcast)),
            ("duration", s(&i.duration)),
            ("rating", s(&i.rating)),
            ("score", n(i.score)),
            ("scoredBy", n(i.scored_by as f64)),
            ("rank", n(i.rank as f64)),
            ("popularity", n(i.popularity as f64)),
            ("members", n(i.members as f64)),
            ("favorites", n(i.favorites as f64)),
            ("synopsis", s(&i.synopsis)),
            ("background", s(&i.background)),
            ("producers", named_list(&i.producers)),
            ("licensors", named_list(&i.licensors)),
            ("studios", named_list(&i.studios)),
            ("genres", named_list(&i.genres)),
            ("themes", named_list(&i.themes)),
            ("demographics", named_list(&i.demographics)),
            ("openings", Json::Arr(i.openings.iter().map(|x| s(x)).collect())),
            ("endings", Json::Arr(i.endings.iter().map(|x| s(x)).collect())),
            ("streaming", pairs(&i.streaming)),
            ("external", pairs(&i.external)),
            ("facts", pairs(&i.facts)),
            (
                "relations",
                Json::Arr(
                    i.relations
                        .iter()
                        .map(|r| Json::obj(vec![("kind", s(&r.kind)), ("entries", Json::Arr(r.entries.iter().map(|e| Json::obj(vec![("id", n(e.id as f64)), ("kind", s(&e.kind)), ("name", s(&e.name))])).collect()))]))
                        .collect(),
                ),
            ),
        ]),
        None => Json::Null,
    };
    let slow = m.started.map(|t| t.elapsed().as_secs() > 20).unwrap_or(false);
    Json::obj(vec![
        ("loading", Json::Bool(m.loading)),
        ("slow", Json::Bool(slow)),
        ("notFound", Json::Bool(m.not_found)),
        ("error", s(&m.error)),
        ("source", s(&m.source)),
        ("info", info),
        ("characters", Json::Arr(m.characters.iter().take(24).map(|x| Json::obj(vec![("name", s(&x.name)), ("role", s(&x.role)), ("image", s(&x.image)), ("actor", s(&x.actor)), ("actorLang", s(&x.actor_lang))])).collect())),
        ("staff", Json::Arr(m.staff.iter().take(20).map(|x| Json::obj(vec![("name", s(&x.name)), ("image", s(&x.image)), ("positions", s(&x.positions))])).collect())),
        ("recs", Json::Arr(m.recs.iter().map(|k| card_json(c, k)).collect())),
        (
            "reviews",
            Json::Arr(
                m.reviews
                    .iter()
                    .take(8)
                    .map(|r| {
                        Json::obj(vec![
                            ("user", s(&r.user)),
                            ("avatar", s(&r.avatar)),
                            ("date", s(r.date.get(..10).unwrap_or(&r.date))),
                            ("tag", s(&r.tag)),
                            ("score", n(r.score as f64)),
                            ("text", s(&r.text)),
                            (
                                "pieces",
                                Json::Arr(
                                    reelhouse::richtext::pieces(&r.text)
                                        .into_iter()
                                        .map(|p| match p {
                                            reelhouse::richtext::Piece::Text(t) => Json::obj(vec![("text", Json::Str(t))]),
                                            reelhouse::richtext::Piece::Image(u) => Json::obj(vec![("image", Json::Str(u))]),
                                        })
                                        .collect(),
                                ),
                            ),
                            ("spoiler", Json::Bool(r.spoiler)),
                            ("reactions", n(r.reactions as f64)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("reviewCount", n(m.reviews.len() as f64)),
        (
            "my",
            match &m.my {
                Some(x) => Json::obj(vec![("status", s(&x.status)), ("score", n(x.score as f64)), ("watched", n(x.watched as f64))]),
                None => Json::Null,
            },
        ),
        ("logged", Json::Bool(c.mal_logged_in())),
        ("statuses", Json::Arr(reelhouse::mal::STATUSES.iter().map(|(id, l)| Json::obj(vec![("id", s(id)), ("label", s(l))])).collect())),
    ])
}

fn state_notices(c: &Core) -> Json {
    Json::obj(vec![(
        "items",
        Json::Arr(
            c.notices
                .iter()
                .map(|x| {
                    Json::obj(vec![
                        ("id", s(&x.id)),
                        ("title", s(&x.title)),
                        ("poster", opt(&x.poster)),
                        ("label", s(&x.label)),
                        ("episodeTitle", s(&x.episode_title)),
                        ("kind", s(&x.kind)),
                        // banners of the Study mode kept in the bell: when, the macaw's mood and the click command
                        ("at", Json::Num(x.at as f64)),
                        ("bannerKind", s(&x.banner_kind)),
                        ("mood", s(&x.mood)),
                        ("action", Json::parse(&x.action).unwrap_or(Json::Null)),
                    ])
                })
                .collect(),
        ),
    )])
}

fn state_genre(c: &Core) -> Json {
    match c.genre.as_ref() {
        None => Json::obj(vec![("open", Json::Bool(false))]),
        Some(g) => Json::obj(vec![
            ("open", Json::Bool(true)),
            ("name", s(&g.name)),
            ("kind", s(&g.kind)),
            ("items", Json::Arr(g.items.iter().map(|k| card_json(c, k)).collect())),
            ("more", Json::Bool(g.more)),
            ("loading", Json::Bool(g.loading)),
            ("error", s(&g.error)),
        ]),
    }
}

fn chapter_json(c: &Core, series: &str, ch: &reelhouse::manga::Chapter) -> Json {
    let read = c.manga_is_read(series, &ch.id);
    Json::obj(vec![
        ("id", s(&ch.id)),
        ("number", s(&ch.number)),
        ("volume", s(&ch.volume)),
        ("title", s(&ch.title)),
        ("lang", s(&ch.lang)),
        ("pages", n(ch.pages as f64)),
        ("published", s(&ch.published)),
        ("group", s(&ch.group)),
        ("label", s(&ch.label())),
        ("read", Json::Bool(read)),
    ])
}


/// Library (with the count of chapters not read), categories and history of a shelf (Images and Text modes share it).
/// The manga server of the app listens on a new port at every start, so a cover saved with the old address is pointed at the current one.
fn local_cover(srv: &reelhouse::suwayomi::Server, cover: &str) -> String {
    for pre in ["http://127.0.0.1:", "http://localhost:"] {
        if let Some(rest) = cover.strip_prefix(pre) {
            if let Some(i) = rest.find('/') {
                if rest[i..].starts_with("/api/") {
                    return srv.url(&rest[i..]);
                }
            }
        }
    }
    cover.to_string()
}

fn shelf_json(st: &reelhouse::manga::State, series: &dyn Fn(&reelhouse::manga::Series) -> Json, fix: &dyn Fn(&str) -> String) -> Vec<(&'static str, Json)> {
    vec![
        (
            "library",
            Json::Arr(
                st.library
                    .iter()
                    .map(|m| {
                        let mut j = series(m);
                        j.set("unread", n(st.unread(m) as f64));
                        j
                    })
                    .collect(),
            ),
        ),
        ("categories", Json::Arr(st.categories.iter().map(|x| s(x)).collect())),
        (
            "history",
            Json::Arr(
                st.history
                    .iter()
                    .map(|h| Json::obj(vec![("seriesId", s(&h.series_id)), ("title", s(&h.title)), ("cover", s(&fix(&h.cover))), ("chapterId", s(&h.chapter_id)), ("label", s(&h.label)), ("at", n(h.at as f64))]))
                    .collect(),
            ),
        ),
    ]
}

/// What the series page needs besides the chapters: the chapter to resume, flags per chapter and the counts.
fn view_extras(st: &reelhouse::manga::State, series_id: &str, chapters: &[reelhouse::manga::Chapter], mut rows: Vec<Json>) -> (Vec<Json>, Vec<(&'static str, Json)>) {
    let prog = st.progress.get(series_id).cloned();
    let read_ids: Vec<String> = prog.as_ref().map(|p| p.read.clone()).unwrap_or_default();
    let at = |id: &str| chapters.iter().position(|c| c.id == id);
    let cur_idx = prog.as_ref().and_then(|p| if p.chapter.is_empty() { None } else { at(&p.chapter) }).or_else(|| read_ids.iter().filter_map(|r| at(r)).max());
    let in_lib = st.in_library(series_id);
    for (i, row) in rows.iter_mut().enumerate() {
        let ch = &chapters[i];
        let read = read_ids.contains(&ch.id);
        let current = prog.as_ref().map(|p| p.chapter == ch.id && !read).unwrap_or(false);
        row.set("current", Json::Bool(current));
        row.set("page", n(if current { prog.as_ref().map(|p| p.page as f64 + 1.0).unwrap_or(0.0) } else { 0.0 }));
        row.set("new", Json::Bool(in_lib && !read && cur_idx.map(|c| i > c).unwrap_or(false)));
    }
    let next = reelhouse::manga::next_to_read(chapters, prog.as_ref()).map(|c| c.id.clone()).unwrap_or_default();
    let started = prog.as_ref().map(|p| !p.read.is_empty() || !p.chapter.is_empty()).unwrap_or(false);
    let unread = chapters.iter().filter(|c| !read_ids.contains(&c.id)).count();
    let last = chapters.last().map(|c| c.published.chars().take(10).collect::<String>()).unwrap_or_default();
    (rows, vec![("next", s(&next)), ("started", Json::Bool(started)), ("unread", n(unread as f64)), ("lastUpdate", s(&last))])
}

/// The Images mode: sources, the explore list, the library, the series page and the reader.
fn state_manga(c: &Core) -> Json {
    let in_lib = |id: &str| c.manga_in_library(id);
    let srv = c.suwayomi_server();
    let series = |m: &reelhouse::manga::Series| {
        let mut j = reelhouse::manga::series_json(m);
        j.set("cover", s(&local_cover(&srv, &m.cover)));
        j.set("inLibrary", Json::Bool(in_lib(&m.id)));
        j
    };
    let list = &c.manga_list;
    let view = match c.manga_view.as_ref() {
        None => Json::Null,
        Some(v) => {
            let rows: Vec<Json> = v.chapters.iter().map(|ch| chapter_json(c, &v.series.id, ch)).collect();
            let (rows, extra) = view_extras(&c.manga, &v.series.id, &v.chapters, rows);
            let src = c.manga_sources.iter().find(|x| v.series.id.contains(&x.id)).map(|x| format!("{} ({})", x.name, x.lang.to_uppercase())).unwrap_or_else(|| if v.series.id.starts_with("mangadex") { "MangaDex".to_string() } else { String::new() });
            let icon = c.manga_sources.iter().find(|x| v.series.id.contains(&x.id)).map(|x| x.icon.clone()).unwrap_or_default();
            let mut fields = vec![("series", series(&v.series)), ("chapters", Json::Arr(rows)), ("loading", Json::Bool(v.loading)), ("refreshing", Json::Bool(v.refreshing)), ("error", s(&v.error)), ("sourceName", s(&src)), ("sourceIcon", s(&icon))];
            fields.extend(extra);
            Json::obj(fields)
        }
    };
    let reader = match c.manga_reader.as_ref() {
        None => Json::Null,
        Some(r) => Json::obj(vec![
            ("seriesId", s(&r.series.id)),
            ("title", s(&r.series.title)),
            ("chapter", chapter_json(c, &r.series.id, &r.chapter)),
            ("pages", Json::Arr(r.pages.iter().map(|p| s(p)).collect())),
            ("resume", n(c.manga_resume_page() as f64)),
            ("hasPrev", Json::Bool(c.manga_neighbour(-1).is_some())),
            ("hasNext", Json::Bool(c.manga_neighbour(1).is_some())),
            ("loading", Json::Bool(r.loading)),
            ("error", s(&r.error)),
        ]),
    };
    let novels = state_novels(c);
    let mut top = vec![
        ("novels", novels),
        // hand corrections of the chapter in the reader (bubbles and picture edits), from its .md file
        ("pagedb", c.pagedb_json()),
        ("pagetools", state_pagetools(c)),
        ("mode", s(&c.settings.general.mode)),
        ("source", s(&c.settings.general.manga_source)),
        ("sources", Json::Arr(c.manga_sources.iter().map(|x| Json::obj(vec![("id", s(&x.id)), ("name", s(&x.name)), ("lang", s(&x.lang)), ("code", s(&reelhouse::manga::lang_code(&x.lang))), ("langName", s(&reelhouse::util::lang_name(&x.lang))), ("icon", s(&x.icon))])).collect())),
        ("pins", Json::Arr(c.settings.general.manga_pins.iter().map(|x| s(x)).collect())),
        ("langs", Json::Arr(c.settings.general.manga_langs.iter().map(|x| s(x)).collect())),
        ("status", s(&c.suwayomi_status)),
    ];
    top.extend(shelf_json(&c.manga, &series, &|u| local_cover(&srv, u)));
    top.extend(vec![
        (
            "list",
            Json::obj(vec![
                ("what", s(match &list.what { reelhouse::manga::Browse::Popular => "popular", reelhouse::manga::Browse::Latest => "latest", reelhouse::manga::Browse::Search(_) => "search" })),
                ("items", Json::Arr(list.items.iter().map(|m| series(m)).collect())),
                ("more", Json::Bool(list.more)),
                ("loading", Json::Bool(list.loading)),
                ("error", s(&list.error)),
            ]),
        ),
        ("view", view),
        ("reader", reader),
    ]);
    Json::obj(top)
}

/// A book made from a file (or written in the app) rather than a site: its text can be edited and chapters added.
fn novel_own(id: &str) -> bool {
    reelhouse::novels::split_id(id).map(|(p, _)| p == reelhouse::epub::PLUGIN).unwrap_or(false)
}

/// Base64 (as Qt writes it) back to bytes. Bad characters are skipped.
fn b64_decode(text: &str) -> Vec<u8> {
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// What the reader needs besides the text: OCR results per page, translations per request, upscaled files per page.
fn state_pagetools(c: &Core) -> Json {
    let text = Json::Obj(
        c.reader_text
            .iter()
            .map(|(url, r)| {
                let v = match r {
                    Ok(blocks) => {
                        // "known": every word is known in the study language (the reader offers a quiz card)
                        let known = c.page_known(url);
                        Json::obj(vec![(
                            "blocks",
                            Json::Arr(blocks.iter().enumerate().map(|(i, b)| Json::obj(vec![("text", s(&b.text)), ("x", n(b.x)), ("y", n(b.y)), ("w", n(b.w)), ("h", n(b.h)), ("known", Json::Bool(known.get(i).copied().unwrap_or(false)))])).collect()),
                        )])
                    }
                    Err(e) => Json::obj(vec![("error", s(e))]),
                };
                (url.clone(), v)
            })
            .collect(),
    );
    let translations = Json::Obj(
        c.reader_translations
            .iter()
            .map(|(id, r)| {
                let v = match r {
                    Ok(texts) => Json::obj(vec![("texts", Json::Arr(texts.iter().map(|t| s(t)).collect()))]),
                    Err(e) => Json::obj(vec![("error", s(e))]),
                };
                (id.to_string(), v)
            })
            .collect(),
    );
    let upscaled = Json::Obj(
        c.reader_upscaled
            .iter()
            .map(|(url, r)| {
                let v = match r {
                    Ok(p) => Json::obj(vec![("path", s(&p.to_string_lossy()))]),
                    Err(e) => Json::obj(vec![("error", s(e))]),
                };
                (url.clone(), v)
            })
            .collect(),
    );
    let progress = Json::Obj(c.reader_tr_progress.iter().map(|(id, (d, t))| (id.to_string(), Json::Arr(vec![n(*d as f64), n(*t as f64)]))).collect());
    Json::obj(vec![
        ("trProgress", progress),
        ("engine", Json::Bool(c.upscale_engine().is_some())),
        ("models", Json::Arr(c.upscale_models().0.iter().map(|m| s(m)).collect())),
        ("model", s(&c.upscale_models().1)),
        ("text", text),
        ("translations", translations),
        ("upscaled", upscaled),
    ])
}

/// The Text mode: installed plugins (sources), the catalogue to install from, the explore list, the library, the book page and the reader.
fn state_novels(c: &Core) -> Json {
    let series = |m: &reelhouse::manga::Series| {
        let mut j = reelhouse::manga::series_json(m);
        j.set("inLibrary", Json::Bool(c.novels.library.iter().any(|x| x.id == m.id)));
        j
    };
    let list = &c.novel_list;
    let view = match c.novel_view.as_ref() {
        None => Json::Null,
        Some(v) => {
            let rows: Vec<Json> = v
                .chapters
                .iter()
                .map(|ch| {
                    Json::obj(vec![
                        ("id", s(&ch.id)),
                        ("number", s(&ch.number)),
                        ("volume", s(&ch.volume)),
                        ("title", s(&ch.title)),
                        ("lang", s(&ch.lang)),
                        ("published", s(&ch.published)),
                        ("group", s(&ch.group)),
                        ("label", s(&ch.label())),
                        ("read", Json::Bool(c.novel_is_read(&v.series.id, &ch.id))),
                    ])
                })
                .collect();
            let (rows, extra) = view_extras(&c.novels, &v.series.id, &v.chapters, rows);
            let src = c.novel_installed.iter().find(|x| v.series.id.starts_with(&format!("{}:", x.id))).map(|x| x.name.clone()).unwrap_or_default();
            let icon = c.novel_installed.iter().find(|x| v.series.id.starts_with(&format!("{}:", x.id))).map(|x| x.icon.clone()).unwrap_or_default();
            let mut fields = vec![("series", series(&v.series)), ("chapters", Json::Arr(rows)), ("own", Json::Bool(novel_own(&v.series.id))), ("loading", Json::Bool(v.loading)), ("refreshing", Json::Bool(v.refreshing)), ("error", s(&v.error)), ("sourceName", s(&src)), ("sourceIcon", s(&icon))];
            fields.extend(extra);
            Json::obj(fields)
        }
    };
    let reader = match c.novel_reader.as_ref() {
        None => Json::Null,
        Some(r) => Json::obj(vec![
            ("seriesId", s(&r.series.id)),
            ("title", s(&r.series.title)),
            ("chapterId", s(&r.chapter.id)),
            ("chapter", s(&r.chapter.number)),
            ("chapterTitle", s(&r.chapter.title)),
            ("html", s(&reelhouse::novels::to_html(&r.blocks))),
            ("paras", Json::Arr(r.blocks.iter().map(|b| Json::obj(vec![("text", s(&b.text)), ("heading", Json::Bool(b.heading)), ("image", s(b.image.as_deref().unwrap_or("")))])).collect())),
            ("resume", n(c.novel_resume_at() as f64)),
            ("hasPrev", Json::Bool(c.novel_neighbour(-1).is_some())),
            ("hasNext", Json::Bool(c.novel_neighbour(1).is_some())),
            ("loading", Json::Bool(r.loading)),
            ("edited", Json::Bool(r.edited)),
            ("own", Json::Bool(novel_own(&r.series.id))),
            ("markup", s(&c.novel_markup())),
            // study in the reader: learned words to mix in, quiz sentences (see reelhouse::readstudy)
            ("study", r.study.clone()),
            ("error", s(&r.error)),
        ]),
    };
    let mut top = vec![
        ("langs", Json::Arr(c.settings.general.novel_langs.iter().map(|x| s(x)).collect())),
        ("source", s(&c.settings.general.novel_source)),
        (
            "sources",
            Json::Arr(c.novel_installed.iter().map(|p| Json::obj(vec![("id", s(&p.id)), ("name", s(&p.name)), ("lang", s(&p.lang)), ("code", s(&reelhouse::manga::lang_code(&p.lang))), ("langName", s(&reelhouse::util::lang_name(&p.lang))), ("icon", s(&p.icon))])).collect()),
        ),
        (
            "catalog",
            Json::obj(vec![
                ("status", s(&c.novel_catalog_status)),
                (
                    "items",
                    Json::Arr(
                        c.novel_catalog
                            .iter()
                            .map(|p| {
                                Json::obj(vec![
                                    ("id", s(&p.id)),
                                    ("name", s(&p.name)),
                                    ("site", s(&p.site)),
                                    ("lang", s(&p.lang)),
                                    ("version", s(&p.version)),
                                    ("icon", s(&p.icon)),
                                    ("installed", Json::Bool(c.novel_is_installed(&p.id).is_some())),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        ),
    ];
    top.extend(shelf_json(&c.novels, &series, &|u| u.to_string()));
    top.extend(vec![
        (
            "list",
            Json::obj(vec![
                ("what", s(match &list.what { reelhouse::manga::Browse::Popular => "popular", reelhouse::manga::Browse::Latest => "latest", reelhouse::manga::Browse::Search(_) => "search" })),
                ("items", Json::Arr(list.items.iter().map(|m| series(m)).collect())),
                ("more", Json::Bool(list.more)),
                ("loading", Json::Bool(list.loading)),
                ("error", s(&list.error)),
            ]),
        ),
        ("view", view),
        ("reader", reader),
    ]);
    Json::obj(top)
}

fn state_calendar(c: &Core) -> Json {
    Json::obj(vec![
        ("loading", Json::Bool(c.calendar_loading)),
        ("info", s(&c.calendar_info)),
        ("hasSeries", Json::Bool(c.library.iter().any(|m| m.kind == "series"))),
        (
            "entries",
            Json::Arr(
                c.calendar
                    .iter()
                    .map(|e| Json::obj(vec![("date", s(&e.date)), ("showId", s(&e.show_id)), ("show", s(&e.show)), ("poster", opt(&e.poster)), ("label", s(&e.label)), ("title", s(&e.title))]))
                    .collect(),
            ),
        ),
    ])
}

/// The library as category cards (GTK: genre::card_of).
fn state_library_cards(c: &Core) -> Json {
    Json::Arr(
        c.library
            .iter()
            .map(|m| {
                let year = m.release_info.split(|ch: char| !ch.is_ascii_digit()).find(|x| x.len() == 4).and_then(|x| x.parse().ok()).unwrap_or(0);
                let k = reelhouse::mal::Card {
                    title: m.name.clone(),
                    image: c.poster_for(m).unwrap_or_default(),
                    score: m.rating.trim().parse().unwrap_or(0.0),
                    kind: m.kind.clone(),
                    year,
                    key: m.id.clone(),
                    synopsis: m.description.clone(),
                    genres: m.genres.clone(),
                    ..Default::default()
                };
                let mut j = card_json(c, &k);
                j.set("item", item_json(c, m, false));
                j
            })
            .collect(),
    )
}

fn state_tasks(c: &Core) -> Json {
    Json::obj(vec![(
        "items",
        Json::Arr(
            c.tasks()
                .iter()
                .take(4)
                .map(|t| Json::obj(vec![("key", s(&t.key)), ("title", s(&t.title)), ("what", s(t.what)), ("detail", s(&t.detail)), ("fraction", t.fraction.map(n).unwrap_or(Json::Null))]))
                .collect(),
        ),
    )])
}

fn state_dlpick(c: &Core) -> Json {
    match c.dl_pick.as_ref() {
        None => Json::obj(vec![("open", Json::Bool(false))]),
        Some(p) => Json::obj(vec![
            ("open", Json::Bool(true)),
            ("season", n(p.season as f64)),
            ("seasons", Json::Arr(p.seasons.iter().map(|x| n(*x as f64)).collect())),
            ("loading", Json::Bool(p.loading)),
            ("list", Json::Arr(p.list.iter().map(stream_json).collect())),
            ("errors", Json::Arr(p.errors.iter().map(|e| s(e)).collect())),
        ]),
    }
}

fn state_torrents(c: &mut Core) -> Json {
    let hb = reelhouse::torrent::human_bytes;
    let ov = c.torrent_overview();
    Json::obj(vec![
        ("total", s(&hb(ov.total))),
        ("saved", s(&hb(ov.saved))),
        ("cache", s(&hb(ov.cache))),
        (
            "groups",
            Json::Arr(
                ov.groups
                    .iter()
                    .map(|g| {
                        Json::obj(vec![
                            ("title", s(&g.title)),
                            ("poster", opt(&g.poster)),
                            (
                                "rows",
                                Json::Arr(
                                    g.rows
                                        .iter()
                                        .map(|r| Json::obj(vec![("key", s(&r.key)), ("title", s(&r.title)), ("detail", s(&r.detail)), ("bytes", n(r.bytes as f64)), ("size", s(&hb(r.bytes))), ("saved", Json::Bool(r.saved))]))
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// The player actions and their keys (GTK shortcuts.rs). Keys are GTK accelerator names ("space", "<Shift>Left").
const ACTIONS: &[(&str, &str, &str)] = &[
    ("play_pause", "Play / Pause", "space"),
    ("seek_back", "Back 10 seconds", "Left"),
    ("seek_forward", "Forward 10 seconds", "Right"),
    ("seek_back_big", "Back 60 seconds", "<Shift>Left"),
    ("seek_forward_big", "Forward 60 seconds", "<Shift>Right"),
    ("skip", "Skip ahead (the Skip button)", "s"),
    ("volume_up", "Volume up", "Up"),
    ("volume_down", "Volume down", "Down"),
    ("mute", "Mute", "m"),
    ("fullscreen", "Fullscreen", "f"),
    ("speed_up", "Faster", "bracketright"),
    ("speed_down", "Slower", "bracketleft"),
    ("speed_reset", "Normal speed", "BackSpace"),
    ("sub_delay_less", "Subtitles earlier", "z"),
    ("sub_delay_more", "Subtitles later", "x"),
    ("next_episode", "Next episode", "n"),
    ("autocard", "Autocard: picture, sentence audio and context to the last Anki card", "<Control>m"),
    ("back", "Leave the player (or fullscreen)", "Escape"),
];

fn key_for(c: &Core, id: &str) -> String {
    let def = ACTIONS.iter().find(|a| a.0 == id).map(|a| a.2).unwrap_or("");
    c.settings.general.shortcuts.iter().find(|(a, _)| a == id).map(|(_, k)| k.clone()).unwrap_or_else(|| def.to_string())
}

/// "Shift + Left" from "<Shift>Left".
fn key_label(accel: &str) -> String {
    let mut mods = Vec::new();
    let mut rest = accel;
    while let Some(i) = rest.find('>') {
        if !rest.starts_with('<') {
            break;
        }
        let m = &rest[1..i];
        mods.push(match m {
            "Control" | "Primary" => "Ctrl",
            "Shift" => "Shift",
            "Alt" => "Alt",
            "Super" => "Super",
            other => other,
        });
        rest = &rest[i + 1..];
    }
    let key = match rest {
        "space" => "Space".to_string(),
        "bracketleft" => "[".to_string(),
        "bracketright" => "]".to_string(),
        "BackSpace" => "Backspace".to_string(),
        "comma" => ",".to_string(),
        "period" => ".".to_string(),
        "minus" => "-".to_string(),
        "equal" => "=".to_string(),
        "slash" => "/".to_string(),
        "Page_Up" => "Page Up".to_string(),
        "Page_Down" => "Page Down".to_string(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k.to_string(),
    };
    mods.push(&key);
    mods.join(" + ")
}

fn shortcuts_json(c: &Core) -> Json {
    Json::Arr(
        ACTIONS
            .iter()
            .map(|(id, label, def)| {
                let k = key_for(c, id);
                Json::obj(vec![("id", s(id)), ("label", s(label)), ("key", s(&k)), ("keyLabel", s(&key_label(&k))), ("default", s(def))])
            })
            .collect(),
    )
}

const MOVIE_GENRES: &[&str] = &[
    "Action", "Adventure", "Animation", "Biography", "Comedy", "Crime", "Documentary", "Drama", "Family", "Fantasy", "History", "Horror", "Music", "Mystery", "Romance", "Sci-Fi", "Sport", "Thriller", "War", "Western",
];

fn state_categories() -> Json {
    let list = |v: Vec<&str>| Json::Arr(v.into_iter().map(|x| s(x)).collect());
    Json::obj(vec![
        ("anime", list(reelhouse::anilist::GENRES.iter().chain(reelhouse::anilist::TAGS.iter()).copied().collect())),
        ("movie", list(MOVIE_GENRES.to_vec())),
        ("series", list(MOVIE_GENRES.to_vec())),
    ])
}

/// Preferences: the settings file as it is saved, plus what the pages show next to it.
/// The size the window was drawn with at start (Qt reads it once, before the window opens).
static START_SCALE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();

/// The Suwayomi server of the Images mode: status, repositories and the extensions they offer (only while that mode is on).
fn state_suwayomi(c: &Core) -> Json {
    if c.settings.general.mode != "images" {
        return Json::Null;
    }
    Json::obj(vec![
        ("status", s(&c.suwayomi_status)),
        ("repos", Json::Arr(c.settings.general.suwayomi_repos.iter().map(|r| s(r)).collect())),
        (
            "exts",
            Json::Arr(
                c.suwayomi_exts
                    .iter()
                    .map(|e| {
                        Json::obj(vec![
                            ("pkg", s(&e.pkg)),
                            ("name", s(&e.name)),
                            ("lang", s(&e.lang)),
                            ("code", s(&reelhouse::manga::lang_code(&e.lang))),
                            ("langName", s(&reelhouse::util::lang_name(&e.lang))),
                            ("installed", Json::Bool(e.installed)),
                            ("update", Json::Bool(e.update)),
                            ("nsfw", Json::Bool(e.nsfw)),
                            ("icon", s(&e.icon)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn state_prefs(c: &Core) -> Json {
    let mut j = c.settings.to_json();
    j.set("uiLang", s(&c.ui_lang()));
    let st = reelhouse::anime4k::status(&c.rt.shaders_dir);
    let addons: Vec<Json> = c.settings.addons.iter().map(|a| Json::obj(vec![("url", s(&a.url)), ("name", s(&a.name())), ("enabled", Json::Bool(a.enabled))])).collect();
    j.set(
        "extra",
        Json::obj(vec![
            ("dataDir", s(&c.data_dir.display().to_string())),
            ("player", s(&if let Some(e) = &c.mpv_error { e.clone() } else if c.rt.has_builtin_mpv() { "mpv is built into this program".to_string() } else { "Using the system mpv library".to_string() })),
            ("engine", s(if c.rt.rqbit.is_some() { "Built in" } else { "Downloaded automatically the first time you play a torrent" })),
            ("debugFile", s(&reelhouse::debug::path().map(|p| p.display().to_string()).unwrap_or_default())),
            ("debugDir", s(&reelhouse::debug::dir().map(|p| p.display().to_string()).unwrap_or_default())),
            ("shadersPresent", n(st.present as f64)),
            ("shadersTotal", n(st.total as f64)),
            ("modeInfo", s(reelhouse::anime4k::mode_info(&c.settings.upscaling.mode))),
            ("modes", Json::Arr(reelhouse::anime4k::MODES.iter().map(|x| s(x)).collect())),
            ("malLogged", Json::Bool(c.mal_logged_in())),
            ("malListInfo", s(&c.mal_list_info)),
            ("anisync", Json::Bool(c.settings.addons.iter().any(|a| a.url.contains(reelhouse::settings::ANISYNC_HOST) && a.enabled))),
            ("backupDir", s(&reelhouse::backup::backup_dir(&c.data_dir, &c.settings.backup.folder).display().to_string())),
            ("backupName", s(&reelhouse::backup::file_name(reelhouse::util::now_unix(), reelhouse::backup::PREFIX))),
            ("cloud", c.cloud_state()),
            ("update", c.update_state()),
            ("accents", Json::Arr(reelhouse::theme::PRESETS.iter().map(|(h, l)| Json::obj(vec![("hex", s(h)), ("name", s(l))])).collect())),
            ("addons", Json::Arr(addons)),
            ("extensionsDir", s(&c.extensions_folder().display().to_string())),
            (
                "extensions",
                Json::Arr(
                    c.app_exts
                        .iter()
                        .map(|x| {
                            Json::obj(vec![
                                ("id", s(&x.id)),
                                ("name", s(&x.name)),
                                ("version", s(&x.version)),
                                ("description", s(&x.description)),
                                ("hooks", Json::Arr(x.hooks.iter().map(|h| s(h)).collect())),
                                ("error", s(&x.error)),
                                ("enabled", Json::Bool(c.extension_enabled(x))),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("shortcuts", shortcuts_json(c)),
            ("uiScaleNow", n(*START_SCALE.get_or_init(|| c.settings.general.ui_scale) as f64)),
            ("suwayomi", state_suwayomi(c)),
            ("translators", c.translators_json()),
            ("mangaLangs", Json::Arr(reelhouse::manga::LANGS.iter().map(|(code, name)| Json::obj(vec![("code", s(code)), ("name", s(name))])).collect())),
        ]),
    );
    j
}

/// Changes one value of the settings file ("general.buffer_seconds") and saves.
fn set_setting(c: &mut Core, path: &str, value: Json) {
    let mut j = c.settings.to_json();
    let mut parts = path.splitn(2, '.');
    let (a, b) = (parts.next().unwrap_or(""), parts.next());
    match b {
        Some(rest) => {
            let mut sec = j.get(a).clone();
            if !matches!(sec, Json::Obj(_)) {
                sec = Json::obj(vec![]);
            }
            match rest.split_once('.') {
                Some((k2, k3)) => {
                    let mut inner = sec.get(k2).clone();
                    if !matches!(inner, Json::Obj(_)) {
                        inner = Json::obj(vec![]);
                    }
                    inner.set(k3, value);
                    sec.set(k2, inner);
                }
                None => sec.set(rest, value),
            }
            j.set(a, sec);
        }
        None => j.set(a, value),
    }
    c.settings = reelhouse::settings::Settings::from_json(&j);
    c.save_settings();
    if path.starts_with("subtitles.style.") {
        c.apply_sub_style();
    }
    if path == "general.video_scale" {
        c.apply_video_scale();
    }
}

fn state_detail(c: &Core) -> Json {
    let Some(d) = c.detail.as_ref() else {
        return Json::obj(vec![("open", Json::Bool(false))]);
    };
    let streams = match &d.streams {
        Some(st) => Json::obj(vec![
            ("loading", Json::Bool(st.loading)),
            ("label", s(&st.label)),
            ("errors", Json::Arr(st.errors.iter().map(|e| s(e)).collect())),
            (
                "list",
                Json::Arr(
                    st.list.iter().map(stream_json).collect(),
                ),
            ),
        ]),
        None => Json::Null,
    };
    let mut seasons: Vec<i64> = d.meta.videos.iter().filter_map(|v| v.season).collect();
    seasons.sort();
    seasons.dedup();
    let mut meta = item_json(c, &d.meta, true);
    // Episodes: on disk, downloading or online, like the pills of the GTK page.
    if let Json::Arr(vs) = meta.get("videos").clone() {
        let with_state: Vec<Json> = vs
            .into_iter()
            .zip(d.meta.videos.iter())
            .map(|(mut j, v)| {
                let (state, pct) = match c.episode_state(v) {
                    reelhouse::downloads::EpState::Saved(_) => ("saved", 100.0),
                    reelhouse::downloads::EpState::Downloading(p) => ("downloading", p),
                    reelhouse::downloads::EpState::Online => ("online", 0.0),
                };
                j.set("state", s(state));
                j.set("percent", n(pct));
                j
            })
            .collect();
        meta.set("videos", Json::Arr(with_state));
    }
    if let Some(r) = d.rating.as_ref().filter(|r| !r.is_empty()) {
        meta.set("rating", s(r));
    }
    let times = match d.meta.season_times(d.season, &|id| c.is_watched(id)) {
        Some(t) => Json::obj(vec![
            ("remaining", n(t.remaining as f64)),
            ("left", s(&if t.remaining == 0 { "Season finished".to_string() } else { format!("{} left", reelhouse::util::fmt_minutes(t.remaining)) })),
            ("season", s(&format!("Season {}", reelhouse::util::fmt_minutes(t.season)))),
            ("series", if t.series != t.season { s(&format!("All seasons {}", reelhouse::util::fmt_minutes(t.series))) } else { Json::Null }),
        ]),
        None => Json::Null,
    };
    Json::obj(vec![
        ("open", Json::Bool(true)),
        // Where the user stopped in this title (Continue button): index into the progress list, episode and fraction.
        (
            "progress",
            match c.progress.iter().position(|p| p.meta.id == d.meta.id && p.position > 0.0) {
                Some(i) => {
                    let p = &c.progress[i];
                    Json::obj(vec![("index", n(i as f64)), ("label", s(&p.label)), ("position", n(p.position)), ("fraction", n(p.fraction()))])
                }
                None => Json::Null,
            },
        ),
        ("loading", Json::Bool(d.loading)),
        ("season", n(d.season as f64)),
        ("seasons", Json::Arr(seasons.into_iter().map(|x| n(x as f64)).collect())),
        ("meta", meta),
        ("streams", streams),
        ("selectedKey", s(&d.streams.as_ref().map(|x| x.key.clone()).unwrap_or_default())),
        ("times", times),
        ("blurUnwatched", Json::Bool(c.settings.general.blur_unwatched)),
        ("genreChips", genre_chips(&d.meta, &d.mal)),
        ("mal", mal_json(c, &d.mal)),
        // "Cover and background" (GTK artdialog): what is shown now, what the addon gave, and where custom pictures go.
        (
            "art",
            Json::obj(vec![
                ("poster", opt(&c.poster_for(&d.meta))),
                ("background", opt(&c.background_for(&d.meta))),
                ("origPoster", opt(&d.meta.poster)),
                ("origBackground", opt(&d.meta.background)),
                ("override", Json::Bool(c.has_art_override(&d.meta.id))),
                ("dir", s(&c.data_dir.join("art").display().to_string())),
            ]),
        ),
    ])
}

fn state_player(b: &Bridge, c: &Core) -> Json {
    let Some(sess) = c.session.as_ref() else {
        return Json::obj(vec![("active", Json::Bool(false)), ("mpvError", s(c.mpv_error.as_deref().unwrap_or("")))]);
    };
    let (stage, error) = match &sess.stage {
        Stage::Resolving => ("resolving", String::new()),
        Stage::Loading => ("loading", String::new()),
        Stage::Buffering => ("buffering", String::new()),
        Stage::Playing => ("playing", String::new()),
        Stage::Failed(e) => ("failed", e.clone()),
    };
    let mut o = vec![
        ("active", Json::Bool(true)),
        ("title", s(&sess.title)),
        ("stage", s(stage)),
        ("error", s(&error)),
        ("message", s(&sess.resolve_msg)),
        ("subMessage", s(&sess.sub_msg)),
    ];
    let cur_res = sess.streams.get(sess.cur).map(|x| x.res).unwrap_or(0);
    o.push(("cur", n(sess.cur as f64)));
    o.push(("streams", Json::Arr(sess.streams.iter().map(stream_json).collect())));
    o.push((
        "qualities",
        Json::Arr(
            reelhouse::streams::quality_options(&sess.streams)
                .iter()
                .map(|q| Json::obj(vec![("label", s(&if q.count > 1 { format!("{}  ({} sources)", q.label, q.count) } else { q.label.clone() })), ("index", n(q.stream_idx as f64)), ("current", Json::Bool(q.res == cur_res))]))
                .collect(),
        ),
    ));
    o.push((
        "subCands",
        Json::Arr(
            sess.sub_cands
                .iter()
                .enumerate()
                .map(|(i, x)| {
                    let base = if x.lang.is_empty() { "und".to_string() } else { reelhouse::util::lang_base(&x.lang) };
                    Json::obj(vec![("index", n(i as f64)), ("lang", s(&x.lang)), ("langKey", s(&base)), ("langName", s(&reelhouse::util::lang_name(&x.lang))), ("addon", s(&x.addon))])
                })
                .collect(),
        ),
    ));
    // The loading screen shows the wallpaper and logo of the title (GTK session_changed), and what is going on.
    if let Some(d) = c.detail.as_ref() {
        o.push(("loadingBg", opt(&c.background_for(&d.meta).or_else(|| c.poster_for(&d.meta)))));
        o.push(("loadingLogo", opt(&d.meta.logo)));
    }
    let net = match (&c.torrent_stats, sess.torrent.is_some()) {
        (Some(t), true) => format!(
            "Peers {} · {}/s · {:.1} % · {} of {}",
            t.peers,
            reelhouse::torrent::human_bytes(t.speed as u64),
            t.percent(),
            reelhouse::torrent::human_bytes(t.progress),
            reelhouse::torrent::human_bytes(t.total)
        ),
        _ => String::new(),
    };
    let waited = sess.started.elapsed().as_secs();
    let (line, sub) = match &sess.stage {
        Stage::Resolving => (
            if sess.resolve_msg.is_empty() { "Finding the video source".to_string() } else { sess.resolve_msg.clone() },
            format!("{} s waiting. A slow start usually means few peers or a slow connection to them.", waited),
        ),
        Stage::Loading => ("Opening the video".to_string(), format!("{} s · {}", waited, net)),
        Stage::Buffering => {
            let cache = c.mpv.as_ref().and_then(|m| m.get_f64("demuxer-cache-duration")).unwrap_or(0.0);
            let subtxt = if !c.settings.subtitles.enabled { String::new() } else if sess.sub_msg.is_empty() { "Subtitles: looking".to_string() } else { format!("Subtitles: {}", sess.sub_msg) };
            (
                format!("Buffering: {:.0} of {} seconds", cache, c.settings.general.buffer_seconds),
                [format!("{} s", waited), net.clone(), subtxt].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n"),
            )
        }
        _ => (String::new(), String::new()),
    };
    o.push(("statusLine", s(&line)));
    o.push(("statusSub", s(&sub)));
    o.push(("thumbnails", Json::Bool(c.settings.general.thumbnails)));
    // Statistics popover (GTK fill_stats).
    let mut stats: Vec<Json> = Vec::new();
    if c.is_torrent_session() {
        match &c.torrent_stats {
            Some(t) => {
                stats.push(Json::Arr(vec![s("Peers"), s(&t.peers.to_string())]));
                stats.push(Json::Arr(vec![s("Speed"), s(&format!("{}/s", reelhouse::torrent::human_bytes(t.speed as u64)))]));
                stats.push(Json::Arr(vec![s("Completed"), s(&format!("{:.2} %", t.percent()))]));
                stats.push(Json::Arr(vec![s("Downloaded"), s(&format!("{} of {}", reelhouse::torrent::human_bytes(t.progress), reelhouse::torrent::human_bytes(t.total)))]));
            }
            None => stats.push(Json::Arr(vec![s("Torrent"), s("waiting for numbers")])),
        }
    } else {
        stats.push(Json::Arr(vec![s("Source"), s("direct stream")]));
    }
    for (k, v) in c.player_stats() {
        stats.push(Json::Arr(vec![s(&k), s(&v)]));
    }
    o.push(("stats", Json::Arr(stats)));
    o.push(("infoHash", s(&c.info_hash().unwrap_or_default())));
    // Subtitle layout (Subtitles menu, Settings column).
    let st = &c.settings.subtitles.style;
    o.push(("subPos", n(st.position as f64)));
    o.push(("subAlign", s(&st.align_x)));
    o.push(("subMargin", n(st.margin_x as f64)));
    o.push(("targetLang", s(&c.settings.target_lang(&c.system_lang))));
    // Forced translation.
    o.push(("translating", match &c.translating { Some(t) => n(t.pct as f64), None => Json::Null }));
    o.push((
        "translateNote",
        match &c.translate_note {
            Some((ok, t)) => Json::obj(vec![("ok", Json::Bool(*ok)), ("text", s(t))]),
            None => Json::Null,
        },
    ));
    // Karaoke.
    let (variants, forced) = c.karaoke_variants();
    let g = &c.settings.general;
    o.push((
        "karaoke",
        Json::obj(vec![
            ("variants", Json::Arr(variants.iter().map(|v| s(v)).collect())),
            ("forced", forced.map(|i| n(i as f64)).unwrap_or(Json::Null)),
            ("status", s(&c.karaoke_status())),
            ("detail", s(&c.karaoke_info().0)),
            ("songKey", s(&c.karaoke_song_delay().map(|x| x.0).unwrap_or_default())),
            ("songDelay", n(c.karaoke_song_delay().map(|x| x.1).unwrap_or(0.0))),
            ("on", Json::Bool(g.karaoke)),
            ("top", Json::Bool(g.karaoke_top)),
            ("size", n(g.karaoke_size as f64)),
            ("margin", n(g.karaoke_margin as f64)),
            ("color", s(&g.karaoke_color)),
            ("font", s(&g.karaoke_font)),
            ("bold", s(&g.karaoke_bold)),
            ("outline", n(g.karaoke_outline as f64)),
            ("shadow", n(g.karaoke_shadow as f64)),
            ("unfilled", s(&g.karaoke_unfilled)),
            ("italic", s(&g.karaoke_italic)),
            ("underline", s(&g.karaoke_underline)),
            ("outlineColor", s(&g.karaoke_outline_color)),
            ("backColor", s(&g.karaoke_back_color)),
            ("scaleX", n(g.karaoke_scale_x as f64)),
            ("scaleY", n(g.karaoke_scale_y as f64)),
            ("spacing", n(g.karaoke_spacing as f64)),
            ("align", n(g.karaoke_align as f64)),
            ("marginL", n(g.karaoke_margin_l as f64)),
            ("marginR", n(g.karaoke_margin_r as f64)),
            ("border", s(&g.karaoke_border)),
            ("delay", n(g.karaoke_delay)),
        ]),
    ));
    // Episodes for the side panel.
    if let Some(d) = c.detail.as_ref() {
        let season = sess.ep.season.unwrap_or_else(|| d.meta.seasons().first().copied().unwrap_or(1));
        let eps: Vec<Json> = d
            .meta
            .episodes(season)
            .iter()
            .map(|v| {
                Json::obj(vec![
                    ("id", s(&v.id)),
                    ("season", v.season.map(|x| n(x as f64)).unwrap_or(Json::Null)),
                    ("episode", v.episode.map(|x| n(x as f64)).unwrap_or(Json::Null)),
                    ("title", s(&v.title)),
                    ("thumbnail", opt(&v.thumbnail)),
                    ("watched", Json::Bool(c.is_watched(&v.id))),
                    ("current", Json::Bool(v.id == sess.sid)),
                ])
            })
            .collect();
        o.push(("episodes", Json::Arr(eps)));
        // Episodes that have no picture of their own (new releases) show the picture of the series instead.
        o.push(("episodeFallback", match c.background_for(&d.meta).or_else(|| c.poster_for(&d.meta)) { Some(u) => Json::Str(u), None => Json::Null }));
    }
    o.push(("blurUnwatched", Json::Bool(c.settings.general.blur_unwatched)));
    o.push(("marks", Json::Arr(c.seek_marks().into_iter().map(n).collect())));
    o.push(("hasNext", Json::Bool(c.has_next())));
    o.push(("isSeries", Json::Bool(sess.kind == "series")));
    o.push(("skipSeconds", n(c.settings.general.skip_seconds as f64)));
    o.push(("autoSkip", Json::Bool(c.settings.anime_skip.auto_skip)));
    o.push(("upscaling", Json::Bool(c.settings.upscaling.enabled)));
    o.push(("upscaleMode", s(&c.settings.upscaling.mode)));
    o.push(("upscaleModes", Json::Arr(reelhouse::anime4k::MODES.iter().map(|x| s(x)).collect())));
    o.push(("videoScale", s(&c.settings.general.video_scale)));
    o.push(("speed", n(c.speed())));
    o.push(("subDelay", n(c.sub_delay())));
    o.push((
        "skip",
        match c.current_skip() {
            Some((label, to)) => Json::obj(vec![("label", s(&label)), ("to", n(to))]),
            None => Json::Null,
        },
    ));
    // the opening / ending playing now, with its video on YouTube (see controller_theme). The lookup runs in the
    // background: nothing is sent while it searches, nor when it failed (the owner asked for no "searching" card),
    // so the card appears only when the song and its video are ready to play.
    o.push((
        "theme",
        match c.theme_card() {
            Some((kind, Some(Ok(v)))) => Json::obj(vec![
                ("kind", s(kind)),
                ("loading", Json::Bool(false)),
                ("song", s(&v.theme.song)),
                ("artist", s(&v.theme.artist)),
                ("number", n(v.theme.number as f64)),
                ("title", s(&v.title)),
                ("channel", s(&v.channel)),
                ("duration", n(v.duration)),
                ("thumbnail", s(&v.thumbnail())),
                ("url", s(&v.url())),
            ]),
            _ => Json::Null,
        },
    ));
    o.push((
        "next",
        match b.next.lock().ok().and_then(|g| g.clone()) {
            Some(nc) => Json::obj(vec![("title", s(&nc.title)), ("secondsLeft", n(nc.seconds_left)), ("thumbnail", opt(&nc.thumbnail))]),
            None => Json::Null,
        },
    ));
    let m = b.mpv.lock().ok().and_then(|g| g.clone());
    if let Some(m) = m {
        o.push(("pos", n(m.get_f64("time-pos").unwrap_or(0.0))));
        o.push(("subText", s(&m.get_str("sub-text").unwrap_or_default())));
        // times of the line on screen: the Editor opens on it (-1 when no line is shown)
        o.push(("subStart", n(m.get_f64("sub-start").unwrap_or(-1.0))));
        o.push(("subEnd", n(m.get_f64("sub-end").unwrap_or(-1.0))));
        o.push(("clipBusy", Json::Bool(c.clip_busy)));
        // study while watching: the questions on the seek bar, the one on screen, the notice before a part
        // without subtitles
        o.push(("studyMarks", c.study_marks_json()));
        o.push(("studyQuiz", c.study_quiz_json()));
        o.push(("studyNotice", s(&c.study_notice())));
        // the macaw's cue over the video ({n, cue, ms}: shh for a part without subtitles, peek at a mixed word)
        o.push(("studyCue", c.study_cue_json()));
        o.push((
            "flash",
            match c.flash.as_ref().filter(|(_, t)| t.elapsed().as_secs_f32() < 5.0) {
                Some((h, t)) => Json::obj(vec![("id", n(h.nid as f64)), ("term", s(&h.term)), ("reading", s(&h.reading)), ("meaning", s(&h.meaning)), ("left", n((5.0 - t.elapsed().as_secs_f32()) as f64))]),
                None => Json::Null,
            },
        ));
        o.push(("dur", n(m.get_f64("duration").unwrap_or(0.0))));
        o.push(("paused", Json::Bool(m.get_flag("pause").unwrap_or(false))));
        o.push(("muted", Json::Bool(m.get_flag("mute").unwrap_or(false))));
        o.push(("volume", n(m.get_f64("volume").unwrap_or(100.0))));
        o.push(("cache", n(m.get_f64("demuxer-cache-duration").unwrap_or(0.0))));
        o.push(("audio", tracks_json(&m, "audio")));
        o.push(("subs", tracks_json(&m, "sub")));
    }
    Json::obj(o)
}

fn state_settings(c: &Core) -> Json {
    let g = &c.settings.general;
    Json::obj(vec![
        ("language", s(&g.language)),
        // the language the interface is shown in, resolved ("pt"): what CoreBridge::uiLang gives QML
        ("uiLang", s(&c.ui_lang())),
        ("mode", s(&g.mode)),
        ("volume", n(g.volume)),
        ("hasTmdbKey", Json::Bool(!c.settings.metadata.tmdb_key.is_empty())),
        ("colorScheme", s(&g.color_scheme)),
        ("accent", s(&g.accent)),
        ("theme", s(&g.theme)),
        ("blurUnwatched", Json::Bool(g.blur_unwatched)),
        ("nightFilter", n(g.night_filter as f64)),
    ])
}

fn event_json(e: Event) -> Option<Json> {
    let plain = |name: &str| Some(Json::obj(vec![("e", s(name))]));
    match e {
        Event::Home => plain("home"),
        Event::Search => plain("search"),
        Event::Detail => plain("detail"),
        Event::Streams => plain("detail"),
        Event::Session => plain("player"),
        Event::Library => plain("library"),
        Event::Mal => plain("mal"),
        Event::Calendar => plain("calendar"),
        Event::Progress => plain("progress"),
        Event::Mode => plain("mode"),
        Event::PageText(..) | Event::Translated(..) | Event::TranslateProgress(..) | Event::Upscaled(..) | Event::UpscaleEngine => plain("manga"),
        Event::Toast(t) => Some(Json::obj(vec![("e", s("toast")), ("text", Json::Str(t))])),
        Event::Banner { kind, title, body, mood, action } => Some(Json::obj(vec![
            ("e", s("banner")),
            ("kind", Json::Str(kind)),
            ("title", Json::Str(title)),
            ("body", Json::Str(body)),
            ("mood", Json::Str(mood)),
            ("action", action.unwrap_or(Json::Null)),
        ])),
        Event::Info(k, t) => {
            let kind = match k {
                MsgKind::RealDebrid => "realdebrid",
                MsgKind::Translator => "translator",
                MsgKind::Shaders => "shaders",
                MsgKind::Addon => "addon",
                MsgKind::AnimeSkip => "animeskip",
                MsgKind::Mal => "mal",
                MsgKind::Backup => "backup",
                MsgKind::AnkiWeb => "ankiweb",
                MsgKind::Autocards => "autocards",
                MsgKind::Update => "update",
            };
            Some(Json::obj(vec![("e", s("info")), ("kind", s(kind)), ("text", Json::Str(t))]))
        }
        Event::Notices => plain("notices"),
        Event::MangaList | Event::MangaLibrary | Event::MangaView | Event::MangaPages | Event::MangaSources => plain("manga"),
        Event::Study => plain("study"),
        // the playback clock observed from mpv, as its own event (no state to fetch)
        Event::Playback(pos, paused) => Some(Json::obj(vec![("e", s("playback")), ("pos", n(pos)), ("paused", Json::Bool(paused))])),
        Event::XRay => plain("xray"),
        Event::NovelList | Event::NovelLibrary | Event::NovelView | Event::NovelText | Event::NovelPlugins => plain("manga"),
        Event::Thumb(t, p) => Some(Json::obj(vec![("e", s("thumb")), ("t", n(t)), ("path", Json::Str(p.display().to_string()))])),
        Event::Stats => plain("player"),
        Event::DlPick => plain("dlpick"),
        Event::Downloads => plain("downloads"),
        Event::Genre => plain("genre"),
        Event::Restored => plain("restored"),
        Event::Profiles => plain("profiles"),
        Event::OpenUrl(u) => Some(Json::obj(vec![("e", s("open_url")), ("url", Json::Str(u))])),
        // a line for reelhouse-qt.log (the `[live]` lines of Gemini Live)
        Event::Log(text) => Some(Json::obj(vec![("e", s("log")), ("text", Json::Str(text))])),
        Event::LensFrame(token, r, ms) => {
            let (frame, error) = match r {
                Ok(p) => (p, String::new()),
                Err(e) => (String::new(), e),
            };
            Some(Json::obj(vec![("e", s("lens_frame")), ("token", n(token as f64)), ("frame", Json::Str(frame)), ("error", Json::Str(error)), ("ms", n(ms as f64))]))
        }
        Event::OpenItem(m) => Some(Json::obj(vec![("e", s("open_item")), ("id", Json::Str(m.id)), ("kind", Json::Str(m.kind))])),
        _ => None,
    }
}

fn run_cmd(b: &Bridge, j: &Json) -> Json {
    let cmd = j.get("cmd").str();
    let mut ok = true;
    let mut msg = String::new();
    {
        let mut guard = match b.core.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let c = &mut guard.0;
        let mpv = b.mpv.lock().ok().and_then(|g| g.clone());
        // Study and Learn commands are the user at work: the study-time counter treats them as activity
        // (sounds are left out, as they also play by themselves when a card shows).
        if (cmd.starts_with("study_") || cmd.starts_with("learn_")) && !cmd.contains("sounds") && cmd != "learn_play" {
            c.time_input();
        }
        match cmd.as_str() {
            "load_home" => c.load_home(),
            // the "Offline mode" switch: the app uses only what is on disk, even with a connection
            "offline_mode" => c.set_offline_mode(j.get("on").as_bool().unwrap_or(false)),
            // ---- study time and the Pomodoro (controller_time.rs); `tz` = minutes east of UTC ----
            // a timing line from the QML (e.g. "[study] opened in N ms") for the debug report
            "perf_log" => reelhouse::debug::log(&j.get("what").str(), &j.get("msg").str()),
            "time_ping" => c.time_ping(j.get("focused").as_bool().unwrap_or(true), j.get("input").as_bool().unwrap_or(false), j.get("tz").as_i64()),
            "studytime_view" => c.studytime_view(&j.get("period").str(), j.get("offset").as_i64().unwrap_or(0), j.get("tz").as_i64()),
            "studytime_export" => c.studytime_export(std::path::Path::new(&j.get("path").str())),
            "pomo" => c.pomo_cmd(&j.get("op").str(), j.get("tz").as_i64()),
            "play_sound" => {
                let n = j.get("name").str();
                c.play_sound(if n.is_empty() { j.get("path").str() } else { n }.as_str())
            }
            "search" => c.run_search(&j.get("q").str()),
            "open_detail" => {
                if let Ok(mut f) = b.forward.lock() {
                    *f = None;
                }
                let item = item_from_json(j.get("item"));
                if let Ok(mut t) = b.trail.lock() {
                    match c.detail.as_ref().map(|d| d.meta.clone()) {
                        Some(cur) if cur.id != item.id => t.push(cur),
                        Some(_) => {}
                        None => t.clear(),
                    }
                }
                c.open_detail(item)
            }
            "set_art" => {
                let v = j.get("value").str();
                let id = match j.get("id").str() {
                    x if x.is_empty() => c.detail.as_ref().map(|d| d.meta.id.clone()).unwrap_or_default(),
                    x => x,
                };
                c.set_art(&id, &j.get("which").str(), if j.get("reset").as_bool().unwrap_or(false) { None } else { Some(v) });
            }
            "close_detail" => {
                let all = j.get("all").as_bool().unwrap_or(false);
                let prev = if all { None } else { b.trail.lock().ok().and_then(|mut t| t.pop()) };
                if let Some(m) = c.detail.as_ref().map(|d| d.meta.clone()) {
                    if let Ok(mut f) = b.forward.lock() {
                        *f = Some(m);
                    }
                }
                if all {
                    if let Ok(mut t) = b.trail.lock() {
                        t.clear();
                    }
                }
                match prev {
                    Some(p) => c.open_detail(p),
                    None => c.close_detail(),
                }
            }
            "go_forward" => {
                let m = b.forward.lock().ok().and_then(|mut f| f.take());
                match m {
                    Some(m) => c.open_detail(m),
                    None => ok = false,
                }
            }
            "set_season" => c.set_season(j.get("season").as_i64().unwrap_or(1)),
            "choose_episode" => c.choose_episode(&j.get("id").str(), &j.get("label").str()),
            "streams" => c.request_streams(&j.get("kind").str(), &j.get("id").str(), &j.get("label").str(), false),
            "play" => c.play_from_detail(j.get("index").as_u64().unwrap_or(0) as usize),
            "resume" => {
                let i = j.get("index").as_u64().unwrap_or(0) as usize;
                if let Some(pr) = c.progress.get(i).cloned() {
                    c.resume(pr);
                }
            }
            "toggle_favorite" => {
                let m = item_from_json(j.get("item"));
                c.toggle_favorite(&m);
            }
            "remove_progress" => c.remove_progress(&j.get("id").str()),
            "play_best" => {
                // A movie asks for its streams and starts the best one; a series does the same for the episode.
                if j.get("kind").str() == "movie" || j.get("videoId").str().is_empty() {
                    c.request_streams(&j.get("kind").str(), &j.get("id").str(), "", true);
                } else {
                    c.choose_episode_with(&j.get("videoId").str(), &j.get("label").str(), true);
                }
            }
            "toggle_library" => {
                let m = item_from_json(j.get("item"));
                c.toggle_library(&m);
            }
            "close_player" => c.close_player(),
            // Plays an address or a file directly (also used to test the player without an addon).
            "play_url" => {
                let st = reelhouse::streams::Stream { name: j.get("title").str(), url: Some(j.get("url").str()), ..Default::default() };
                // "kind"/"sid" are optional (testing aid: a local file played as a known title, for the X-Ray and songs)
                let kind = if j.get("kind").str().is_empty() { "movie".to_string() } else { j.get("kind").str() };
                c.start_play(vec![st], 0, j.get("title").str(), kind, j.get("sid").str(), Default::default());
            }
            "pause" => {
                if let Some(m) = &mpv {
                    let _ = m.cmd(&["cycle", "pause"]);
                }
            }
            "seek" => {
                if let Some(m) = &mpv {
                    // `exact`: frame-exact seek (the editor), even with hr-seek=no for normal seeks.
                    let flags = if j.get("exact").as_bool().unwrap_or(false) { "absolute+exact" } else { "absolute" };
                    let _ = m.cmd(&["seek", &format!("{}", j.get("pos").as_f64().unwrap_or(0.0)), flags]);
                }
            }
            // Explicit pause state (the editor must play / stop, not toggle).
            "set_pause" => {
                if let Some(m) = &mpv {
                    let _ = m.set("pause", if j.get("value").as_bool().unwrap_or(true) { "yes" } else { "no" });
                }
            }
            // mpv's own A-B loop: frame-exact jump from `b` back to `a`. Without `a` / `b` the loop is cleared.
            "ab_loop" => {
                if let Some(m) = &mpv {
                    match (j.get("a").as_f64(), j.get("b").as_f64()) {
                        (Some(a), Some(b)) if b > a => {
                            let _ = m.set("ab-loop-a", &format!("{:.3}", a));
                            let _ = m.set("ab-loop-b", &format!("{:.3}", b));
                        }
                        _ => {
                            let _ = m.set("ab-loop-a", "no");
                            let _ = m.set("ab-loop-b", "no");
                        }
                    }
                }
            }
            "volume" => {
                if let Some(m) = &mpv {
                    let _ = m.set("volume", &format!("{}", j.get("value").as_f64().unwrap_or(100.0)));
                }
            }
            "toggle_mute" => {
                if let Some(m) = &mpv {
                    let _ = m.cmd(&["cycle", "mute"]);
                }
            }
            "seek_relative" => {
                if let Some(m) = &mpv {
                    let _ = m.cmd(&["seek", &format!("{}", j.get("secs").as_f64().unwrap_or(0.0)), "relative"]);
                }
            }
            "set_audio" => {
                if let Some(m) = &mpv {
                    let _ = m.set("aid", &format!("{}", j.get("id").as_i64().unwrap_or(1)));
                }
            }
            "set_sub" => {
                // id 0 turns subtitles off.
                if let Some(m) = &mpv {
                    let id = j.get("id").as_i64().unwrap_or(0);
                    let _ = m.set("sid", &if id <= 0 { "no".to_string() } else { id.to_string() });
                }
            }
            "load_sub_cand" => c.load_sub_candidate(j.get("index").as_u64().unwrap_or(0) as usize),
            "sub_delay" => c.set_sub_delay(j.get("value").as_f64().unwrap_or(0.0)),
            "speed" => c.set_speed(j.get("value").as_f64().unwrap_or(1.0)),
            "video_scale" => c.set_video_scale(&j.get("mode").str()),
            "upscale" => {
                let mode = j.get("mode").str();
                if mode.is_empty() {
                    c.settings.upscaling.enabled = false;
                } else {
                    c.settings.upscaling.enabled = true;
                    c.settings.upscaling.mode = mode;
                }
                c.apply_upscaling_now();
                c.save_settings();
            }
            "switch_stream" => c.switch_stream(j.get("index").as_u64().unwrap_or(0) as usize),
            "retry_stream" => c.retry_stream(),
            "skip_segment" => c.do_skip(),
            "skip_forward" => c.skip_forward(),
            "next_episode" => c.play_next_now(),
            "prev_episode" => c.play_prev_now(),
            "dismiss_next" => c.dismiss_next(),
            // ---- title page: watched marks ----
            "toggle_watched" => c.toggle_watched(&j.get("id").str()),
            "mark" => {
                use reelhouse::controller::MarkScope;
                let id = j.get("id").str();
                let scope = match j.get("scope").str().as_str() {
                    "before" => MarkScope::Before(id),
                    "after" => MarkScope::After(id),
                    "all" => MarkScope::All,
                    _ => MarkScope::Season,
                };
                c.mark(scope, j.get("watched").as_bool().unwrap_or(true));
            }
            // ---- notifications ----
            "check_new_episodes" => {
                c.check_new_episodes();
                c.manga_refresh_library();
                c.novel_refresh_library();
                c.toast("Looking for new episodes…");
            }
            "clear_all_notices" => c.clear_all_notices(),
            // a banner that was shown (from the core or born in QML) goes into the bell too; one entry can be removed
            "note_banner" => c.note_banner(&j.get("kind").str(), &j.get("title").str(), &j.get("body").str(), &j.get("mood").str(), j.get("action")),
            "dismiss_notice" => c.clear_notice(&j.get("id").str()),
            "open_notice" => {
                let id = j.get("id").str();
                // New chapters: the series opens in its own mode.
                if let Some(sid) = id.strip_prefix("manga:") {
                    if let Some(sr) = c.manga.library.iter().find(|x| x.id == sid).cloned() {
                        c.set_mode("images");
                        c.manga_open(sr);
                    }
                } else if let Some(sid) = id.strip_prefix("novel:") {
                    if let Some(sr) = c.novels.library.iter().find(|x| x.id == sid).cloned() {
                        c.set_mode("text");
                        c.novel_open(sr);
                    }
                } else {
                    let item = c.saved_title(&id).unwrap_or_else(|| MetaItem { id: id.clone(), kind: "series".to_string(), name: j.get("title").str(), poster: j.get("poster").as_str().filter(|x| !x.is_empty()).map(|x| x.to_string()), ..Default::default() });
                    c.open_detail(item);
                }
            }
            // ---- categories ----
            "open_genre" => c.open_genre(j.get("id").as_u64().unwrap_or(0), &j.get("name").str(), &j.get("kind").str()),
            "genre_more" => c.genre_more(),
            // ---- Images mode ----
            "novel_browse" => {
                let what = match j.get("what").str().as_str() {
                    "latest" => reelhouse::manga::Browse::Latest,
                    "search" => reelhouse::manga::Browse::Search(j.get("text").str()),
                    _ => reelhouse::manga::Browse::Popular,
                };
                c.novel_browse(what)
            }
            "novel_more" => c.novel_more(),
            "novel_set_source" => c.novel_set_source(&j.get("id").str()),
            "novel_load_catalog" => c.novel_load_catalog(),
            "novel_install" => {
                let id = j.get("id").str();
                if let Some(p) = c.novel_catalog.iter().find(|p| p.id == id).cloned() {
                    c.novel_install(&p);
                }
            }
            "novel_remove_plugin" => c.novel_remove_plugin(&j.get("id").str()),
            "novel_open" => {
                if let Some(sr) = reelhouse::manga::series_from(j.get("series")) {
                    c.novel_open(sr);
                }
            }
            "novel_close" => c.novel_close(),
            "novel_toggle_library" => {
                if let Some(sr) = reelhouse::manga::series_from(j.get("series")) {
                    c.novel_toggle_library(&sr);
                }
            }
            "novel_read" => {
                let id = j.get("id").str();
                let found = c.novel_view.as_ref().and_then(|v| v.chapters.iter().find(|ch| ch.id == id).cloned());
                if let Some(ch) = found {
                    c.novel_read(ch);
                }
            }
            "novel_mark" => c.novel_mark(&j.get("series").str(), &j.get("id").str(), j.get("read").as_bool().unwrap_or(true)),
            "forget_page_text" => {
                c.reader_text.remove(&j.get("url").str());
            }
            "page_text" => {
                let url = j.get("url").str();
                let tiles: Vec<reelhouse::ocr::Tile> = match j.get("tiles") {
                    Json::Arr(v) => v
                        .iter()
                        .map(|t| reelhouse::ocr::Tile {
                            jpeg: b64_decode(&t.get("b64").str()),
                            w: t.get("w").as_u64().unwrap_or(0) as u32,
                            h: t.get("h").as_u64().unwrap_or(0) as u32,
                            y0: t.get("y0").as_f64().unwrap_or(0.0),
                            y1: t.get("y1").as_f64().unwrap_or(1.0),
                        })
                        .collect(),
                    _ => vec![],
                };
                if tiles.is_empty() {
                    c.on_page_text(url, Err("The page could not be downloaded to read its text.".to_string()));
                } else {
                    c.ocr_page(&url, move || Ok(tiles));
                }
            }
            "translate_texts" => {
                let target = j.get("target").str();
                let kind = j.get("kind").str();
                if target.is_empty() && kind.is_empty() {
                    c.translate_texts(j.get("id").as_u64().unwrap_or(0), j.get("texts").strings())
                } else {
                    let sub = &c.settings.subtitles;
                    let target = if !target.is_empty() { target } else if sub.target_lang.trim().is_empty() { c.ui_lang() } else { sub.target_lang.clone() };
                    let given = j.get("source").str();
                    let source = if given.is_empty() { reelhouse::ocr::source_code(&c.settings.general.ocr_lang).to_string() } else { given };
                    // "use" names the place for Preferences > Translators ("bubbles", "study_cards"…); without it the kind decides
                    let use_id = j.get("use").str();
                    if use_id.is_empty() {
                        c.translate_texts_kind(j.get("id").as_u64().unwrap_or(0), j.get("texts").strings(), &source, &target, &kind)
                    } else {
                        c.translate_texts_for(j.get("id").as_u64().unwrap_or(0), j.get("texts").strings(), &source, &target, &kind, &use_id)
                    }
                }
            }
            "upscale_page" => c.upscale_page(&j.get("url").str()),
            "upscale_install" => c.upscale_install(),
            "study_open" => c.study_open(j.get("id").as_i64().unwrap_or(0)),
            "study_close" => c.study_close(),
            "study_add_note" => c.study_add_note(j.get("did").as_i64().unwrap_or(0), j.get("mid").as_i64().unwrap_or(0), j.get("fields").strings()),
            "study_edit_note" => c.study_edit_note(j.get("nid").as_i64().unwrap_or(0), j.get("fields").strings()),
            "study_delete_note" => c.study_delete_note(j.get("nid").as_i64().unwrap_or(0)),
            "study_browse" => c.study_browse(j.get("did").as_i64().unwrap_or(0), j.get("q").str()),
            "study_browse_close" => c.study_browse_close(),
            "study_stats" => c.study_stats_open(Some(j.get("did").as_i64().unwrap_or(0))),
            "study_stats_close" => c.study_stats_open(None),
            "study_show" => c.study_show(),
            "study_card" => c.study_card_action(&j.get("what").str(), j.get("arg").as_i64().unwrap_or(0)),
            "clip_save" => c.clip_save(j.get("start").as_f64().unwrap_or(0.0), j.get("end").as_f64().unwrap_or(0.0), j.get("text").str()),
            // study while watching (studyplay.rs): answer, close / continue, play the line again
            "study_quiz_answer" => c.study_quiz_answer(j.get("id").as_f64().unwrap_or(0.0) as u64, &j.get("answer").str()),
            "study_quiz_skip" => c.study_quiz_skip(j.get("id").as_f64().unwrap_or(0.0) as u64),
            // a quiz card of the Text or Images reader: {text, lang ("" = study language), correct}
            "reader_study_answer" => c.reader_study_answer(&j.get("text").str(), &j.get("lang").str(), j.get("correct").as_bool().unwrap_or(false)),
            "study_quiz_replay" => c.study_quiz_replay(j.get("id").as_f64().unwrap_or(0.0) as u64),
            // a cut-out of a page of the Images reader, as a Study card
            "study_add_picture" => c.study_add_picture(&j.get("path").str(), &j.get("text").str(), &j.get("translation").str(), j.get("deck").as_i64().unwrap_or(0), &j.get("source").str()),
            "study_undo" => c.study_undo(),
            "extensions_reload" => c.extensions_reload(),
            "theme_play" => c.theme_play(&j),
            "theme_pause" => c.theme_pause(j.get("pause").as_bool()),
            "theme_seek" => c.theme_seek(j.get("pos").as_f64().unwrap_or(0.0)),
            "theme_volume" => c.theme_volume(j.get("volume").as_f64(), j.get("mute").as_bool()),
            "theme_stop" => c.theme_stop(j.get("resume").as_bool()),
            "xray_fetch" => c.xray_fetch(&j.get("source").str()),
            "xray_lens" => c.xray_lens(&j),
            "xray_person" => c.xray_person(&j.get("ref").str()),
            "xray_trivia" => c.xray_trivia(j.get("again").as_bool().unwrap_or(false)),
            "kaiwa_start" => c.kaiwa_start(&j),
            "kaiwa_send" => c.kaiwa_send(&j.get("text").str()),
            "kaiwa_retry" => c.kaiwa_retry(),
            "kaiwa_cancel" => c.kaiwa_cancel(),
            "kaiwa_release" => c.kaiwa_release(),
            "kaiwa_hint" => c.kaiwa_hint(),
            "kaiwa_word" => c.kaiwa_word(&j.get("word").str(), &j.get("sentence").str()),
            "kaiwa_card" => c.kaiwa_card(&j),
            "kaiwa_end" => c.kaiwa_end(),
            "kaiwa_open" => c.kaiwa_open(j.get("id").as_f64().unwrap_or(0.0) as i64),
            "kaiwa_delete" => c.kaiwa_delete(j.get("id").as_f64().unwrap_or(0.0) as i64),
            "kaiwa_speak" => c.kaiwa_speak(&j.get("text").str()),
            "kaiwa_listen" => c.kaiwa_listen(&j.get("path").str()),
            // Conversation by voice (Gemini Live)
            "kaiwa_live_talk" => c.kaiwa_live_talk(j.get("down").as_bool().unwrap_or(false)),
            "kaiwa_live_text" => c.kaiwa_live_text(&j.get("text").str()),
            "kaiwa_live_call" => c.kaiwa_live_call(),
            "kaiwa_live_stop" => c.kaiwa_live_stop(),
            "kaiwa_live_opts" => c.kaiwa_live_opts(&j),
            "kaiwa_annotate" => c.kaiwa_annotate(),
            "ollama_setup" => c.ollama_setup(),
            "ollama_status" => c.ollama_status(),
            "ollama_stop" => c.ollama_stop(),
            // the voice of the Study mode (controller_voice.rs) and Whisper on this computer
            "voice_setup" => c.whisper_setup(),
            "voice_stop" => c.whisper_stop(),
            "voice_status" => c.whisper_check(),
            "voice_test" => c.voice_test(&j.get("engine").str()),
            "voice_hear_test" => c.voice_hear_test(&j.get("path").str()),
            "voice_local_info" => c.voice_local_info(&j),
            "voice_local_failed" => c.voice_local_failed(&j.get("why").str()),
            "theme_dismiss" => c.theme_dismiss(),
            "pagedb_bubble" => {
                let b = reelhouse::pagedb::Bubble {
                    ocr: j.get("ocr").as_i64().unwrap_or(-1),
                    x: j.get("x").as_f64().unwrap_or(0.0),
                    y: j.get("y").as_f64().unwrap_or(0.0),
                    w: j.get("w").as_f64().unwrap_or(0.1),
                    h: j.get("h").as_f64().unwrap_or(0.1),
                    original: j.get("original").str(),
                    html: j.get("html").str(),
                    reviewed: j.get("reviewed").as_bool().unwrap_or(false),
                    deleted: false,
                    // the look of the bubble (font, size %, text colour, highlight style); checked in the core
                    font: j.get("font").str(),
                    size: j.get("size").as_f64().filter(|v| v.is_finite() && *v > 0.0).map(|v| v.round().min(1000.0) as u32).unwrap_or(0),
                    color: j.get("color").str(),
                    highlight: j.get("highlight").str(),
                };
                c.pagedb_set_bubble(j.get("page").as_u64().unwrap_or(1) as usize, j.get("index").as_i64().unwrap_or(-1), b)
            }
            "pagedb_remove_bubble" => c.pagedb_remove_bubble(j.get("page").as_u64().unwrap_or(1) as usize, j.get("index").as_u64().unwrap_or(0) as usize),
            "pagedb_ops" => c.pagedb_set_ops(j.get("page").as_u64().unwrap_or(1) as usize, &j.get("ops").str()),
            "pagedb_export" => c.pagedb_export(std::path::Path::new(&j.get("path").str())),
            "pagedb_import" => c.pagedb_import(std::path::Path::new(&j.get("path").str())),
            "autocard" => c.autocard_mine(),
            "feedback_add" => c.feedback_add(&j.get("text").str(), &j.get("place").str()),
            "feedback_open" => c.feedback_open(),
            "autocards_test" => c.autocards_test(),
            "extension_toggle" => c.extension_toggle(&j.get("id").str(), j.get("on").as_bool().unwrap_or(true)),
            "study_play_sounds" => {
                let names: Vec<String> = j.get("names").items().iter().map(|x| x.str()).collect();
                c.study_play_sounds(&names)
            }
            "study_stop_sounds" => c.study_stop_sounds(),
            "study_set_volume" => c.study_set_volume(j.get("volume").as_f64().unwrap_or(100.0).clamp(0.0, 100.0) as u32),
            "study_play_clip" => c.study_play_clip(&j.get("name").str()),
            "study_clip_pause" => c.study_clip_pause(j.get("pause").as_bool()),
            "study_stop_clip" => c.study_stop_clip(),
            "ankiweb_login" => c.ankiweb_login(&j.get("user").str(), &j.get("pass").str()),
            "ankiweb_logout" => c.ankiweb_logout(),
            "ankiweb_sync" => c.ankiweb_sync(),
            "study_unbury" => c.study_unbury(),
            // Learn (Duolingo-style lessons); `tz` is the window's time zone in minutes east of UTC
            "learn_open" => c.learn_open(j.get("tz").as_i64()),
            "learn_start" => c.learn_start(&j.get("lesson").str()),
            "learn_answer" => c.learn_answer(&j.get("answer").str(), &j.get("missed").strings()),
            "learn_continue" => c.learn_continue(),
            "learn_quit" => c.learn_quit(),
            "learn_practice" => c.learn_practice(&j.get("mode").str()),
            "learn_set_goal" => c.learn_set_goal(j.get("xp").as_u64().unwrap_or(20) as u32),
            "learn_buy" => c.learn_buy(&j.get("item").str()),
            "learn_set_course" => c.learn_set_course(&j.get("lang").str()),
            "learn_start_from" => c.learn_start_from(&j.get("lesson").str()),
            "learn_play" => c.learn_play(&j.get("url").str(), j.get("slow").as_bool().unwrap_or(false)),
            "learn_chest" => c.learn_chest(),
            "study_answer" => c.study_answer(j.get("ease").as_i64().unwrap_or(0) as u8),
            "study_create_deck" => c.study_create_deck(&j.get("name").str()),
            "study_delete_deck" => c.study_delete_deck(j.get("id").as_i64().unwrap_or(0)),
            "study_rename_deck" => c.study_rename_deck(j.get("id").as_i64().unwrap_or(0), &j.get("name").str()),
            "study_import" => c.study_import(std::path::PathBuf::from(j.get("path").str())),
            // AnkiWeb's shared decks inside Study (state "shared")
            "shared_open" => c.shared_open(),
            "shared_close" => c.shared_close(),
            "shared_search" => c.shared_search(&j.get("q").str(), &j.get("cat").str(), j.get("page").as_u64().unwrap_or(0) as usize, &j.get("sort").str()),
            "shared_info" => c.shared_info(j.get("id").as_u64().unwrap_or(0) as u32),
            "shared_download" => c.shared_download(j.get("id").as_u64().unwrap_or(0) as u32),
            "shared_play" => c.shared_play(&j.get("url").str()),
            "novel_import" => c.novel_import(std::path::PathBuf::from(j.get("path").str())),
            "novel_save_edit" => c.novel_save_edit(&j.get("text").str()),
            "novel_reset_edit" => c.novel_reset_edit(),
            "novel_new_book" => {
                if let Some(sr) = c.novel_new_book(&j.get("title").str()) {
                    c.novel_open(sr);
                }
            }
            "novel_add_chapter" => c.novel_add_chapter(&j.get("title").str()),
            "novel_mark_all" => c.novel_mark_all(j.get("read").as_bool().unwrap_or(true)),
            "novel_mark_before" => c.novel_mark_before(&j.get("id").str()),
            "novel_progress" => c.novel_progress(j.get("permille").as_u64().unwrap_or(0) as u32),
            "novel_finished" => c.novel_finished(),
            "novel_close_reader" => c.novel_close_reader(),
            "novel_next" => {
                let delta = j.get("delta").as_i64().unwrap_or(1) as i32;
                if let Some(ch) = c.novel_neighbour(delta) {
                    c.novel_read(ch);
                }
            }
            "set_mode" => c.set_mode(&j.get("mode").str()),
            // The window is up (time zone in minutes east of UTC, interface language): the macaw greets once a day.
            "banner_hello" => c.banner_hello(j.get("tz").as_i64(), &j.get("lang").str()),
            // An example study reminder (Preferences): kind streak | goal | review | quest | idle
            "banner_test" => c.banner_test(&j.get("kind").str()),
            "manga_browse" => {
                let what = match j.get("what").str().as_str() {
                    "latest" => reelhouse::manga::Browse::Latest,
                    "search" => reelhouse::manga::Browse::Search(j.get("text").str()),
                    _ => reelhouse::manga::Browse::Popular,
                };
                c.manga_browse(what)
            }
            "manga_more" => c.manga_more(),
            "manga_set_source" => c.manga_set_source(&j.get("id").str()),
            "manga_open" => {
                if let Some(sr) = reelhouse::manga::series_from(j.get("series")) {
                    c.manga_open(sr);
                }
            }
            "manga_close" => c.manga_close(),
            "manga_set_categories" => c.manga_set_categories(&j.get("id").str(), j.get("cats").strings()),
            "manga_set_cover" | "novel_set_cover" => {
                if let Err(e) = c.set_series_cover(cmd.starts_with("novel"), &j.get("id").str(), &j.get("file").str()) {
                    ok = false;
                    msg = e;
                }
            }
            "manga_add_category" => c.manga_add_category(&j.get("name").str()),
            "manga_remove_category" => c.manga_remove_category(&j.get("name").str()),
            "manga_mark_all" => c.manga_mark_all(j.get("read").as_bool().unwrap_or(true)),
            "manga_mark_before" => c.manga_mark_before(&j.get("id").str()),
            "manga_refresh_library" => c.manga_refresh_library(),
            // the Images mode was shown: the server of the app is started (or the one of the user is asked) and the library looked at
            "suwayomi_enter" => {
                if !c.suwayomi_ready() || (c.manga_sources.is_empty() && c.suwayomi_status.is_empty()) {
                    c.suwayomi_connect();
                }
                // at most every half hour: entering the mode does not ask every source again
                c.manga_refresh_library_due();
            }
            "suwayomi_restart" => {
                if c.settings.general.suwayomi_managed {
                    c.suwayomi_restart()
                } else {
                    c.suwayomi_connect()
                }
            }
            "suwayomi_add_repo" => {
                if let Err(e) = c.suwayomi_add_repo(&j.get("text").str()) {
                    c.suwayomi_status = e;
                }
            }
            "suwayomi_remove_repo" => c.suwayomi_remove_repo(&j.get("url").str()),
            "suwayomi_list_exts" => c.suwayomi_list_exts(),
            "suwayomi_install" => c.suwayomi_install(&j.get("pkg").str()),
            "suwayomi_install_many" => c.suwayomi_install_many(j.get("pkgs").strings()),
            "suwayomi_uninstall" => c.suwayomi_uninstall(&j.get("pkg").str()),
            "novel_set_categories" => c.novel_set_categories(&j.get("id").str(), j.get("cats").strings()),
            "novel_add_category" => c.novel_add_category(&j.get("name").str()),
            "novel_remove_category" => c.novel_remove_category(&j.get("name").str()),
            "manga_toggle_library" => {
                if let Some(sr) = reelhouse::manga::series_from(j.get("series")) {
                    c.manga_toggle_library(&sr);
                }
            }
            "manga_read" => {
                let id = j.get("id").str();
                let found = c.manga_view.as_ref().and_then(|v| v.chapters.iter().find(|x| x.id == id).cloned());
                if let Some(ch) = found {
                    c.manga_read(ch);
                }
            }
            "manga_mark" => {
                let (sid, cid) = (j.get("series").str(), j.get("id").str());
                c.manga_mark(&sid, &cid, j.get("read").as_bool().unwrap_or(true));
            }
            "manga_page" => c.manga_page(j.get("page").as_u64().unwrap_or(0) as u32),
            "manga_finished" => c.manga_finished(),
            "manga_close_reader" => c.manga_close_reader(),
            "manga_next" => {
                let delta = j.get("delta").as_i64().unwrap_or(1) as i32;
                if let Some(ch) = c.manga_neighbour(delta) {
                    c.manga_read(ch);
                }
            }
            "close_genre" => c.close_genre(),
            "open_card" => c.open_card(&card_from_json(j.get("card"))),
            // ---- calendar and library ----
            "load_calendar" => c.load_calendar(),
            "fill_library" => c.fill_library(),
            // ---- MyAnimeList on the INFO page ----
            "retry_mal" => c.retry_mal(),
            "set_my_status" => c.set_my_status(&j.get("status").str(), j.get("score").as_u64().unwrap_or(0) as u32, j.get("watched").as_u64().unwrap_or(0) as u32),
            "remove_my_status" => c.remove_my_status(),
            // ---- preferences ----
            // A sound of controller mode picked in Preferences: copied into <data>/sounds (so the backup keeps it)
            // and saved as general.sound_<slot> relative to the data folder; an empty path clears the slot.
            "set_ui_sound" => {
                let slot = j.get("slot").str();
                if matches!(slot.as_str(), "nav" | "select" | "music") {
                    let src = j.get("path").str();
                    let rel = if src.is_empty() {
                        reelhouse::uisounds::clear(&c.data_dir, &slot);
                        Ok(String::new())
                    } else {
                        reelhouse::uisounds::set_file(&c.data_dir, &slot, std::path::Path::new(&src))
                    };
                    match rel {
                        Ok(rel) => {
                            reelhouse::debug::log("sound", &format!("{} = {:?}", slot, rel));
                            set_setting(c, &format!("general.sound_{}", slot), Json::Str(rel));
                        }
                        Err(e) => {
                            reelhouse::debug::log("sound", &format!("{}: {}", slot, e));
                            c.toast(&e);
                        }
                    }
                }
            }
            // Preferences > Controller > Sounds, "Import sound pack…": a .zip or a folder with nav.*, select.*,
            // music.* (reelhouse::uisounds); each slot found is copied into <data>/sounds and set.
            "import_sound_pack" => {
                let src = j.get("path").str();
                match reelhouse::uisounds::import_pack(std::path::Path::new(&src), &c.data_dir) {
                    Ok(done) => {
                        for (slot, rel) in &done {
                            reelhouse::debug::log("sound", &format!("pack {:?}: {} = {}", src, slot, rel));
                            set_setting(c, &format!("general.sound_{}", slot), Json::Str(rel.clone()));
                        }
                        c.toast(if done.len() == 3 { "Sound pack imported" } else { "Sound pack imported (only some of the sounds were in it)" });
                    }
                    Err(e) => {
                        reelhouse::debug::log("sound", &format!("pack {:?}: {}", src, e));
                        c.toast(&e);
                    }
                }
            }
            // Android: the app's own extension host listens (docs/android-extensions.md)
            "ext_host" => c.ext_host_up(j.get("port").as_u64().unwrap_or(0) as u16, &j.get("dir").str()),
            // the cookies the in-app browser got after a site's bot check, for the text sources (sitepass.rs)
            "set_site_pass" => {
                reelhouse::sitepass::set(&j.get("url").str(), &j.get("cookie").str(), &j.get("ua").str());
            }
            "set_setting" => {
                set_setting(c, &j.get("path").str(), j.get("value").clone());
                if j.get("path").str().starts_with("upscaling.") && c.session.is_some() {
                    c.apply_upscaling_now();
                }
            }
            "toggle_addon" => {
                let url = j.get("url").str();
                let on = j.get("on").as_bool().unwrap_or(true);
                if let Some(x) = c.settings.addons.iter_mut().find(|x| x.url == url) {
                    x.enabled = on;
                }
                c.save_settings();
                c.load_home();
            }
            "install_addon" => c.install_addon(&j.get("url").str()),
            "remove_addon" => c.remove_addon(&j.get("url").str()),
            "test_real_debrid" => c.test_real_debrid(),
            "anime_skip_login" => c.anime_skip_login(&j.get("email").str(), &j.get("password").str()),
            "anime_skip_logout" => c.anime_skip_logout(),
            "mal_login" => c.mal_login(),
            "mal_finish_with" => c.mal_finish_with(&j.get("url").str()),
            "mal_logout" => c.mal_logout(),
            "refresh_mal_list" => c.refresh_mal_list(),
            "backup_now" => {
                let p = j.get("path").str();
                c.backup_now(if p.is_empty() { None } else { Some(std::path::PathBuf::from(p)) });
            }
            "restore_backup" => c.restore_backup(std::path::PathBuf::from(j.get("path").str())),
            // the copy of the backup online (controller_backup.rs)
            x if x.starts_with("cloud_") => c.cloud_cmd(x, j),
            // the update of the app (controller_update.rs)
            x if x.starts_with("update_") => c.update_cmd(x),
            "download_shaders" => c.download_shaders(),
            "save_settings" => c.save_settings(),
            // ---- background tasks and downloads ----
            "cancel_task" => c.cancel_task(&j.get("key").str()),
            "dl_open" => {
                c.dl_open();
                if c.dl_pick.as_ref().map(|p| p.seasons.is_empty()).unwrap_or(true) {
                    c.dl_close();
                    ok = false;
                    msg = "This title has no seasons to keep.".to_string();
                }
            }
            "dl_load_streams" => c.dl_load_streams(j.get("season").as_i64().unwrap_or(1)),
            "dl_start" => {
                let seasons: Vec<i64> = j.get("seasons").items().iter().filter_map(|x| x.as_i64()).collect();
                c.dl_start(j.get("index").as_u64().unwrap_or(0) as usize, seasons);
                c.dl_close();
                c.toast("Downloading in the background. Progress is in Preferences, Torrents.");
            }
            "dl_close" => c.dl_close(),
            "remove_torrent" => c.remove_torrent(&j.get("key").str()),
            "clear_torrent_cache" => {
                let m = c.clear_torrent_cache();
                c.toast(&m);
            }
            "test_translator" => c.test_translator(),
            // Preferences > Translators: one translator, or all that are ready, translate a sample (or "text")
            "translator_test" => c.translator_test(&j.get("id").str(), &j.get("text").str(), &j.get("target").str()),
            "translator_test_all" => c.translator_test_all(&j.get("text").str(), &j.get("target").str()),
            // ---- player extras ----
            "request_thumb" => c.request_thumb(j.get("t").as_f64().unwrap_or(0.0)),
            "refresh_stats" => c.refresh_stats(),
            "set_sub_layout" => c.set_sub_layout(j.get("pos").as_u64().unwrap_or(100) as u32, &j.get("align").str(), j.get("margin").as_u64().unwrap_or(0) as u32),
            "translate_subtitle" => {
                use reelhouse::controller::SubSource;
                let track = j.get("track").as_i64().unwrap_or(0);
                let src = if track > 0 {
                    SubSource::Embedded(track)
                } else if j.get("file").str().is_empty() {
                    SubSource::Candidate(j.get("index").as_u64().unwrap_or(0) as usize)
                } else {
                    SubSource::File(j.get("file").str(), j.get("lang").str())
                };
                c.translate_subtitle(src, &j.get("target").str());
            }
            "cancel_translation" => c.cancel_translation(),
            "karaoke_choose" => c.karaoke_choose(j.get("index").as_i64().filter(|x| *x >= 0).map(|x| x as usize)),
            "karaoke_set_song_delay" => c.karaoke_set_song_delay(&j.get("key").str(), j.get("value").as_f64().unwrap_or(0.0)),
            "karaoke_retry" => c.karaoke_retry(),
            "karaoke_on" => {
                let on = j.get("on").as_bool().unwrap_or(true);
                c.settings.general.karaoke = on;
                c.save_settings();
                if on {
                    c.karaoke_retry();
                }
            }
            "play_episode" => c.choose_episode_with(&j.get("id").str(), &j.get("label").str(), true),
            "upscale_mode" => {
                // Ctrl+0..6 in the player.
                let i = j.get("n").as_u64().unwrap_or(0) as usize;
                if i == 0 {
                    c.settings.upscaling.enabled = false;
                } else if i <= reelhouse::anime4k::MODES.len() {
                    c.settings.upscaling.enabled = true;
                    c.settings.upscaling.mode = reelhouse::anime4k::MODES[i - 1].to_string();
                }
                c.apply_upscaling_now();
                c.save_settings();
                c.toast(&if i == 0 { "Anime4K turned off".to_string() } else { format!("Anime4K mode {}", reelhouse::anime4k::MODES[(i - 1).min(5)]) });
            }
            // ---- shortcuts ----
            "set_shortcut" => {
                let id = j.get("id").str();
                let key = j.get("key").str();
                let clash = ACTIONS.iter().find(|a| a.0 != id && key_for(c, a.0) == key).map(|a| a.1);
                match clash {
                    Some(other) => {
                        ok = false;
                        msg = format!("That key is already used for “{}”.", other);
                    }
                    None => {
                        let def = ACTIONS.iter().find(|a| a.0 == id).map(|a| a.2).unwrap_or("");
                        c.settings.general.shortcuts.retain(|(a, _)| *a != id);
                        if key != def {
                            c.settings.general.shortcuts.push((id.clone(), key));
                        }
                        c.save_settings();
                    }
                }
            }
            "reset_shortcuts" => {
                c.settings.general.shortcuts.clear();
                c.save_settings();
            }
            "shader" => {
                // `on` turns Anime4K on or off like the preference does.
                c.apply_upscaling_now();
            }
            // Research mode and the player's quick search (src/controller_browser.rs)
            other if other.starts_with("browser_") => {
                if let Err(e) = c.browser_cmd(other, j) {
                    ok = false;
                    msg = e;
                }
            }
            // local profiles: profile_switch, profile_add, profile_rename, profile_avatar, profile_delete
            other if other.starts_with("profile_") => {
                if let Err(e) = c.profile_cmd(other, j) {
                    ok = false;
                    msg = e;
                }
            }
            other => {
                ok = false;
                msg = format!("unknown command: {}", other);
            }
        }
    }
    Json::obj(vec![("ok", Json::Bool(ok)), ("error", Json::Str(msg))])
}

unsafe fn text<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    CStr::from_ptr(p).to_str().unwrap_or("")
}

#[no_mangle]
pub extern "C" fn rh_new() -> *mut Bridge {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let core = Core::new();
        core.fire_hook("onStart", Json::obj(vec![("version", s(env!("CARGO_PKG_VERSION")))]));
        Box::into_raw(Box::new(Bridge { core: Mutex::new(SendCore(core)), mpv: Mutex::new(None), frame_cb: Mutex::new(None), next: Mutex::new(None), started: std::time::Instant::now(), last_minute: Mutex::new(None), last_stats: Mutex::new(None), first_check_done: std::sync::atomic::AtomicBool::new(false), forward: Mutex::new(None), trail: Mutex::new(Vec::new()) }))
    }));
    r.unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn rh_free(b: *mut Bridge) {
    if b.is_null() {
        return;
    }
    let b = Box::from_raw(b);
    // A panic must not cross into C++; a poisoned lock still shuts the core down.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        if let Ok(mut f) = b.frame_cb.lock() {
            *f = None;
        }
        b.core.lock().unwrap_or_else(|p| p.into_inner()).0.shutdown();
        drop(b);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn rh_free_str(p: *mut c_char) {
    if !p.is_null() {
        drop(CString::from_raw(p));
    }
}

#[no_mangle]
pub unsafe extern "C" fn rh_cmd(b: *mut Bridge, json: *const c_char) -> *mut c_char {
    let b = &*b;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match Json::parse(text(json)) {
        Ok(j) => run_cmd(b, &j),
        Err(e) => Json::obj(vec![("ok", Json::Bool(false)), ("error", Json::Str(e))]),
    }));
    out(r.unwrap_or_else(|_| Json::obj(vec![("ok", Json::Bool(false)), ("error", s("the command crashed"))])))
}

/// The slow jobs the GTK window ran from its clock: the debug report switch, Suwayomi, the automatic backup (about once a
/// minute) and the new-episode check (20 s after start, then once a minute; the core itself waits 6 h between real checks).
/// The connection state for the window (`e` names the event; empty for the "net" state).
fn net_json(e: &str) -> Json {
    use reelhouse::connectivity as net;
    let mut o = vec![("online", Json::Bool(net::online())), ("network", Json::Bool(net::network())), ("forced", Json::Bool(net::forced()))];
    if !e.is_empty() {
        o.insert(0, ("e", s(e)));
    }
    Json::obj(o)
}

fn periodic(b: &Bridge, c: &mut Core) {
    use std::sync::atomic::Ordering;
    // the offline mode follows the settings (also after a restored backup or another profile)
    reelhouse::connectivity::set_forced(c.settings.general.offline_mode);
    // The debug report is always on in the Qt version (its switch was removed from Preferences).
    reelhouse::debug::set_enabled(true, || c.debug_header());
    c.suwayomi_tick();
    // Torrent numbers for the loading screen and the Statistics menu, every two seconds while something plays.
    if c.session.is_some() {
        if let Ok(mut g) = b.last_stats.lock() {
            if g.map(|t| t.elapsed().as_millis() >= 2000).unwrap_or(true) {
                *g = Some(std::time::Instant::now());
                c.refresh_stats();
            }
        }
    }
    let video = c.mode() == "video";
    if video && !b.first_check_done.load(Ordering::Relaxed) && b.started.elapsed().as_secs() >= 20 {
        b.first_check_done.store(true, Ordering::Relaxed);
        c.notify_tick();
    }
    let due = match b.last_minute.lock() {
        Ok(mut g) => {
            let due = g.map(|t| t.elapsed().as_secs() >= 60).unwrap_or(true);
            if due {
                *g = Some(std::time::Instant::now());
            }
            due
        }
        Err(_) => false,
    };
    if due && b.started.elapsed().as_secs() >= 5 {
        c.auto_backup_tick();
        if video && b.first_check_done.load(Ordering::Relaxed) {
            c.notify_tick();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rh_poll(b: *mut Bridge) -> *mut c_char {
    let b = &*b;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut guard = match b.core.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let c = &mut guard.0;
        c.drive_session();
        periodic(b, c);
        // Progress, watched marks, the next episode (prefetch, autoplay and its card): what the GTK player tick did.
        let card = c.poll_playback();
        if let Ok(mut g) = b.next.lock() {
            *g = card;
        }
        let mut events: Vec<Json> = Vec::new();
        for e in c.poll() {
            match e {
                // A title found from a card (category, similar, notification) opens like any other.
                Event::OpenItem(m) => {
                    // Same bookkeeping as the "open_detail" command: Back returns to the page shown now, Forward is gone.
                    if let Ok(mut f) = b.forward.lock() {
                        *f = None;
                    }
                    if let Ok(mut t) = b.trail.lock() {
                        match c.detail.as_ref().map(|d| d.meta.clone()) {
                            Some(cur) if cur.id != m.id => t.push(cur),
                            Some(_) => {}
                            None => t.clear(),
                        }
                    }
                    c.open_detail(m);
                    events.push(Json::obj(vec![("e", s("detail"))]));
                    events.push(Json::obj(vec![("e", s("open_detail"))]));
                }
                Event::Raise => events.push(Json::obj(vec![("e", s("raise"))])),
                Event::Quit => events.push(Json::obj(vec![("e", s("quit"))])),
                other => {
                    if let Some(j) = event_json(other) {
                        events.push(j);
                    }
                }
            }
        }
        // The connection came or went (src/connectivity.rs): the window shows the offline state.
        static NET_SEEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
        let gen = reelhouse::connectivity::generation();
        if NET_SEEN.swap(gen, std::sync::atomic::Ordering::Relaxed) != gen {
            let online = reelhouse::connectivity::online();
            if gen != 0 {
                c.on_net_change(online);
            }
            events.push(net_json("net"));
        }
        Json::obj(vec![("events", Json::Arr(events)), ("playing", Json::Bool(c.session.is_some()))])
    }));
    out(r.unwrap_or_else(|_| Json::obj(vec![("events", Json::Arr(vec![]))])))
}

#[no_mangle]
pub unsafe extern "C" fn rh_state(b: *mut Bridge, name: *const c_char) -> *mut c_char {
    let b = &*b;
    let name = text(name).to_string();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // "mascot:<lang>:<moment>": one of the macaw's funny lines, random, not the last one ({text}; "" when unknown).
        // No core needed (src/mascot_lines.rs).
        if let Some(rest) = name.strip_prefix("mascot:") {
            let (lang, moment) = rest.split_once(':').unwrap_or(("", rest));
            return Json::obj(vec![("text", Json::Str(reelhouse::mascot_lines::say(lang, moment)))]);
        }
        // "net": is there internet, and did the user choose the offline mode ({online, network, forced})
        if name == "net" {
            return net_json("");
        }
        let mut guard = match b.core.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if name == "torrents" {
            return state_torrents(&mut guard.0);
        }
        // Research mode: "browser", "browser:address", "browser:frame" (its lists are read from disk on first use)
        if name.starts_with("browser") {
            return guard.0.browser_json(&name);
        }
        let c = &guard.0;
        match name.as_str() {
            "home" => state_home(c),
            "search" => state_search(c),
            "library" => state_library(c),
            "detail" => state_detail(c),
            "player" => state_player(b, c),
            "settings" => state_settings(c),
            "notices" => state_notices(c),
            "genre" => state_genre(c),
            "manga" => state_manga(c),
            "study" => state_study(c),
            "learn" => c.learn_json(),
            "shared" => c.shared_json(),
            "studytime" => c.studytime_json(),
            "pomodoro" => c.pomo_json(),
            "calendar" => state_calendar(c),
            "prefs" => state_prefs(c),
            "xray" => c.xray_json(),
            "theme_video" => c.theme_video_json(),
            "study_clip" => c.study_clip_json(),
            "kaiwa" => c.kaiwa_json(),
            "kaiwa_live" => c.kaiwa_live_json(),
            "kaiwa_status" => c.kaiwa_status_json(),
            "ollama" => c.ollama_json(),
            "voice" => c.voice_json(),
            "categories" => state_categories(),
            "tasks" => state_tasks(c),
            "dlpick" => state_dlpick(c),
            // the people who use the app on this computer (src/controller_profiles.rs)
            "profiles" => c.profiles_json(),
            _ => Json::Null,
        }
    }));
    out(r.unwrap_or(Json::Null))
}

#[no_mangle]
pub unsafe extern "C" fn rh_set_frame_callback(b: *mut Bridge, cb: Option<FrameCb>, data: *mut c_void) {
    if b.is_null() {
        return;
    }
    let b = &*b;
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        *b.frame_cb.lock().unwrap_or_else(|p| p.into_inner()) = cb.map(|f| (f as usize, data as usize));
    }));
}

/// The window's own OpenGL loader (QOpenGLContext::getProcAddress), used by mpv before any other lookup.
#[no_mangle]
pub extern "C" fn rh_set_gl_loader(f: Option<unsafe extern "C" fn(*const c_char) -> *mut c_void>) {
    reelhouse::mpv::set_host_gl_loader(f);
}

/// The reason the player could not start (empty when it did). Release with `rh_free_str`.
#[no_mangle]
pub unsafe extern "C" fn rh_video_error(b: *mut Bridge) -> *mut c_char {
    if b.is_null() {
        return std::ptr::null_mut();
    }
    let b = &*b;
    let e = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        b.core.lock().unwrap_or_else(|p| p.into_inner()).0.mpv_error.clone().unwrap_or_default()
    }))
    .unwrap_or_default();
    CString::new(e).unwrap_or_default().into_raw()
}

/// Render thread, OpenGL context current: sets up mpv's drawing. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn rh_video_init(b: *mut Bridge) -> c_int {
    if b.is_null() {
        return -1;
    }
    let b = &*b;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let cb = b.frame_cb.lock().ok().and_then(|g| *g);
        let mut guard = match b.core.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let c = &mut guard.0;
        c.init_player(
            Box::new(move || {
                if let Some((f, d)) = cb {
                    let f: FrameCb = std::mem::transmute::<usize, FrameCb>(f);
                    f(d as *mut c_void);
                }
            }),
            0,
        );
        match c.mpv.clone() {
            Some(m) => {
                if let Ok(mut slot) = b.mpv.lock() {
                    *slot = Some(m);
                }
                0
            }
            None => -1,
        }
    }))
    .unwrap_or(-2)
}

/// Render thread, OpenGL context current: draws the current frame into `fbo`.
#[no_mangle]
pub unsafe extern "C" fn rh_video_render(b: *mut Bridge, fbo: c_int, w: c_int, h: c_int) {
    if b.is_null() {
        return;
    }
    let b = &*b;
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let m = b.mpv.lock().ok().and_then(|g| g.clone());
        if let Some(m) = m {
            m.render_flip(fbo, w, h, false);
        }
    }));
}

/// Decodes a WebP picture (the poster servers send them and Qt's own plugin may be missing).
/// Returns RGBA pixels (release with `rh_free_pixels`) and writes the size, or null when it is not a readable WebP.
#[no_mangle]
pub unsafe extern "C" fn rh_decode_webp(data: *const u8, len: usize, w: *mut c_int, h: *mut c_int, out_len: *mut usize) -> *mut u8 {
    if data.is_null() || w.is_null() || h.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(data, len);
    let r = std::panic::catch_unwind(|| -> Option<(Vec<u8>, u32, u32)> {
        let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
        let (iw, ih) = d.dimensions();
        let alpha = d.has_alpha();
        let mut buf = vec![0u8; d.output_buffer_size()?];
        d.read_image(&mut buf).ok()?;
        let rgba = if alpha {
            buf
        } else {
            let mut v = Vec::with_capacity(buf.len() / 3 * 4);
            for px in buf.chunks_exact(3) {
                v.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            v
        };
        Some((rgba, iw, ih))
    });
    match r {
        Ok(Some((v, iw, ih))) => {
            *w = iw as c_int;
            *h = ih as c_int;
            // A boxed slice: its length is exactly its allocation, so rh_free_pixels can rebuild it.
            let bx = v.into_boxed_slice();
            *out_len = bx.len();
            Box::into_raw(bx) as *mut u8
        }
        _ => std::ptr::null_mut(),
    }
}

/// A cover found by the title alone (AniList, Kitsu, Jikan, Cinemeta), for a picture whose own address failed.
/// Returns the picture file bytes (release with `rh_free_pixels`) or null. Blocks: call it from a worker thread.
#[no_mangle]
pub unsafe extern "C" fn rh_title_cover(title: *const c_char, out_len: *mut usize) -> *mut u8 {
    if title.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let t = CStr::from_ptr(title).to_string_lossy().into_owned();
    let r = std::panic::catch_unwind(|| {
        let http = reelhouse::net::UreqHttp::new();
        reelhouse::artwork::title_cover(&http, &t)
    });
    match r {
        Ok(Some(v)) => {
            let bx = v.into_boxed_slice();
            *out_len = bx.len();
            Box::into_raw(bx) as *mut u8
        }
        _ => std::ptr::null_mut(),
    }
}

/// A picture's bytes by the core's own HTTPS: the image provider asks when Qt's network fails (Android's TLS).
#[no_mangle]
pub unsafe extern "C" fn rh_fetch_picture(url: *const c_char, out_len: *mut usize) -> *mut u8 {
    if url.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let u = CStr::from_ptr(url).to_string_lossy().into_owned();
    let r = std::panic::catch_unwind(|| {
        use reelhouse::http::Http;
        let http = reelhouse::net::UreqHttp::new();
        let rq = reelhouse::http::Request::get(&u)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
            .header("Accept", "image/png,image/jpeg,image/gif,image/*;q=0.8")
            .timeout(25);
        match http.send(&rq) {
            Ok(r) if r.ok() && !r.body.is_empty() => Some(r.body),
            _ => None,
        }
    });
    match r {
        Ok(Some(v)) => {
            let bx = v.into_boxed_slice();
            *out_len = bx.len();
            Box::into_raw(bx) as *mut u8
        }
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rh_free_pixels(p: *mut u8, len: usize) {
    if !p.is_null() {
        // Made by rh_decode_webp / rh_title_cover as Box<[u8]> of exactly `len` bytes.
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, len)));
    }
}

/// The picture of a video drawn by a second mpv into memory (`clip`: the clip of a Study card, else the opening / ending in the X-Ray), fitted in `max_w` x `max_h` (keeping its shape).
/// Returns null when mpv drew nothing new since the last call (unless `force`, e.g. after a resize) or no video
/// plays. The pixels are 4 bytes each, 0xAARRGGBB in memory order of the machine (QImage::Format_RGB32); release them
/// with `rh_free_pixels`. Called from the interface thread; the drawing itself happens without the core locked.
unsafe fn frame_of(clip: bool, b: *mut Bridge, max_w: c_int, max_h: c_int, force: c_int, out_w: *mut c_int, out_h: *mut c_int, out_len: *mut usize) -> *mut u8 {
    if b.is_null() || out_w.is_null() || out_h.is_null() || out_len.is_null() || max_w < 16 || max_h < 16 {
        return std::ptr::null_mut();
    }
    let b = &*b;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Option<(Vec<u8>, i32, i32)> {
        let src = {
            let g = match b.core.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if clip { g.0.study_clip_source() } else { g.0.theme_frame_source() }
        };
        let (m, fresh) = src?;
        if !fresh.swap(false, std::sync::atomic::Ordering::SeqCst) && force == 0 {
            return None;
        }
        let (vw, vh) = (m.get_i64("dwidth")? as f64, m.get_i64("dheight")? as f64);
        if vw < 1.0 || vh < 1.0 {
            return None;
        }
        let scale = (max_w as f64 / vw).min(max_h as f64 / vh).min(1.0);
        // a width in steps of 16 pixels: rows of 64 bytes are what mpv's software drawing is fastest with
        let w = (((vw * scale) as i32) / 16 * 16).max(16);
        let h = ((w as f64 * vh / vw).round() as i32).max(2);
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        m.render_sw_now(w, h, &mut buf);
        // mpv leaves the fourth byte at 0; Qt wants it opaque
        for px in buf.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Some((buf, w, h))
    }));
    match r {
        Ok(Some((v, w, h))) => {
            *out_w = w;
            *out_h = h;
            let bx = v.into_boxed_slice();
            *out_len = bx.len();
            Box::into_raw(bx) as *mut u8
        }
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rh_theme_frame(b: *mut Bridge, max_w: c_int, max_h: c_int, force: c_int, out_w: *mut c_int, out_h: *mut c_int, out_len: *mut usize) -> *mut u8 {
    frame_of(false, b, max_w, max_h, force, out_w, out_h, out_len)
}

/// Same as `rh_theme_frame`, for the clip playing inside a Study card.
#[no_mangle]
pub unsafe extern "C" fn rh_clip_frame(b: *mut Bridge, max_w: c_int, max_h: c_int, force: c_int, out_w: *mut c_int, out_h: *mut c_int, out_len: *mut usize) -> *mut u8 {
    frame_of(true, b, max_w, max_h, force, out_w, out_h, out_len)
}

/// The interface size chosen in Preferences (percent; the GTK version's 120 is Qt's 100), read before the window exists.
#[no_mangle]
pub extern "C" fn rh_ui_scale() -> c_int {
    std::panic::catch_unwind(|| *START_SCALE.get_or_init(|| reelhouse::settings::Settings::load(&reelhouse::util::data_dir()).general.ui_scale) as c_int).unwrap_or(120)
}

/// Sets the folder that holds every log (call before `rh_new`).
#[no_mangle]
pub unsafe extern "C" fn rh_set_debug_dir(dir: *const c_char) {
    if dir.is_null() {
        return;
    }
    let d = CStr::from_ptr(dir).to_string_lossy().into_owned();
    if !d.is_empty() {
        let _ = std::fs::create_dir_all(&d);
        // Every panic of the core (even one that is caught) is written to crash-rust.txt, so a silent close leaves a trace.
        let file = std::path::Path::new(&d).join("crash-rust.txt");
        std::panic::set_hook(Box::new(move |info| {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
                let _ = writeln!(f, "{}\nlast step: {}\n", info, reelhouse::util::last_breadcrumb());
            }
        }));
        reelhouse::debug::set_dir(std::path::PathBuf::from(d));
    }
}

/// Where crash.txt goes (the data folder). Release with `rh_free_str`.
#[no_mangle]
pub extern "C" fn rh_data_dir() -> *mut c_char {
    CString::new(reelhouse::util::data_dir().display().to_string()).unwrap_or_default().into_raw()
}

/// The last risky step the core announced (for crash.txt). Release with `rh_free_str`.
#[no_mangle]
pub extern "C" fn rh_last_step() -> *mut c_char {
    CString::new(reelhouse::util::last_breadcrumb()).unwrap_or_default().into_raw()
}

// ---------------------------------------------------------------- sound of Gemini Live (qt/src/liveaudio.cpp)
// The window's sound thread moves the microphone and the voice of the tutor through the hub of `reelhouse::live`,
// without the lock of the core: these never block more than a short mutex of the hub.

/// The window can record and play (Qt Multimedia was built in): 1, else 0.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[no_mangle]
pub extern "C" fn rh_live_audio_present(on: c_int) {
    let _ = std::panic::catch_unwind(|| reelhouse::live::hub().set_present(on != 0));
}

/// A call wants the sound devices open: 1, else 0 (the window opens and closes them on this).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[no_mangle]
pub extern "C" fn rh_live_audio_wanted() -> c_int {
    std::panic::catch_unwind(|| reelhouse::live::hub().wanted() as c_int).unwrap_or(0)
}

/// The microphone the learner chose (setting `voice_mic`; "" = the system's default). Free with rh_free_str.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[no_mangle]
pub extern "C" fn rh_live_audio_mic() -> *mut c_char {
    let id = std::panic::catch_unwind(|| reelhouse::live::hub().device()).unwrap_or_default();
    CString::new(id.replace('\0', "")).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// A piece of the microphone: `n` mono samples at `rate` Hz.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[no_mangle]
pub unsafe extern "C" fn rh_live_mic(pcm: *const i16, n: usize, rate: c_int) {
    if pcm.is_null() || n == 0 || rate <= 0 {
        return;
    }
    let s = std::slice::from_raw_parts(pcm, n);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reelhouse::live::hub().mic(s, rate as u32)));
}

/// Up to `cap` mono samples of the tutor's voice at `rate` Hz for the speaker (`queued_ms`: what the device still holds).
/// Returns how many were written; `*flush` = 1 when the device must drop what it holds (the learner interrupted).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[no_mangle]
pub unsafe extern "C" fn rh_live_speaker(out: *mut i16, cap: usize, rate: c_int, queued_ms: c_int, flush: *mut c_int) -> usize {
    // `cap` 0 (the device is full) still reads a pending flush
    if out.is_null() || rate <= 0 {
        return 0;
    }
    let o = std::slice::from_raw_parts_mut(out, cap);
    let (n, f) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reelhouse::live::hub().speaker(o, rate as u32, queued_ms.max(0) as u32))).unwrap_or((0, false));
    if !flush.is_null() {
        *flush = f as c_int;
    }
    n
}

/// A problem of the sound devices (shown in the call and written to the log).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[no_mangle]
pub unsafe extern "C" fn rh_live_audio_error(text: *const c_char) {
    if text.is_null() {
        return;
    }
    let t = CStr::from_ptr(text).to_string_lossy().into_owned();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reelhouse::live::hub().device_error(&t)));
}

/// The four main colours of a picture (moving background): `px` is RGB triples, `n` pixels; the result goes into `out`
/// (12 bytes, RGB of each colour).
#[no_mangle]
pub unsafe extern "C" fn rh_mood(px: *const u8, n: usize, out: *mut u8) {
    if px.is_null() || out.is_null() {
        return;
    }
    let bytes = std::slice::from_raw_parts(px, n * 3);
    let r = std::panic::catch_unwind(|| {
        let pixels: Vec<reelhouse::palette::Rgb> = bytes.chunks_exact(3).map(|c| (c[0], c[1], c[2])).collect();
        reelhouse::palette::mood(&pixels)
    });
    if let Ok(m) = r {
        // `out` holds 12 bytes: at most four colours.
        for (i, c) in m.iter().take(4).enumerate() {
            *out.add(i * 3) = c.0;
            *out.add(i * 3 + 1) = c.1;
            *out.add(i * 3 + 2) = c.2;
        }
    }
}

/// The orbs of time at `now` (seconds, local wall clock): `reelhouse::orbs::ORBS * TRAIL` points of 5 floats each
/// (x, y in -1..1, size, brightness, depth) written to `out`, which holds `cap` floats. Returns the number of points.
#[no_mangle]
pub unsafe extern "C" fn rh_orbs(now: f64, out: *mut f32, cap: usize) -> usize {
    if out.is_null() {
        return 0;
    }
    let pts = std::panic::catch_unwind(|| reelhouse::orbs::frame(now)).unwrap_or_default();
    let n = pts.len().min(cap / 5);
    for (i, p) in pts.iter().take(n).enumerate() {
        let o = out.add(i * 5);
        *o = p.x;
        *o.add(1) = p.y;
        *o.add(2) = p.size;
        *o.add(3) = p.bright;
        *o.add(4) = p.depth;
    }
    n
}

/// A made-up interface sound as a WAV file (reelhouse::orbs): `kind` 0 = focus moved, 1 = select. Its length in `len`.
/// Release with `rh_free_pixels(p, len)`.
#[no_mangle]
pub unsafe extern "C" fn rh_nav_blip(kind: i32, len: *mut usize) -> *mut u8 {
    let w = if kind == 1 { reelhouse::orbs::select_blip_wav() } else { reelhouse::orbs::nav_blip_wav() };
    let mut v = w.into_boxed_slice();
    if !len.is_null() {
        *len = v.len();
    }
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Game controller presses since the last call (GTK gui/gamepad.rs): names "A", "B", "X", "Y", "Start", "Select",
/// "Left", "Right", "Up", "Down" (D-pad or its hat axes), "LSLeft", "LSRight", "LSUp", "LSDown" (left stick), "LB", "RB", "LT", "RT" (buttons or analog
/// triggers), "L3", "R3", comma separated; "~Name" is a
/// release, "RS:<y>" the right stick's height (-1 down to 1 up) and "RSX:<x>" its side (-1 left to 1 right), 0 in the
/// deadzone. Connections come in the same list as
/// "@connected|<kind>|<name>" and "@disconnected|<kind>|<name>", kind = "xbox", "ps4", "ps5" or "pad". Release with `rh_free_str`.
#[no_mangle]
pub extern "C" fn rh_pad_poll() -> *mut c_char {
    let names = std::panic::catch_unwind(pad::poll).unwrap_or_default().join(",");
    CString::new(names).unwrap_or_default().into_raw()
}

/// The left stick deadzone (0.05-0.6; a direction counts past twice it, at most 0.9). Preferences > Controller.
#[no_mangle]
pub extern "C" fn rh_pad_deadzone(dz: f32) {
    pad::set_deadzone(dz);
}

/// PlayStation controllers over HID (ds4.rs): on while controller mode is on.
#[no_mangle]
pub extern "C" fn rh_ds4_enable(on: c_int) {
    let _ = std::panic::catch_unwind(|| ds4::set_enabled(on != 0));
}

/// Light bar colour (already scaled by the brightness), touchpad as a mouse and its speed (1 = normal).
#[no_mangle]
pub extern "C" fn rh_ds4_config(r: u8, g: u8, b: u8, touch_mouse: c_int, speed: f32) {
    let _ = std::panic::catch_unwind(|| {
        ds4::set_light([r, g, b]);
        ds4::set_touch(touch_mouse != 0, speed);
    });
}

/// A short vibration (weak = right motor, strong = left motor, 0-255) of `ms` milliseconds.
#[no_mangle]
pub extern "C" fn rh_ds4_rumble(weak: u8, strong: u8, ms: u32) {
    let _ = std::panic::catch_unwind(|| ds4::rumble(weak, strong, ms));
}

/// The state of the PlayStation controller into `out` (10 floats): connected, kind (4 = DualShock 4, 5 = DualSense),
/// Bluetooth, battery (0-100, -1 unknown), charging, pointer dx, dy (pixels since the last call), wheel (positive =
/// up), touchpad pressed, finger on the pad. Returns the floats written.
#[no_mangle]
pub unsafe extern "C" fn rh_ds4_poll(out: *mut f32, cap: usize) -> usize {
    if out.is_null() {
        return 0;
    }
    let v = std::panic::catch_unwind(ds4::poll).unwrap_or([0.0; 10]);
    let n = v.len().min(cap);
    for (i, x) in v.iter().take(n).enumerate() {
        *out.add(i) = *x;
    }
    n
}

#[cfg(any(windows, target_os = "macos"))]
mod pad {
    use std::cell::RefCell;

    // Polled from the Qt thread only (the timer of CoreBridge), so a thread-local is enough.
    thread_local! {
        // Never dropped: at process exit the threads of gilrs are already gone and dropping it panicked
        // ("threads should not terminate unexpectedly").
        // where the left stick points now on x and y (-1, 0, 1)
        static STICK: RefCell<[i8; 2]> = const { RefCell::new([0, 0]) };
        // where the D-pad hat axes point (some drivers give the D-pad as axes), and the analog triggers (pressed or not)
        static HAT: RefCell<[i8; 2]> = const { RefCell::new([0, 0]) };
        static TRIG: RefCell<[bool; 2]> = const { RefCell::new([false, false]) };
        static DEADZONE: std::cell::Cell<f32> = const { std::cell::Cell::new(0.3) };
        static PAD: RefCell<Option<Option<std::mem::ManuallyDrop<gilrs::Gilrs>>>> = const { RefCell::new(None) };
        // the PlayStation controller read over HID (ds4.rs): its last buttons, stick and trigger directions, right stick
        static HID: RefCell<(u32, [i8; 2], [bool; 2], [f32; 2])> = const { RefCell::new((0, [0, 0], [false, false], [0.0, 0.0])) };
        // the PlayStation controller was read over HID at the last poll (a change zeroes the right stick, so a value
        // left from the other path does not keep scrolling or zooming)
        static HID_ON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// The presses of the controller ds4.rs reads (it switched the pad to reports gilrs cannot read). Same names.
    fn hid(out: &mut Vec<String>) -> bool {
        let Some((c, latch)) = crate::ds4::controls_now() else {
            // let go of whatever was held when it went away: buttons, triggers, left stick, and the right stick to 0
            HID.with(|h| {
                let mut h = h.borrow_mut();
                for (i, n) in crate::ds4::NAMES.iter().enumerate() {
                    if h.0 & (1 << i) != 0 {
                        out.push(format!("~{}", n));
                    }
                }
                for (i, n) in ["LT", "RT"].into_iter().enumerate() {
                    if h.2[i] {
                        out.push(format!("~{}", n));
                    }
                }
                steer(out, &mut h.1[0], 0, "LSLeft", "LSRight");
                steer(out, &mut h.1[1], 0, "LSDown", "LSUp");
                *h = (0, [0, 0], [false, false], [0.0, 0.0]);
            });
            if HID_ON.with(|o| o.replace(false)) {
                out.push("RS:0.00".into());
                out.push("RSX:0.00".into());
            }
            return false;
        };
        if !HID_ON.with(|o| o.replace(true)) {
            out.push("RS:0.00".into());
            out.push("RSX:0.00".into());
        }
        let dz = DEADZONE.with(|d| d.get());
        HID.with(|h| {
            let mut h = h.borrow_mut();
            let old = h.0;
            for (i, n) in crate::ds4::NAMES.iter().enumerate() {
                let bit = 1u32 << i;
                let was = old & bit != 0;
                let now = c.buttons & bit != 0;
                if now && !was {
                    out.push(n.to_string());
                } else if !now && was {
                    out.push(format!("~{}", n));
                } else if !now && !was && latch & bit != 0 {
                    // tapped and let go between two polls
                    out.push(n.to_string());
                    out.push(format!("~{}", n));
                }
            }
            h.0 = c.buttons;
            // L2 / R2: pressed past half way
            for (i, (v, n)) in [(c.l2, "LT"), (c.r2, "RT")].into_iter().enumerate() {
                if v > 0.5 && !h.2[i] {
                    h.2[i] = true;
                    out.push(n.to_string());
                } else if v < 0.3 && h.2[i] {
                    h.2[i] = false;
                    out.push(format!("~{}", n));
                }
            }
            // left stick: "LSLeft" … "LSUp" (the menus and the focus; the D-pad moves the page), with the deadzone of
            // Preferences > Controller
            let on = (dz * 2.0).min(0.9);
            for (i, (v, neg, pos)) in [(c.lx, "LSLeft", "LSRight"), (c.ly, "LSDown", "LSUp")].into_iter().enumerate() {
                let cur = h.1[i];
                let dir = if v > on { 1 } else if v < -on { -1 } else if v.abs() < dz { 0 } else { cur };
                steer(out, &mut h.1[i], dir, neg, pos);
            }
            // right stick: "RS:<y>" and "RSX:<x>", sent when they change noticeably (and once more back at 0)
            for (i, (v, key)) in [(c.ry, "RS"), (c.rx, "RSX")].into_iter().enumerate() {
                let v = if v.abs() < rs_dead(dz) { 0.0 } else { v };
                if (v - h.3[i]).abs() > 0.05 || (v == 0.0 && h.3[i] != 0.0) {
                    h.3[i] = v;
                    out.push(format!("{}:{:.2}", key, v));
                }
            }
        });
        true
    }

    /// The model from the USB ids (names differ between drivers): Microsoft = Xbox, Sony = DualShock 4 or DualSense.
    pub fn kind(vendor: Option<u16>, product: Option<u16>, name: &str) -> &'static str {
        let n = name.to_lowercase();
        match (vendor, product) {
            (Some(0x045e), _) => "xbox",
            (Some(0x054c), Some(0x0ce6 | 0x0df2)) => "ps5",
            (Some(0x054c), _) => "ps4",
            _ if n.contains("xbox") || n.contains("xinput") => "xbox",
            _ if n.contains("dualsense") => "ps5",
            _ if n.contains("dualshock") || n.contains("wireless controller") || n.contains("playstation") => "ps4",
            _ => "pad",
        }
    }

    fn tag(pad: &gilrs::Gilrs, id: gilrs::GamepadId, what: &str) -> String {
        let g = pad.gamepad(id);
        let name: String = g.name().chars().filter(|c| *c != ',' && *c != '|').collect();
        format!("@{}|{}|{}", what, kind(g.vendor_id(), g.product_id(), &name), name)
    }

    pub fn set_deadzone(dz: f32) {
        DEADZONE.with(|d| d.set(dz.clamp(0.05, 0.6)));
    }

    /// A three-way direction (-1, 0, 1) that changed: the release of the old one and the press of the new one.
    /// The right stick's deadzone: at least 0.25, so a worn stick that rests a little off centre does not keep
    /// scrolling or zooming (HP: the image reader zoomed out on its own).
    fn rs_dead(dz: f32) -> f32 {
        dz.clamp(0.25, 0.6)
    }

    fn steer(out: &mut Vec<String>, old: &mut i8, dir: i8, neg: &str, pos: &str) {
        if dir != *old {
            if *old != 0 {
                out.push(format!("~{}", if *old > 0 { pos } else { neg }));
            }
            if dir != 0 {
                out.push((if dir > 0 { pos } else { neg }).to_string());
            }
            *old = dir;
        }
    }

    pub fn poll() -> Vec<String> {
        PAD.with(|cell| {
            let mut slot = cell.borrow_mut();
            let mut out = Vec::new();
            if slot.is_none() {
                *slot = Some(gilrs::Gilrs::new().ok().map(std::mem::ManuallyDrop::new));
                // Controllers already plugged in at start-up get no Connected event: report them once.
                if let Some(Some(pad)) = slot.as_ref() {
                    for (id, _) in pad.gamepads() {
                        out.push(tag(pad, id, "connected"));
                    }
                }
            }
            let from_hid = hid(&mut out);
            let Some(Some(pad)) = slot.as_mut() else { return out };
            while let Some(ev) = pad.next_event() {
                // while ds4.rs reads the PlayStation controller, gilrs' events of it are stale or missing: skip them
                if from_hid && !matches!(ev.event, gilrs::EventType::Connected | gilrs::EventType::Disconnected) {
                    let g = pad.gamepad(ev.id);
                    if g.vendor_id() == Some(0x054c) || kind(g.vendor_id(), g.product_id(), g.name()).starts_with("ps") {
                        continue;
                    }
                }
                match ev.event {
                    gilrs::EventType::ButtonPressed(b, _) => {
                        // the triggers are reported once, by ButtonChanged above (both events come)
                        if matches!(b, gilrs::Button::LeftTrigger2 | gilrs::Button::RightTrigger2) {
                            continue;
                        }
                        if let Some(n) = name(b) {
                            out.push(n.to_string());
                        }
                    }
                    // releases come as "~Name" (the interface repeats a held direction until then)
                    gilrs::EventType::ButtonReleased(b, _) => {
                        if matches!(b, gilrs::Button::LeftTrigger2 | gilrs::Button::RightTrigger2) {
                            continue;
                        }
                        if let Some(n) = name(b) {
                            out.push(format!("~{}", n));
                        }
                    }
                    // Analog triggers (L2 / R2 of a PlayStation controller through some drivers): pressed past 0.5.
                    gilrs::EventType::ButtonChanged(b @ (gilrs::Button::LeftTrigger2 | gilrs::Button::RightTrigger2), v, _) => {
                        let i = if b == gilrs::Button::LeftTrigger2 { 0 } else { 1 };
                        let n = if i == 0 { "LT" } else { "RT" };
                        TRIG.with(|t| {
                            let mut t = t.borrow_mut();
                            if v > 0.5 && !t[i] {
                                t[i] = true;
                                out.push(n.to_string());
                            } else if v < 0.3 && t[i] {
                                t[i] = false;
                                out.push(format!("~{}", n));
                            }
                        });
                    }
                    // The left stick ("LSLeft" …) works like a D-pad of its own: a press when it passes twice the deadzone (Preferences >
                    // Controller, 0.3 by default), the release when it comes back inside the deadzone. Drivers that
                    // give the D-pad as hat axes, or L2 / R2 as Z axes, are read too. The right stick scrolls:
                    // "RS:<y>" with y from -1 (down) to 1 up, "RSX:<x>" with x from -1 (left) to 1 right.
                    gilrs::EventType::AxisChanged(axis, v, _) => {
                        let dz = DEADZONE.with(|d| d.get());
                        let (i, neg, pos) = match axis {
                            gilrs::Axis::LeftStickX => (0, "LSLeft", "LSRight"),
                            // gilrs: up is positive
                            gilrs::Axis::LeftStickY => (1, "LSDown", "LSUp"),
                            gilrs::Axis::DPadX | gilrs::Axis::DPadY => {
                                let x = axis == gilrs::Axis::DPadX;
                                let dir = if v > 0.5 { 1 } else if v < -0.5 { -1 } else { 0 };
                                HAT.with(|h| {
                                    let mut h = h.borrow_mut();
                                    let (neg, pos) = if x { ("Left", "Right") } else { ("Down", "Up") };
                                    steer(&mut out, &mut h[if x { 0 } else { 1 }], dir, neg, pos);
                                });
                                continue;
                            }
                            gilrs::Axis::LeftZ | gilrs::Axis::RightZ => {
                                let i = if axis == gilrs::Axis::LeftZ { 0 } else { 1 };
                                let n = if i == 0 { "LT" } else { "RT" };
                                TRIG.with(|t| {
                                    let mut t = t.borrow_mut();
                                    if v > 0.5 && !t[i] {
                                        t[i] = true;
                                        out.push(n.to_string());
                                    } else if v < 0.3 && t[i] {
                                        t[i] = false;
                                        out.push(format!("~{}", n));
                                    }
                                });
                                continue;
                            }
                            gilrs::Axis::RightStickY | gilrs::Axis::RightStickX => {
                                let a = if v.abs() < rs_dead(dz) { 0.0 } else { v };
                                let key = if axis == gilrs::Axis::RightStickX { "RSX" } else { "RS" };
                                out.push(format!("{}:{:.2}", key, a));
                                continue;
                            }
                            _ => continue,
                        };
                        STICK.with(|st| {
                            let mut st = st.borrow_mut();
                            let on = (dz * 2.0).min(0.9);
                            let dir = if v > on { 1 } else if v < -on { -1 } else if v.abs() < dz { 0 } else { st[i] };
                            steer(&mut out, &mut st[i], dir, neg, pos);
                        });
                    }
                    gilrs::EventType::Connected => out.push(tag(pad, ev.id, "connected")),
                    gilrs::EventType::Disconnected => out.push(tag(pad, ev.id, "disconnected")),
                    _ => {}
                }
            }
            out
        })
    }

    fn name(b: gilrs::Button) -> Option<&'static str> {
        use gilrs::Button::*;
        Some(match b {
            South => "A",
            East => "B",
            West => "X",
            North => "Y",
            Start => "Start",
            Select => "Select",
            DPadLeft => "Left",
            DPadRight => "Right",
            DPadUp => "Up",
            DPadDown => "Down",
            LeftTrigger => "LB",
            RightTrigger => "RB",
            LeftTrigger2 => "LT",
            RightTrigger2 => "RT",
            LeftThumb => "L3",
            RightThumb => "R3",
            _ => return None,
        })
    }
}

// Linux (gilrs needs libudev), Android and iOS (the controllers arrive as Qt key events): nothing to poll.
#[cfg(not(any(windows, target_os = "macos")))]
mod pad {
    pub fn poll() -> Vec<String> {
        Vec::new()
    }
    pub fn set_deadzone(_dz: f32) {}
}

/// What this build of the core can do where it runs (see `reelhouse::platform::caps`): JSON such as
/// `{"os":"android","mobile":true,"localProcesses":false,...}`. The `Platform` singleton of QML reads it.
/// Release with `rh_free_str`.
#[no_mangle]
pub extern "C" fn rh_platform_caps() -> *mut c_char {
    let out = std::panic::catch_unwind(|| reelhouse::platform::caps().dump()).unwrap_or_else(|_| "{}".to_string());
    CString::new(out).unwrap_or_default().into_raw()
}

/// A text of the interface in the language `lang` ("en", "pt-BR", "es"...), the text itself when nothing is translated.
/// Release with `rh_free_str`.
#[no_mangle]
pub unsafe extern "C" fn rh_tr(lang: *const c_char, text: *const c_char) -> *mut c_char {
    let lang = if lang.is_null() { String::new() } else { CStr::from_ptr(lang).to_string_lossy().into_owned() };
    let text = if text.is_null() { String::new() } else { CStr::from_ptr(text).to_string_lossy().into_owned() };
    let out = std::panic::catch_unwind(|| reelhouse::i18n::translate(&lang, &text).map(|s| s.to_string()).unwrap_or_else(|| text.clone())).unwrap_or(text);
    CString::new(out).unwrap_or_default().into_raw()
}

/// Research mode's ad blocker (reelhouse::adblock), asked by Chromium's network thread for each request of a page:
/// 1 when the request should be cancelled. `kind`: "script", "image", "subdocument"... Any thread.
#[no_mangle]
pub unsafe extern "C" fn rh_adblock_check(url: *const c_char, source: *const c_char, kind: *const c_char) -> c_int {
    let (u, s, k) = (text(url), text(source), text(kind));
    std::panic::catch_unwind(|| reelhouse::adblock::should_block(u, s, k) as c_int).unwrap_or(0)
}

/// The element hiding CSS of the ad blocker for the page at `url` ("" when nothing applies). Free with rh_free_str.
#[no_mangle]
pub unsafe extern "C" fn rh_adblock_css(url: *const c_char) -> *mut c_char {
    let u = text(url);
    let css = std::panic::catch_unwind(|| reelhouse::adblock::page_css(u)).unwrap_or_default();
    CString::new(css.replace('\0', "")).unwrap_or_default().into_raw()
}

fn state_study(c: &Core) -> Json {
    let started = std::time::Instant::now();
    let now = reelhouse::util::now_unix() as i64;
    let col = &c.study;
    // the numbers of every deck in one pass over the cards (per-deck `counts` + `total_cards` walked all the cards
    // several times per deck: seconds with a big AnkiWeb collection, on the GUI thread, at each Study refresh)
    let ov = col.overview(now);
    let num = |id: i64| ov.get(&id).copied().unwrap_or_default();
    let mut decks: Vec<&reelhouse::study::Deck> = col.decks.iter().collect();
    decks.sort_by_cached_key(|d| d.name.to_lowercase());
    let rows: Vec<Json> = decks
        .iter()
        .map(|d| {
            let (nw, ln, rv, total) = num(d.id);
            let depth = d.name.matches("::").count();
            Json::obj(vec![
                ("id", n(d.id as f64)),
                ("name", s(&d.name)),
                ("short", s(d.name.rsplit("::").next().unwrap_or(&d.name))),
                ("depth", n(depth as f64)),
                ("new", n(nw as f64)),
                ("learn", n(ln as f64)),
                ("review", n(rv as f64)),
                ("total", n(total as f64)),
            ])
        })
        .collect();
    let session = match c.study_session.as_ref() {
        None => Json::Null,
        Some(sess) => {
            let (nw, ln, rv) = col.counts(sess.did, now);
            let card = sess.card.and_then(|i| col.cards.get(i)).map(|cd| {
                let r = col.render_full(cd, &c.study_media_dir());
                let (q, a) = (r.q, r.a);
                Json::obj(vec![
                    ("id", n(cd.id as f64)),
                    ("q", s(&q)),
                    ("a", s(&a)),
                    // played by themselves when the side shows (Anki autoplay), cloze cards included
                    ("qSounds", Json::Arr(r.q_sounds.iter().map(|x| s(x)).collect())),
                    ("aSounds", Json::Arr(r.a_sounds.iter().map(|x| s(x)).collect())),
                    ("clip", r.clip.as_deref().map(s).unwrap_or(Json::Null)),
                    ("kind", s(match cd.kind { 0 => "new", 2 => "review", _ => "learn" })),
                    ("flag", n(cd.flag as f64)),
                    ("susp", Json::Bool(cd.susp)),
                    ("marked", Json::Bool(col.notes.iter().any(|x| x.id == cd.nid && x.tags.split_whitespace().any(|t| t.eq_ignore_ascii_case("marked"))))),
                    ("labels", Json::Arr(reelhouse::study::labels(cd, now).iter().map(|x| s(x)).collect())),
                    // the note behind the card, for "Edit" in the review
                    ("nid", n(cd.nid as f64)),
                    ("mid", n(col.notes.iter().find(|x| x.id == cd.nid).map(|x| x.mid).unwrap_or(0) as f64)),
                    ("fields", Json::Arr(col.notes.iter().find(|x| x.id == cd.nid).map(|x| x.fields.iter().map(|f| s(f)).collect()).unwrap_or_default())),
                ])
            });
            Json::obj(vec![
                ("id", n(sess.did as f64)),
                ("name", s(&col.deck_name(sess.did))),
                ("card", card.unwrap_or(Json::Null)),
                ("showAnswer", Json::Bool(sess.answer_shown)),
                ("done", n(sess.done as f64)),
                ("canUndo", Json::Bool(col.undo.is_some())),
                ("buried", n(col.buried_count(sess.did, now) as f64)),
                ("new", n(nw as f64)),
                ("learn", n(ln as f64)),
                ("review", n(rv as f64)),
            ])
        }
    };
    let models: Vec<Json> = col.models.iter().map(|m| Json::obj(vec![("id", n(m.id as f64)), ("name", s(&m.name)), ("cloze", Json::Bool(m.cloze)), ("fields", Json::Arr(m.fields.iter().map(|f| s(f)).collect()))])).collect();
    let browse = match c.study_browse.as_ref() {
        None => Json::Null,
        Some((did, q)) => {
            let idx = col.browse(*did, q, 300);
            let notes: std::collections::HashMap<i64, &reelhouse::study::Note> = col.notes.iter().map(|x| (x.id, x)).collect();
            let deck_names: std::collections::HashMap<i64, &str> = col.decks.iter().map(|d| (d.id, d.name.as_str())).collect();
            let rows: Vec<Json> = idx
                .iter()
                .filter_map(|&i| {
                    let cd = &col.cards[i];
                    let note = *notes.get(&cd.nid)?;
                    let text = |t: &str| {
                        let mut o = String::new();
                        let mut inside = false;
                        for ch in t.chars() {
                            match ch {
                                '<' => inside = true,
                                '>' if inside => inside = false,
                                _ if !inside => o.push(ch),
                                _ => {}
                            }
                        }
                        o.replace("&nbsp;", " ").chars().take(90).collect::<String>()
                    };
                    Some(Json::obj(vec![
                        ("nid", n(note.id as f64)),
                        ("cid", n(cd.id as f64)),
                        ("mid", n(note.mid as f64)),
                        ("front", s(&text(note.fields.first().map(|x| x.as_str()).unwrap_or("")))),
                        ("back", s(&text(note.fields.get(1).map(|x| x.as_str()).unwrap_or("")))),
                        ("deck", s(deck_names.get(&cd.did).copied().unwrap_or(""))),
                        ("kind", s(match cd.kind { 0 => "new", 2 => "review", _ => "learn" })),
                        ("due", s(&if cd.kind == 2 { reelhouse::study::span((cd.due - reelhouse::study::today(now)).max(0) * 86400) } else { String::new() })),
                        ("fields", Json::Arr(note.fields.iter().map(|f| s(f)).collect())),
                    ]))
                })
                .collect();
            Json::obj(vec![("did", n(*did as f64)), ("q", s(q)), ("rows", Json::Arr(rows))])
        }
    };
    let stats = match c.study_stats {
        None => Json::Null,
        Some(did) => {
            let st = col.stats(did, now);
            Json::obj(vec![
                ("did", n(did as f64)),
                ("today", n(st.today as f64)),
                ("todayAgain", n(st.today_again as f64)),
                ("last30", Json::Arr(st.last30.iter().map(|x| n(*x as f64)).collect())),
                ("retention", n(st.retention)),
                ("new", n(st.new as f64)),
                ("learning", n(st.learning as f64)),
                ("young", n(st.young as f64)),
                ("mature", n(st.mature as f64)),
                ("suspended", n(st.suspended as f64)),
                ("forecast", Json::Arr(st.forecast.iter().map(|x| n(*x as f64)).collect())),
                ("totalReviews", n(st.total_reviews as f64)),
            ])
        }
    };
    // AnkiWeb panel: account, state of the last pull, and today's numbers over all the top decks.
    let ankiweb = {
        let (mut due, mut total) = (0u32, 0usize);
        let mut tops = std::collections::HashSet::new();
        for d in col.decks.iter().filter(|d| !d.name.contains("::")) {
            if tops.insert(d.id) {
                let (a, b, c2, t) = num(d.id);
                due += a + b + c2;
                total += t;
            }
        }
        // the review history is one for all decks: today's reviews once (adding it per top deck counted them twice
        // or more, and walked all the cards again for each deck)
        let (today, again) = col.today_reviews(now);
        Json::obj(vec![
            ("user", s(&c.settings.general.ankiweb_user)),
            ("logged", Json::Bool(!c.settings.general.ankiweb_hkey.is_empty())),
            ("busy", Json::Bool(c.ankiweb_busy)),
            ("status", s(&c.ankiweb_status)),
            ("last", n(c.ankiweb_last as f64)),
            ("due", n(due as f64)),
            ("reviewedToday", n(today as f64)),
            ("againToday", n(again as f64)),
            ("cards", n(total as f64)),
        ])
    };
    // the game of Study (XP and goal shared with Learn, streak, the combo of the review): StudyReview / StudyPage
    let game = c.study_game_json();
    let j = Json::obj(vec![("decks", Json::Arr(rows)), ("session", session), ("importing", Json::Bool(c.study_importing)), ("ankiweb", ankiweb), ("models", Json::Arr(models)), ("browse", browse), ("stats", stats), ("game", game)]);
    // it runs on the GUI thread at each Study refresh: write it down when it is slow (or the first time)
    static FIRST: std::sync::Once = std::sync::Once::new();
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut first = false;
    FIRST.call_once(|| first = true);
    if first || ms >= 30.0 {
        reelhouse::debug::log("study", &format!("state built in {:.1} ms ({} decks, {} cards, {} notes)", ms, col.decks.len(), col.cards.len(), col.notes.len()));
    }
    j
}

// ---------------------------------------------------------------- the microphone (qt/src/voiceinput.cpp)

/// A recording of the microphone: the window feeds the samples of the device (any rate and channels) and the core
/// mixes them down to 16 kHz mono, watches when the learner starts and stops talking (reelhouse::vad) and writes the
/// WAV file. `options` is the JSON of `VadCfg` (`Core.state("voice").vad`, with `auto`). Free with `rh_voice_free`.
#[no_mangle]
pub unsafe extern "C" fn rh_voice_new(options: *const c_char) -> *mut reelhouse::vad::Capture {
    let text = if options.is_null() { String::new() } else { CStr::from_ptr(options).to_string_lossy().into_owned() };
    let cfg = reelhouse::vad::VadCfg::from_json(&Json::parse(&text).unwrap_or(Json::Null));
    Box::into_raw(Box::new(reelhouse::vad::Capture::new(cfg)))
}

/// `count` interleaved samples (-1..1) of `channels` channels at `rate` Hz. Returns 0 nothing new, 1 the learner
/// started talking, 2 the turn ended (stop and save), 3 nobody spoke (stop).
#[no_mangle]
pub unsafe extern "C" fn rh_voice_feed(c: *mut reelhouse::vad::Capture, samples: *const f32, count: usize, channels: c_int, rate: c_int) -> c_int {
    if c.is_null() || samples.is_null() || count == 0 || channels <= 0 || rate <= 0 {
        return 0;
    }
    let data = std::slice::from_raw_parts(samples, count);
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (*c).feed(data, channels as usize, rate as u32) as c_int)).unwrap_or(0)
}

/// The loudness now, 0-1 (for the ring around the button).
#[no_mangle]
pub unsafe extern "C" fn rh_voice_level(c: *const reelhouse::vad::Capture) -> f32 {
    if c.is_null() {
        return 0.0;
    }
    (*c).level()
}

/// 1 while the learner is talking.
#[no_mangle]
pub unsafe extern "C" fn rh_voice_speaking(c: *const reelhouse::vad::Capture) -> c_int {
    if c.is_null() {
        return 0;
    }
    (*c).vad.speaking() as c_int
}

/// The detector's last decision (room noise measured, speech started, end of speech, nothing heard…) for the
/// `[voice]` log, and its number in `seq` (it grows with each one). Free with `rh_free_str`.
#[no_mangle]
pub unsafe extern "C" fn rh_voice_note(c: *const reelhouse::vad::Capture, seq: *mut u32) -> *mut c_char {
    if c.is_null() {
        return CString::default().into_raw();
    }
    if !seq.is_null() {
        *seq = (*c).vad.note_seq;
    }
    CString::new((*c).vad.note.replace('\0', "")).unwrap_or_default().into_raw()
}

/// Writes the speech (or the whole recording when no speech was found) to `path` as a WAV file, a quiet voice
/// raised. Returns 1 saved with speech heard, 2 saved without (a soft voice: the engines decide), 0 nothing recorded,
/// 3 the device gave only silence (nothing written: a closed or wrong microphone), -1 the file failed.
#[no_mangle]
pub unsafe extern "C" fn rh_voice_save(c: *mut reelhouse::vad::Capture, path: *const c_char) -> c_int {
    if c.is_null() || path.is_null() {
        return -1;
    }
    let path = CStr::from_ptr(path).to_string_lossy().into_owned();
    let cap = &*c;
    if cap.samples().len() < reelhouse::vad::RATE as usize / 5 {
        return 0;
    }
    if cap.dead_silent() {
        return 3;
    }
    match std::fs::write(&path, cap.wav()) {
        Ok(()) => {
            if cap.vad.heard() {
                1
            } else {
                2
            }
        }
        Err(_) => -1,
    }
}

/// For the log: the seconds recorded, the loudest sample (dBFS), the gain the WAV file gets and the RMS level of
/// the clip sent (dBFS, before the gain).
#[no_mangle]
pub unsafe extern "C" fn rh_voice_stats(c: *const reelhouse::vad::Capture, seconds: *mut f32, peak_db: *mut f32, gain: *mut f32, rms_db: *mut f32) {
    if c.is_null() {
        return;
    }
    let cap = &*c;
    if !seconds.is_null() {
        *seconds = cap.seconds();
    }
    if !peak_db.is_null() {
        *peak_db = cap.peak_db();
    }
    if !gain.is_null() {
        *gain = cap.gain();
    }
    if !rms_db.is_null() {
        *rms_db = cap.rms_db();
    }
}

#[no_mangle]
pub unsafe extern "C" fn rh_voice_free(c: *mut reelhouse::vad::Capture) {
    if !c.is_null() {
        drop(Box::from_raw(c));
    }
}
