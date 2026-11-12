//! Builds the java command line for an instance.

use crate::auth::Account;
use crate::error::Result;
use crate::instance::Instance;
use crate::manifest::*;
use crate::paths::Paths;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command};

pub const LAUNCHER_NAME: &str = "Pristine";

#[derive(Debug, Clone)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

impl LaunchCommand {
    pub fn spawn(&self) -> Result<Child> {
        Ok(Command::new(&self.program).args(&self.args).current_dir(&self.cwd).spawn()?)
    }
}

fn substitute(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

pub fn classpath(paths: &Paths, meta: &VersionMeta) -> Vec<PathBuf> {
    let mut cp: Vec<PathBuf> = Vec::new();
    for lib in meta.libraries.iter().filter(|l| l.is_allowed()) {
        if let Some(a) = lib.artifact() {
            let p = paths.libraries().join(&a.path);
            if !cp.contains(&p) { cp.push(p); }
        }
    }
    cp.push(paths.client_jar(&meta.id));
    cp
}

pub fn build_command(
    paths: &Paths,
    meta: &VersionMeta,
    instance: &Instance,
    java: PathBuf,
    account: &Account,
) -> LaunchCommand {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let cp = classpath(paths, meta)
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(sep);
    let game_dir = paths.game_dir(&instance.id);

    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", account.username.clone()),
        ("auth_uuid", account.uuid.simple().to_string()),
        ("auth_access_token", account.access_token.clone()),
        ("auth_xuid", "0".into()),
        ("clientid", "".into()),
        ("user_type", account.user_type.to_string()),
        ("user_properties", "{}".into()),
        ("version_name", meta.id.clone()),
        ("version_type", if meta.kind.is_empty() { "release".into() } else { meta.kind.clone() }),
        ("game_directory", game_dir.to_string_lossy().into_owned()),
        ("assets_root", paths.assets().to_string_lossy().into_owned()),
        ("game_assets", paths.assets().join("virtual").join("legacy").to_string_lossy().into_owned()),
        ("assets_index_name", meta.asset_index.id.clone()),
        ("natives_directory", paths.natives_dir(&meta.id).to_string_lossy().into_owned()),
        ("library_directory", paths.libraries().to_string_lossy().into_owned()),
        ("classpath_separator", sep.to_string()),
        ("classpath", cp),
        ("launcher_name", LAUNCHER_NAME.to_string()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_string()),
    ]);

    let (jvm_raw, game_raw): (Vec<String>, Vec<String>) = match &meta.arguments {
        Some(a) => (
            a.jvm.iter().flat_map(|x| x.resolve()).collect(),
            a.game.iter().flat_map(|x| x.resolve()).collect(),
        ),
        None => (
            // Pre-1.13 versions carry no JVM arguments of their own.
            vec!["-Djava.library.path=${natives_directory}".into(), "-cp".into(), "${classpath}".into()],
            meta.minecraft_arguments.as_deref().unwrap_or("").split_whitespace().map(String::from).collect(),
        ),
    };

    let mut args = vec![format!("-Xmx{}M", instance.max_memory_mb), "-Xms512M".to_string()];
    args.extend(crate::modules::jvm_flags(&instance.modules));
    args.extend(instance.extra_jvm_args.iter().cloned());
    args.extend(jvm_raw.iter().map(|a| substitute(a, &vars)));
    args.push(meta.main_class.clone());
    args.extend(game_raw.iter().map(|a| substitute(a, &vars)));

    LaunchCommand { program: java, args, cwd: game_dir }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_vars() {
        let vars = HashMap::from([("a", "1".to_string()), ("b", "two".to_string())]);
        assert_eq!(substitute("x=${a};y=${b};z=${c}", &vars), "x=1;y=two;z=${c}");
    }
}
