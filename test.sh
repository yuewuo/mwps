#!/bin/sh
set -ex

cargo clean

cargo test
cargo test --features u32_index
cargo test --features r64_weight
