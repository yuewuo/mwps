extern crate clap;

use crate::clap::Parser;
use mwpf_rational::cli::*;

pub fn main() {
    #[cfg(all(feature = "slp", feature = "incr_lp"))]
    panic!("slp does not support incr_lp!");

    Cli::parse().run();
}
