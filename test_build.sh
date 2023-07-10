#!/bin/sh
set -ex

cargo clean
cargo clippy -- -Dwarnings  # A collection of lints to catch common mistakes and improve your Rust code.

# check this first because it's easy to have errors
cargo test --no-run --features u32_index
cargo test --no-run --features u32_index --release

cargo test --no-run
cargo test --no-run --release
cargo test --no-run --features r64_weight
cargo test --no-run --features r64_weight --release
# cargo test --no-run --features python_binding
# cargo test --no-run --features python_binding --release

wasm-pack build --no-default-features --features wasm_binding,u32_index
