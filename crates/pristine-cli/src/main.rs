use clap::{Parser, Subcommand};
use pristine_core::modrinth::{Kind, SearchParams};
use pristine_core::{content, Account, Instance, Loader, Paths, Pristine, Progress, ProgressFn};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "pristine", version, about = "Pristine: a fast Minecraft launcher")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List Minecraft versions
    Versions {
        /// Include snapshots, betas and alphas
        #[arg(long)]
        all: bool,
    },
    /// Create an instance
    Create {
        name: String,
        version: String,
        /// vanilla | fabric | quilt
        #[arg(long, default_value = "vanilla")]
        loader: String,
    },
    /// Search Modrinth
    Search {
        query: String,
        #[arg(long, default_value = "mod")]
        kind: String,
        #[arg(long)]
        mc: Option<String>,
    },
    /// Install a Modrinth project into an instance
    Add {
        id: String,
        project: String,
        #[arg(long, default_value = "mod")]
        kind: String,
        #[arg(long)]
        world: Option<String>,
    },
    /// Enable (or --off disable) a client module on an instance
    Module {
        id: String,
        module: String,
        #[arg(long)]
        off: bool,
    },
    /// Create an instance from a Modrinth modpack
    Modpack { project: String },
    /// List installed content of an instance
    Content { id: String },
    /// List instances
    List,
    /// Delete an instance
    Delete { id: String },
    /// Download game files and Java for an instance
    Install { id: String },
    /// Launch an instance (installs first if needed)
    Launch {
        id: String,
        /// Offline username
        #[arg(long, default_value = "Player")]
        offline: String,
        /// Print the command instead of running it
        #[arg(long)]
        dry_run: bool,
    },
}

fn parse_loader(s: &str) -> Result<Loader, String> {
    match s { "vanilla" => Ok(Loader::Vanilla), "fabric" => Ok(Loader::Fabric), "quilt" => Ok(Loader::Quilt), o => Err(format!("unknown loader {o}")) }
}

fn parse_kind(s: &str) -> Result<Kind, String> {
    match s { "mod" => Ok(Kind::Mod), "modpack" => Ok(Kind::Modpack), "resourcepack" => Ok(Kind::ResourcePack),
        "shader" => Ok(Kind::Shader), "datapack" => Ok(Kind::DataPack), o => Err(format!("unknown kind {o}")) }
}

fn progress() -> ProgressFn {
    Arc::new(|p: Progress| {
        eprint!("\r{:<16} {}/{}   ", p.stage, p.done, p.total);
        if p.done == p.total { eprintln!(); }
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let app = Pristine::new(Paths::default_location())?;
    match cli.cmd {
        Cmd::Versions { all } => {
            let m = app.versions().await?;
            println!("latest release: {}   latest snapshot: {}", m.latest.release, m.latest.snapshot);
            for v in m.versions.iter().filter(|v| all || v.kind == "release").take(if all { usize::MAX } else { 30 }) {
                println!("{:<14} {}", v.id, v.kind);
            }
        }
        Cmd::Create { name, version, loader } => {
            let loader = parse_loader(&loader)?;
            let m = app.versions().await?;
            if m.find(&version).is_none() {
                return Err(format!("unknown version {version}").into());
            }
            let i = Instance::create_with(&app.paths, &name, &version, loader)?;
            println!("created instance '{}' (id: {}, Minecraft {}, {})", i.name, i.id, i.mc_version, i.loader.label());
        }
        Cmd::Search { query, kind, mc } => {
            let r = app.search(&SearchParams { query, kind: parse_kind(&kind)?, mc_version: mc, loader: None, offset: 0, index: None, categories: vec![] }).await?;
            println!("{} results", r.total);
            for h in r.hits.iter().take(10) { println!("{:<28} {:>10} dl  {}", h.slug, h.downloads, h.title); }
        }
        Cmd::Add { id, project, kind, world } => {
            let names = app.install_content(&id, &project, parse_kind(&kind)?, world.as_deref()).await?;
            println!("installed: {}", names.join(", "));
        }
        Cmd::Module { id, module, off } => {
            if off { app.disable_module(&id, &module)?; println!("disabled {module}"); }
            else {
                let r = app.enable_module(&id, &module).await?;
                println!("installed: {}", r.installed.join(", "));
                for s in r.skipped { println!("skipped: {s}"); }
            }
        }
        Cmd::Modpack { project } => {
            let i = app.install_modpack(&project, progress()).await?;
            println!("created '{}' (id: {}, Minecraft {}, {})", i.name, i.id, i.mc_version, i.loader.label());
        }
        Cmd::Content { id } => {
            let inst = Instance::load(&app.paths, &id)?;
            for c in content::list(&app.paths, &inst) {
                println!("{} {:<36} {:<12} {}", if c.enabled { "on " } else { "off" }, c.title, c.version_number.unwrap_or_default(), c.file);
            }
        }
        Cmd::List => {
            for i in Instance::list(&app.paths)? {
                println!("{:<20} {:<14} {}", i.id, i.mc_version, i.name);
            }
        }
        Cmd::Delete { id } => Instance::delete(&app.paths, &id)?,
        Cmd::Install { id } => app.install_instance(&id, progress()).await?,
        Cmd::Launch { id, offline, dry_run } => {
            app.install_instance(&id, progress()).await?;
            let cmd = app.prepare_launch(&id, &Account::offline(&offline), progress()).await?;
            if dry_run {
                println!("{} {}", cmd.program.display(), cmd.args.join(" "));
            } else {
                cmd.spawn()?.wait()?;
            }
        }
    }
    Ok(())
}
