use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use xtask::*;

#[derive(Parser)]
#[command(name = "xtask", about = "Development and release tasks for Hearth (replaces scripts/*.js)")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Set the version everywhere: feature | fix | major | X.Y.Z   (scripts/set-version.js)
    SetVersion { spec: String },
    /// Check that the tag, release notes and every version file agree (the release workflow's `check` job)
    CheckRelease {
        /// The tag, like v1.41.0 (default: $GITHUB_REF_NAME, else the version now in the files)
        #[arg(long)]
        tag: Option<String>,
    },
    /// Pack desktop/relay/mcpkit/ into desktop/relay/mcpkit-files.js   (scripts/build-mcpkit.js)
    BuildMcpkit {
        /// Only check that the packed file is up to date
        #[arg(long)]
        check: bool,
    },
    /// Turn Lucide SVG files into desktop markup and phone path strings   (scripts/lucide-gen.js)
    LucideGen {
        /// Folder with the Lucide .svg files (package/icons)
        dir: PathBuf,
        /// The map as JSON text, or a file like scripts/lucide-map.json (then --key picks the part, default phone)
        map: String,
        #[arg(long, default_value = "phone")]
        key: String,
    },
    /// Connect the support mail list to Google Tasks   (scripts/google-tasks-token.js)
    GoogleTasksToken { client_id: String, client_secret: String, server: Option<String>, token: Option<String> },
    /// Find names used but never defined in the old desktop screens (needs eslint through npx)   (scripts/lint-renderer.js)
    LintRenderer,
}

fn main() {
    let r: Result<i32> = (|| match Cli::parse().command {
        Cmd::SetVersion { spec } => { version::set_version(&version::find_root()?, &spec)?; Ok(0) }
        Cmd::CheckRelease { tag } => { let root = version::find_root()?; let tag = tag.or_else(|| std::env::var("GITHUB_REF_NAME").ok().filter(|t| !t.is_empty())); let errs = version::check_release(&root, tag.as_deref())?; if errs.is_empty() { println!("Release check passed."); Ok(0) } else { for e in &errs { eprintln!("{e}"); } Ok(1) } }
        Cmd::BuildMcpkit { check } => { let root = version::find_root()?; if check { let ok = mcpkit::is_current(&root)?; println!("{}", if ok { "mcpkit-files.js is up to date" } else { "mcpkit-files.js is out of date: run cargo xtask build-mcpkit" }); Ok(if ok { 0 } else { 1 }) } else { println!("{}", mcpkit::build(&root)?); Ok(0) } }
        Cmd::LucideGen { dir, map, key } => { print!("{}", lucide::run(&dir, &lucide::load_map(&map, &key)?)?); Ok(0) }
        Cmd::GoogleTasksToken { client_id, client_secret, server, token } => google::run(&google::Args { client_id, client_secret, server, token }, &google::Endpoints::real(), &mut |url| println!("Open this address in your browser and allow access:\n{url}")),
        Cmd::LintRenderer => lint::run(&version::find_root()?),
    })();
    match r { Ok(c) => std::process::exit(c), Err(e) => { eprintln!("{e}"); std::process::exit(1); } }
}
