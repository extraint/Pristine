// Prevents an extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pristine_core::accounts::AccountStore;
use pristine_core::msa::DeviceCode;
use pristine_core::{Account, Instance, Paths, Pristine, Progress, ProgressFn};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

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
fn create_instance(state: State<'_, Pristine>, name: String, version: String) -> CmdResult<Instance> {
    if name.trim().is_empty() { return Err("Give the instance a name".into()); }
    Instance::create(&state.paths, &name, &version).map_err(err)
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

#[tauri::command]
async fn install_instance(app: AppHandle, state: State<'_, Pristine>, id: String) -> CmdResult<()> {
    state.install_instance(&id, progress_emitter(app, id.clone())).await.map_err(err)
}

#[tauri::command]
async fn launch_instance(app: AppHandle, state: State<'_, Pristine>, id: String, username: String) -> CmdResult<()> {
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
    let mut child = cmd.spawn().map_err(err)?;
    if let Ok(mut inst) = Instance::load(&state.paths, &id) {
        inst.last_played = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
        let _ = inst.save(&state.paths);
    }
    let exit_id = id.clone();
    std::thread::spawn(move || {
        let code = child.wait().ok().and_then(|s| s.code());
        let _ = app.emit("game-exit", ExitEvent { id: exit_id, code });
    });
    Ok(())
}

fn main() {
    let pristine = Pristine::new(Paths::default_location()).expect("failed to initialise Pristine");
    tauri::Builder::default()
        .manage(pristine)
        .manage(LoginState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            app_info, accounts_state, get_client_id, set_client_id, select_account, remove_account,
            ms_login_start, ms_login_cancel, list_versions, list_instances, create_instance, delete_instance,
            set_memory, install_instance, launch_instance
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pristine");
}
