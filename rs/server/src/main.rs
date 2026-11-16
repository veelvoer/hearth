use clap::Parser;
use hearth_server::{cli, ui};

fn main() {
    let c = cli::Cli::parse();
    match cli::run(c) {
        Ok(code) => std::process::exit(code),
        Err(e) => { ui::bad(&e.to_string()); std::process::exit(1); }
    }
}
