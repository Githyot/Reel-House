// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use reelhouse_core::engine::{Engine, EngineConfig, PlayRequest};
use reelhouse_core::streams::StreamView;
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    engine: Arc<Engine>,
}

type Res<T> = std::result::Result<T, String>;

fn s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Res<Value> {
    serde_json::to_value(state.engine.settings.get()).map_err(s)
}

#[tauri::command]
async fn set_settings(state: State<'_, AppState>, patch: Value) -> Res<Value> {
    let updated = state.engine.settings.patch(patch).map_err(s)?;
    serde_json::to_value(updated).map_err(s)
}

#[tauri::command]
async fn addons_list(state: State<'_, AppState>) -> Res<Value> {
    serde_json::to_value(state.engine.addons.list()).map_err(s)
}

#[tauri::command]
async fn addons_install(state: State<'_, AppState>, url: String) -> Res<Value> {
    let list = state.engine.addons.install(&url).await.map_err(s)?;
    serde_json::to_value(list).map_err(s)
}

#[tauri::command]
async fn addons_remove(state: State<'_, AppState>, url: String) -> Res<Value> {
    serde_json::to_value(state.engine.addons.remove(&url).map_err(s)?).map_err(s)
}

#[tauri::command]
async fn addons_toggle(state: State<'_, AppState>, url: String, enabled: bool) -> Res<Value> {
    serde_json::to_value(state.engine.addons.toggle(&url, enabled).map_err(s)?).map_err(s)
}

#[tauri::command]
async fn board(state: State<'_, AppState>) -> Res<Value> {
    serde_json::to_value(state.engine.addons.board(12).await).map_err(s)
}

#[tauri::command]
async fn search(state: State<'_, AppState>, query: String) -> Res<Value> {
    serde_json::to_value(state.engine.addons.search(&query).await).map_err(s)
}

#[tauri::command]
async fn meta(state: State<'_, AppState>, kind: String, id: String) -> Res<Option<Value>> {
    Ok(state.engine.meta(&kind, &id).await)
}

#[tauri::command]
async fn streams(state: State<'_, AppState>, kind: String, id: String) -> Res<Vec<StreamView>> {
    Ok(state.engine.streams(&kind, &id).await)
}

#[tauri::command]
async fn play(state: State<'_, AppState>, request: PlayRequest) -> Res<()> {
    state.engine.play(request).await;
    Ok(())
}

#[tauri::command]
async fn stop(state: State<'_, AppState>) -> Res<()> {
    state.engine.stop().await;
    Ok(())
}

#[tauri::command]
async fn shaders_status(state: State<'_, AppState>) -> Res<Value> {
    serde_json::to_value(state.engine.shaders.status()).map_err(s)
}

#[tauri::command]
async fn shaders_install(state: State<'_, AppState>) -> Res<Value> {
    let status = state.engine.install_shaders().await.map_err(s)?;
    serde_json::to_value(status).map_err(s)
}

#[tauri::command]
async fn shaders_open_folder(state: State<'_, AppState>) -> Res<()> {
    let dir = state.engine.shaders.dir.clone();
    std::fs::create_dir_all(&dir).map_err(s)?;
    let opener = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(opener).arg(dir).spawn().map_err(s)?;
    Ok(())
}

#[tauri::command]
async fn subtitles_test(state: State<'_, AppState>) -> Res<Value> {
    state.engine.test_translator().await.map_err(s)
}

#[tauri::command]
async fn real_debrid_verify(state: State<'_, AppState>) -> Res<Value> {
    let user = state.engine.verify_real_debrid().await.map_err(s)?;
    serde_json::to_value(user).map_err(s)
}

#[tauri::command]
fn set_locale(state: State<'_, AppState>, locale: String) {
    state.engine.set_locale(&locale);
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle: AppHandle = app.handle().clone();
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&data_dir).ok();

            // mpv ships inside the app as a resource. In development the system mpv is used instead.
            let mpv_name = if cfg!(windows) { "mpv.exe" } else { "mpv" };
            let bundled = app.path().resource_dir().ok().map(|d| d.join("resources").join("mpv").join(mpv_name));

            let emitter = handle.clone();
            let engine = Engine::new(
                EngineConfig { data_dir, bundled_mpv: bundled, locale: String::new() },
                Arc::new(move |channel, payload| {
                    let _ = emitter.emit(channel, payload);
                }),
            );
            let starter = engine.clone();
            tauri::async_runtime::spawn(async move { starter.startup().await });
            app.manage(AppState { engine });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            addons_list,
            addons_install,
            addons_remove,
            addons_toggle,
            board,
            search,
            meta,
            streams,
            play,
            stop,
            shaders_status,
            shaders_install,
            shaders_open_folder,
            subtitles_test,
            real_debrid_verify,
            set_locale
        ])
        .run(tauri::generate_context!())
        .expect("error while running Reelhouse");
}
