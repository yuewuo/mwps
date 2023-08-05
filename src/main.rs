extern crate clap;
extern crate pbr;

use crate::clap::Parser;
use mwpf::cli::*;

pub fn main() {
    Cli::parse().run();
}
