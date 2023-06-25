#!/bin/sh
set -ex

cargo fmt --check
cargo clippy -- -Dwarnings  # A collection of lints to catch common mistakes and improve your Rust code.

cargo check
cargo check --release
