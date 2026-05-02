use clap::Parser;

use fathomdb_cli::{run, Cli};

fn main() {
    let cli = Cli::parse();
    std::process::exit(run(cli));
}
