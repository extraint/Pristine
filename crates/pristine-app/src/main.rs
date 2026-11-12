// Prevents an extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pristine_core::accounts::AccountStore;
use pristine_core::content::{self, ContentView};
use pristine_core::modrinth::{Kind, SearchParams, SearchResult};
use pristine_core::modules::{self, ModuleDef, ModuleReport};
use pristine_core::msa::DeviceCode;
use pristine_core::logparse::{classify, LogLine};
use pristine_core::skins::{self, SkinEntry};
use pristine_core::{Account, Instance, Loader, Paths, Pristine, Progress, ProgressFn};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::process::Child;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

const LOG_CAP: usize = 5000;

/// Running game processes, by instance id.
#[derive(Default)]
struct Games(Mutex<HashMap<String, Arc<Mutex<Child>>>>);

/// Recent output lines per instance, so the Live Log can be opened after the fact.
#[derive(Default)]
struct Logs(Mutex<HashMap<String, Vec<LogLine>>>);

/// Cancel flag for the sign-in currently waiting on the user, if any.
#[derive(Default)]
struct LoginState(Mutex<Option<Arc<AtomicBool>>>);

#[derive(Serialize)]
struct AccountView { uuid: String, username: String }

#[derive(Serialize)]
struct AccountsState {
    configured: bool,
    selected: Option<String>,
    accounts: Vec<AccountView>,
}

#[derive(Serialize, Clone)]
struct LoginResult { ok: bool, username: Option<String>, error: Option<String> }

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String { e.to_string() }

#[derive(Serialize)]
struct VersionInfo { id: String, kind: String }

#[derive(Serialize)]
struct VersionList { latest_release: String, versions: Vec<VersionInfo> }

#[derive(Serialize)]
struct InstanceView {
    #[serde(flatten)]
    instance: Instance,
    installed: bool,
}

#[derive(Serialize)]
struct AppInfo { data_dir: String, os: &'static str, arch: &'static str, version: &'static str }

#[derive(Serialize, Clone)]
struct ProgressEvent { id: String, stage: String, done: usize, total: usize }

#[derive(Serialize, Clone)]
struct LogEvent { id: String, lines: Vec<LogLine> }

#[derive(Serialize, Clone)]
struct ExitEvent { id: String, code: Option<i32> }

fn progress_emitter(app: AppHandle, id: String) -> ProgressFn {
    Arc::new(move |p: Progress| {
        // Thousands of asset downloads: only forward every 25th tick and the last one.
        if p.done % 25 == 0 || p.done == p.total {
            let _ = app.emit("progress", ProgressEvent {
                id: id.clone(), stage: p.stage.to_string(), done: p.done, total: p.total,
            });
        }
    })
}

#[tauri::command]
async fn list_versions(state: State<'_, Pristine>) -> CmdResult<VersionList> {
    let m = state.versions().await.map_err(err)?;
    Ok(VersionList {
        latest_release: m.latest.release,
        versions: m.versions.into_iter().map(|v| VersionInfo { id: v.id, kind: v.kind }).collect(),
    })
}

#[tauri::command]
fn app_info(state: State<'_, Pristine>) -> AppInfo {
    AppInfo {
        data_dir: state.paths.root().to_string_lossy().into_owned(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        version: env!("CARGO_PKG_VERSION"),
    }
}

fn accounts_state_of(p: &Pristine) -> AccountsState {
    let store = AccountStore::load(&p.paths);
    AccountsState {
        configured: p.ms_client_id().is_some(),
        selected: store.selected.clone().filter(|id| store.accounts.iter().any(|a| &a.uuid == id)),
        accounts: store.accounts.into_iter().map(|a| AccountView { uuid: a.uuid, username: a.username }).collect(),
    }
}

#[tauri::command]
fn accounts_state(state: State<'_, Pristine>) -> AccountsState { accounts_state_of(&state) }

#[tauri::command]
fn get_client_id(state: State<'_, Pristine>) -> Option<String> { state.ms_client_id() }

#[tauri::command]
fn set_client_id(state: State<'_, Pristine>, id: String) -> CmdResult<()> {
    state.set_ms_client_id(Some(id)).map_err(err)
}

#[tauri::command]
fn select_account(state: State<'_, Pristine>, uuid: Option<String>) -> CmdResult<()> {
    let mut store = AccountStore::load(&state.paths);
    store.selected = uuid.filter(|u| store.accounts.iter().any(|a| &a.uuid == u));
    store.save(&state.paths).map_err(err)
}

#[tauri::command]
fn remove_account(state: State<'_, Pristine>, uuid: String) -> CmdResult<()> {
    let mut store = AccountStore::load(&state.paths);
    store.remove(&uuid);
    store.save(&state.paths).map_err(err)
}

/// Start Microsoft sign-in. Returns the code to show; the result arrives as a `ms-login` event.
#[tauri::command]
async fn ms_login_start(app: AppHandle, state: State<'_, Pristine>, login: State<'_, LoginState>) -> CmdResult<DeviceCode> {
    let dc = state.ms_begin().await.map_err(err)?;
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(old) = login.0.lock().unwrap().replace(cancel.clone()) {
        old.store(true, Ordering::Relaxed);
    }
    let pristine: Pristine = state.inner().clone();
    let dc2 = dc.clone();
    tauri::async_runtime::spawn(async move {
        let result = match pristine.ms_finish(&dc2, &cancel).await {
            Ok(a) => LoginResult { ok: true, username: Some(a.username), error: None },
            Err(e) => LoginResult { ok: false, username: None, error: Some(e.to_string()) },
        };
        if !cancel.load(Ordering::Relaxed) { let _ = app.emit("ms-login", result); }
    });
    Ok(dc)
}

#[tauri::command]
fn ms_login_cancel(login: State<'_, LoginState>) {
    if let Some(c) = login.0.lock().unwrap().take() { c.store(true, Ordering::Relaxed); }
}

#[tauri::command]
fn list_instances(state: State<'_, Pristine>) -> CmdResult<Vec<InstanceView>> {
    let paths = &state.paths;
    Ok(Instance::list(paths).map_err(err)?
        .into_iter()
        .map(|i| {
            let installed = paths.version_json(&i.mc_version).is_file() && paths.client_jar(&i.mc_version).is_file();
            InstanceView { instance: i, installed }
        })
        .collect())
}

#[tauri::command]
fn create_instance(state: State<'_, Pristine>, name: String, version: String, loader: Option<Loader>) -> CmdResult<Instance> {
    let loader = loader.unwrap_or_default();
    if name.trim().is_empty() { return Err("Give the instance a name".into()); }
    Instance::create_with(&state.paths, &name, &version, loader).map_err(err)
}

#[tauri::command]
fn delete_instance(state: State<'_, Pristine>, id: String) -> CmdResult<()> {
    Instance::delete(&state.paths, &id).map_err(err)
}

#[tauri::command]
fn set_memory(state: State<'_, Pristine>, id: String, mb: u32) -> CmdResult<()> {
    let mut i = Instance::load(&state.paths, &id).map_err(err)?;
    i.max_memory_mb = mb.clamp(512, 65536);
    i.save(&state.paths).map_err(err)
}

fn load_instance(state: &Pristine, id: &str) -> CmdResult<Instance> {
    Instance::load(&state.paths, id).map_err(err)
}

#[tauri::command]
fn list_modules() -> &'static [ModuleDef] { modules::MODULES }

#[tauri::command]
async fn enable_module(state: State<'_, Pristine>, id: String, module: String) -> CmdResult<ModuleReport> {
    state.enable_module(&id, &module).await.map_err(err)
}

#[tauri::command]
fn disable_module(state: State<'_, Pristine>, id: String, module: String) -> CmdResult<()> {
    state.disable_module(&id, &module).map_err(err)
}

#[tauri::command]
fn set_loader(state: State<'_, Pristine>, id: String, loader: Loader) -> CmdResult<()> {
    state.set_loader(&id, loader).map_err(err)
}

#[tauri::command]
async fn search_content(state: State<'_, Pristine>, params: SearchParams) -> CmdResult<SearchResult> {
    state.search(&params).await.map_err(err)
}

#[tauri::command]
async fn install_content(state: State<'_, Pristine>, id: String, project: String, kind: Kind, world: Option<String>) -> CmdResult<Vec<String>> {
    state.install_content(&id, &project, kind, world.as_deref()).await.map_err(err)
}

/// Creates a new instance from a modpack. Progress is emitted under the id `__modpack`.
#[tauri::command]
async fn install_modpack(app: AppHandle, state: State<'_, Pristine>, project: String) -> CmdResult<Instance> {
    state.install_modpack(&project, progress_emitter(app, "__modpack".into())).await.map_err(err)
}

#[tauri::command]
fn list_content(state: State<'_, Pristine>, id: String) -> CmdResult<Vec<ContentView>> {
    Ok(content::list(&state.paths, &load_instance(&state, &id)?))
}

#[tauri::command]
fn list_worlds(state: State<'_, Pristine>, id: String) -> CmdResult<Vec<String>> {
    Ok(content::list_worlds(&state.paths, &load_instance(&state, &id)?))
}

#[tauri::command]
fn toggle_content(state: State<'_, Pristine>, id: String, file: String) -> CmdResult<()> {
    content::toggle(&state.paths, &load_instance(&state, &id)?, &file).map_err(err)
}

#[tauri::command]
fn remove_content(state: State<'_, Pristine>, id: String, file: String) -> CmdResult<()> {
    content::remove(&state.paths, &load_instance(&state, &id)?, &file).map_err(err)
}

#[tauri::command]
fn open_instance_folder(app: AppHandle, state: State<'_, Pristine>, id: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let dir = state.paths.game_dir(&load_instance(&state, &id)?.id);
    std::fs::create_dir_all(&dir).map_err(err)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(err)
}

#[tauri::command]
async fn install_instance(app: AppHandle, state: State<'_, Pristine>, id: String) -> CmdResult<()> {
    state.install_instance(&id, progress_emitter(app, id.clone())).await.map_err(err)
}

#[tauri::command]
async fn launch_instance(app: AppHandle, state: State<'_, Pristine>, games: State<'_, Games>, logs: State<'_, Logs>, id: String, username: String) -> CmdResult<()> {
    // Use the selected Microsoft account if there is one, otherwise an offline account.
    let selected = AccountStore::load(&state.paths).selected;
    let account = match selected {
        Some(uuid) => state.launch_account(&uuid).await.map_err(err)?,
        None => {
            let name = username.trim();
            if name.is_empty() || name.len() > 16 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err("Username must be 1-16 letters, numbers or underscores".into());
            }
            Account::offline(name)
        }
    };
    let progress = progress_emitter(app.clone(), id.clone());
    state.install_instance(&id, progress.clone()).await.map_err(err)?;
    let cmd = state.prepare_launch(&id, &account, progress).await.map_err(err)?;
    let mut child = cmd.spawn_piped().map_err(err)?;
    if let Ok(mut inst) = Instance::load(&state.paths, &id) {
        inst.last_played = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
        let _ = inst.save(&state.paths);
    }
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let child = Arc::new(Mutex::new(child));
    games.0.lock().unwrap().insert(id.clone(), child.clone());
    logs.0.lock().unwrap().insert(id.clone(), Vec::new());

    let (tx, rx) = channel::<(String, bool)>();
    if let Some(o) = stdout { pump(o, false, tx.clone()); }
    if let Some(e) = stderr { pump(e, true, tx.clone()); }
    drop(tx);
    let (app_log, log_id) = (app.clone(), id.clone());
    std::thread::spawn(move || aggregate(app_log, log_id, rx));

    let exit_id = id.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(300));
        let status = child.lock().unwrap().try_wait();
        match status {
            Ok(None) => continue,
            other => {
                let code = other.ok().flatten().and_then(|s| s.code());
                app.state::<Games>().0.lock().unwrap().remove(&exit_id);
                let _ = app.emit("game-exit", ExitEvent { id: exit_id, code });
                break;
            }
        }
    });
    Ok(())
}

/// Read a pipe line by line (lossily, so odd bytes never end the stream).
fn pump<R: Read + Send + 'static>(r: R, is_err: bool, tx: Sender<(String, bool)>) {
    std::thread::spawn(move || {
        for line in BufReader::new(r).split(b'\n') {
            let Ok(bytes) = line else { break };
            let text = String::from_utf8_lossy(&bytes).trim_end_matches('\r').to_string();
            if tx.send((text, is_err)).is_err() { break; }
        }
    });
}

fn flush(app: &AppHandle, id: &str, n: &mut u64, batch: Vec<(String, bool)>) {
    if batch.is_empty() { return; }
    let lines: Vec<LogLine> = batch.into_iter().map(|(text, is_err)| {
        let (ts, level) = classify(&text, is_err);
        *n += 1;
        LogLine { n: *n, ts, level, text }
    }).collect();
    {
        let logs = app.state::<Logs>();
        let mut map = logs.0.lock().unwrap();
        let buf = map.entry(id.to_string()).or_default();
        buf.extend(lines.iter().cloned());
        if buf.len() > LOG_CAP { let extra = buf.len() - LOG_CAP; buf.drain(..extra); }
    }
    let _ = app.emit("game-log", LogEvent { id: id.to_string(), lines });
}

/// Batch output lines (startup is chatty) before handing them to the UI.
fn aggregate(app: AppHandle, id: String, rx: std::sync::mpsc::Receiver<(String, bool)>) {
    let mut n = 0u64;
    loop {
        let mut batch = Vec::new();
        match rx.recv() { Ok(m) => batch.push(m), Err(_) => break }
        let started = Instant::now();
        let mut done = false;
        while batch.len() < 300 && started.elapsed() < Duration::from_millis(80) {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(m) => batch.push(m),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => { done = true; break; }
            }
        }
        flush(&app, &id, &mut n, batch);
        if done { break; }
    }
}

#[tauri::command]
fn get_log(logs: State<'_, Logs>, id: String) -> Vec<LogLine> {
    logs.0.lock().unwrap().get(&id).cloned().unwrap_or_default()
}

#[tauri::command]
fn clear_log(logs: State<'_, Logs>, id: String) {
    if let Some(b) = logs.0.lock().unwrap().get_mut(&id) { b.clear(); }
}

#[tauri::command]
fn stop_instance(games: State<'_, Games>, id: String) -> CmdResult<()> {
    if let Some(c) = games.0.lock().unwrap().get(&id) { c.lock().unwrap().kill().map_err(err)?; }
    Ok(())
}

#[tauri::command]
fn running_instances(games: State<'_, Games>) -> Vec<String> {
    games.0.lock().unwrap().keys().cloned().collect()
}

#[tauri::command]
fn list_skins(state: State<'_, Pristine>) -> Vec<SkinEntry> { skins::list(&state.paths) }

#[tauri::command]
fn save_skin(state: State<'_, Pristine>, name: String, data: String) -> CmdResult<String> {
    skins::save(&state.paths, &name, &data).map_err(err)
}

#[tauri::command]
fn delete_skin(state: State<'_, Pristine>, file: String) -> CmdResult<()> {
    skins::delete(&state.paths, &file).map_err(err)
}

#[tauri::command]
async fn apply_skin(state: State<'_, Pristine>, file: String, slim: bool) -> CmdResult<()> {
    state.apply_skin(&file, slim).await.map_err(err)
}

fn main() {
    let pristine = Pristine::new(Paths::default_location()).expect("failed to initialise Pristine");
    tauri::Builder::default()
        .manage(pristine)
        .manage(LoginState::default())
        .manage(Games::default())
        .manage(Logs::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            app_info, accounts_state, get_client_id, set_client_id, select_account, remove_account,
            ms_login_start, ms_login_cancel, list_versions, list_instances, create_instance, delete_instance,
            set_memory, install_instance, launch_instance,
            list_modules, enable_module, disable_module, set_loader, search_content, install_content,
            install_modpack, get_log, clear_log, stop_instance, running_instances,
            list_skins, save_skin, delete_skin, apply_skin, list_content, list_worlds, toggle_content, remove_content, open_instance_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pristine");
}
