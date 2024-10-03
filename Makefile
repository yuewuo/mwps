all: test check build python wasm

fmt:
	cargo fmt --check

# A collection of lints to catch common mistakes and improve your Rust code.
clippy:
	cargo clippy -- -Dwarnings
	cargo clippy --all-targets --features=python_binding,u32_index -- -D warnings

clean:
	cargo clean

clean-env: clean fmt

test: clean-env
	cargo test
	cargo test --features u32_index
	cargo test --features r64_weight

build: clean-env
	cargo build
	cargo build --release

# build test binary
	cargo test --no-run --features u32_index
	cargo test --no-run --features u32_index --release
	cargo test --no-run
	cargo test --no-run --release
	cargo test --no-run --features r64_weight
	cargo test --no-run --features r64_weight --release
	cargo test --no-run --features python_binding
	cargo test --no-run --features python_binding --release

check: clean-env
	cargo check
	# cargo check --features r64_weight
	# cargo check --features u32_index
	# cargo check --lib --no-default-features --features wasm_binding
	# cargo check --lib --no-default-features --features wasm_binding,u32_index
	cargo check --features cluster_size_limit
	cargo check --release

python: clean-env
	maturin develop
	# pytest tests/python

wasm: clean-env
	wasm-pack build --no-default-features --features wasm_binding,u32_index

# test code coverage: see https://lib.rs/crates/cargo-llvm-cov
coverage:
	cargo llvm-cov --html
	# open target/llvm-cov/html/index.html
