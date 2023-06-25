extern crate clap;
extern crate pbr;

use crate::clap::Parser;
use mwps::cli::*;

pub fn main() {
    Cli::parse().run();
}
