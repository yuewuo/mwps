#!/bin/sh
set -ex

cargo fmt --check
cargo clippy -- -Dwarnings  # A collection of lints to catch common mistakes and improve your Rust code.

# check this first because it's easy to have errors
cargo check
cargo check --features r64_weight
cargo check --features u32_index

cargo check --release
