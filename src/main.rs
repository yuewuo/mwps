extern crate clap;
extern crate pbr;

use mwps::cli::*;
use crate::clap::Parser;


pub fn main() {

    Cli::parse().run();

}
