extern crate clap;
extern crate pbr;

use crate::clap::Parser;
use highs::highs_release_resources;
use mwpf::cli::*;

pub fn main() {
    Cli::parse().run();
    #[cfg(feature = "highs")]
    unsafe {
        highs_release_resources(true)
    };
}
