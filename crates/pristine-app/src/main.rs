// Prevents an extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pristine_core::{Account, Instance, Paths, Pristine, Progress, ProgressFn};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

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
    let name = username.trim();
    if name.is_empty() || name.len() > 16 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Username must be 1-16 letters, numbers or underscores".into());
    }
    let progress = progress_emitter(app.clone(), id.clone());
    state.install_instance(&id, progress.clone()).await.map_err(err)?;
    let cmd = state.prepare_launch(&id, &Account::offline(name), progress).await.map_err(err)?;
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
        .invoke_handler(tauri::generate_handler![
            app_info, list_versions, list_instances, create_instance, delete_instance,
            set_memory, install_instance, launch_instance
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pristine");
}
