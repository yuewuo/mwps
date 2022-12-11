use crate::clap;
use crate::clap::{Parser, Subcommand};


#[derive(Parser, Clone)]
#[clap(author = clap::crate_authors!(", "))]
#[clap(version = env!("CARGO_PKG_VERSION"))]
#[clap(about = "Minimum-Weight Parity Subgraph Algorithm for Quantum Error Correction Decoding")]
#[clap(color = clap::ColorChoice::Auto)]
#[clap(propagate_version = true)]
#[clap(subcommand_required = true)]
#[clap(arg_required_else_help = true)]
pub struct Cli {
    #[clap(subcommand)]
    command: Commands,
}


#[derive(Subcommand, Clone)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// benchmark the speed (and also correctness if enabled)
    Benchmark {

    },
}


impl Cli {
    pub fn run(self) {
        match self.command {
            Commands::Benchmark { } => {
                unimplemented!()
            }
        }
    }
}
