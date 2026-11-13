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

/// Where the game should go straight after it starts, skipping the title screen.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(tag = "kind", content = "target", rename_all = "lowercase")]
pub enum QuickPlay {
    /// A server address, `host` or `host:port`.
    Server(String),
    /// A world folder name inside the instance's `saves`.
    World(String),
}

impl QuickPlay {
    fn feature(&self) -> &'static str {
        match self { QuickPlay::Server(_) => "is_quick_play_multiplayer", QuickPlay::World(_) => "is_quick_play_singleplayer" }
    }
}

/// `host:port` split, defaulting to Minecraft's port.
pub fn split_address(addr: &str) -> (String, u16) {
    let addr = addr.trim();
    // Bracketed IPv6: [::1]:25565
    if let Some(rest) = addr.strip_prefix('[') {
        if let Some((h, p)) = rest.split_once(']') {
            let port = p.trim_start_matches(':').parse().unwrap_or(25565);
            return (h.to_string(), port);
        }
    }
    match addr.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => (h.to_string(), p.parse().unwrap_or(25565)),
        _ => (addr.to_string(), 25565),
    }
}

#[derive(Debug, Clone)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

impl LaunchCommand {
    /// Spawn with stdout/stderr captured so a launcher can show the game's output.
    pub fn spawn_piped(&self) -> Result<Child> {
        use std::process::Stdio;
        Ok(Command::new(&self.program).args(&self.args).current_dir(&self.cwd)
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?)
    }

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
    quick: Option<&QuickPlay>,
) -> LaunchCommand {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let cp = classpath(paths, meta)
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(sep);
    let game_dir = paths.game_dir(&instance.id);

    let mut vars: HashMap<&str, String> = HashMap::from([
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

    if let Some(q) = quick {
        let target = match q { QuickPlay::Server(s) | QuickPlay::World(s) => s.clone() };
        let keys = match q {
            QuickPlay::Server(_) => ["quickPlayMultiplayer", "quick_play_multiplayer"],
            QuickPlay::World(_) => ["quickPlaySingleplayer", "quick_play_singleplayer"],
        };
        for k in keys { vars.insert(k, target.clone()); }
    }
    let features: Vec<&str> = quick.map(|q| vec![q.feature()]).unwrap_or_default();
    let (jvm_raw, mut game_raw): (Vec<String>, Vec<String>) = match &meta.arguments {
        Some(a) => (
            a.jvm.iter().flat_map(|x| x.resolve_with(&features)).collect(),
            a.game.iter().flat_map(|x| x.resolve_with(&features)).collect(),
        ),
        None => (
            // Pre-1.13 versions carry no JVM arguments of their own.
            vec!["-Djava.library.path=${natives_directory}".into(), "-cp".into(), "${classpath}".into()],
            meta.minecraft_arguments.as_deref().unwrap_or("").split_whitespace().map(String::from).collect(),
        ),
    };

    // Versions without quick play (before 1.20) still take --server/--port.
    if let Some(QuickPlay::Server(addr)) = quick {
        if !game_raw.iter().any(|a| a == "--quickPlayMultiplayer") {
            let (host, port) = split_address(addr);
            game_raw.extend(["--server".into(), host, "--port".into(), port.to_string()]);
        }
    }

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

    #[cfg(unix)]
    #[test]
    fn spawn_piped_captures_both_streams() {
        use std::io::Read;
        let cmd = LaunchCommand { program: "sh".into(), args: vec!["-c".into(), "echo out; echo err 1>&2".into()], cwd: std::env::temp_dir() };
        let mut child = cmd.spawn_piped().unwrap();
        let (mut o, mut e) = (String::new(), String::new());
        child.stdout.take().unwrap().read_to_string(&mut o).unwrap();
        child.stderr.take().unwrap().read_to_string(&mut e).unwrap();
        child.wait().unwrap();
        assert_eq!((o.trim(), e.trim()), ("out", "err"));
    }

    fn meta(args: &str) -> VersionMeta {
        serde_json::from_str(&format!(r#"{{"id":"1.21.4","mainClass":"net.minecraft.client.main.Main","assetIndex":{{"id":"19","sha1":"x","size":1,"url":"https://x"}},
            "downloads":{{"client":{{"sha1":"x","size":1,"url":"https://x"}}}},{args}}}"#)).unwrap()
    }

    fn build(m: &VersionMeta, q: Option<&QuickPlay>) -> Vec<String> {
        let root = std::env::temp_dir().join(format!("pristine-launch-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(&root);
        let inst = Instance::create_with(&paths, "T", &m.id, crate::loader::Loader::Vanilla).unwrap();
        build_command(&paths, m, &inst, "java".into(), &Account::offline("Steve"), q).args
    }

    #[test]
    fn quick_play_joins_a_server_or_world() {
        let m = meta(r#""arguments":{"game":["--username","${auth_player_name}",
            {"rules":[{"action":"allow","features":{"is_quick_play_singleplayer":true}}],"value":["--quickPlaySingleplayer","${quickPlaySingleplayer}"]},
            {"rules":[{"action":"allow","features":{"is_quick_play_multiplayer":true}}],"value":["--quickPlayMultiplayer","${quickPlayMultiplayer}"]}],"jvm":[]}"#);
        let plain = build(&m, None);
        assert!(!plain.iter().any(|a| a.starts_with("--quickPlay")));
        let mp = build(&m, Some(&QuickPlay::Server("mc.example.net:25570".into())));
        let i = mp.iter().position(|a| a == "--quickPlayMultiplayer").unwrap();
        assert_eq!(mp[i + 1], "mc.example.net:25570");
        assert!(!mp.iter().any(|a| a == "--quickPlaySingleplayer" || a == "--server"));
        let sp = build(&m, Some(&QuickPlay::World("My World".into())));
        let i = sp.iter().position(|a| a == "--quickPlaySingleplayer").unwrap();
        assert_eq!(sp[i + 1], "My World");
    }

    #[test]
    fn old_versions_join_with_server_and_port() {
        let m = meta(r#""minecraftArguments":"--username ${auth_player_name}""#);
        let a = build(&m, Some(&QuickPlay::Server("mc.example.net:25570".into())));
        let i = a.iter().position(|x| x == "--server").unwrap();
        assert_eq!(&a[i..i + 4], ["--server", "mc.example.net", "--port", "25570"]);
    }

    #[test]
    fn splits_addresses() {
        assert_eq!(split_address("play.example.net"), ("play.example.net".into(), 25565));
        assert_eq!(split_address("1.2.3.4:25570"), ("1.2.3.4".into(), 25570));
        assert_eq!(split_address("[::1]:25566"), ("::1".into(), 25566));
    }

    #[test]
    fn quick_play_serializes_for_the_ui() {
        let q: QuickPlay = serde_json::from_str(r#"{"kind":"server","target":"mc.example.net"}"#).unwrap();
        assert_eq!(q, QuickPlay::Server("mc.example.net".into()));
    }

    #[test]
    fn substitutes_vars() {
        let vars = HashMap::from([("a", "1".to_string()), ("b", "two".to_string())]);
        assert_eq!(substitute("x=${a};y=${b};z=${c}", &vars), "x=1;y=two;z=${c}");
    }
}
