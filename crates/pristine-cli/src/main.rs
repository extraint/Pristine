use clap::{Parser, Subcommand};
use pristine_core::{Account, Instance, Paths, Pristine, Progress, ProgressFn};
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
    Create { name: String, version: String },
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
        Cmd::Create { name, version } => {
            let m = app.versions().await?;
            if m.find(&version).is_none() {
                return Err(format!("unknown version {version}").into());
            }
            let i = Instance::create(&app.paths, &name, &version)?;
            println!("created instance '{}' (id: {}, Minecraft {})", i.name, i.id, i.mc_version);
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
