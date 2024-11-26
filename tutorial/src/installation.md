# Installation

There are multiple ways to install this library.
Choose any one of the methods below that best suit your needs.

## Python Package

This is the easiest way to use the library.
All the demos are in Python, but once you become familiar with the Python interface, the Rust native interface is exactly the same.

```shell
pip3 install mwpf
```

## Build from Source using Rust

This is the recommended way for experts to install the library for full customizability.
For example, the default build of Python package uses floating point dual variables, which is good for practical usage
but falls short if you want exact rational solution. In this case, you can build your own Python package with Rational number.

### Install the Rust Toolchain

We need the Rust toolchain to compile the project written in the Rust programming language.
Please see [https://rustup.rs/](https://rustup.rs/) for the latest instructions.
An example on Unix-like operating systems is below.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.bashrc  # this will add `~/.cargo/bin` to path
```

After installing the Rust toolchain successfully, you can compile the library and binary by

```bash
cargo build --release
cargo run --release -- --help
```

### Install the Python Development Tools [Optional]

If you want to develop the Python module, you need a few more tools

```bash
sudo apt install python3 python3-pip cmake clang
pip3 install maturin
maturin develop  # build the Python package and install in your virtualenv or conda
python3 tutorial/demo.py  # run a demo using the installed library
```

### Install Frontend tools [Optional]

The frontend is a single-page application using Vue.js and Three.js frameworks.
The visualization tool is now upgraded to use Vite to compile the tool into a single library, so that no HTTP server is needed anymore.
For more information of using the visualization tool, see 

```sh
# install nodejs https://nodejs.org/en/download/package-manager
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.0/install.sh | bash
nvm install 22

# build the frontend
cd visualizer
npm i --include=dev
npm run dev  # for development
npm run build
```

### Install mdbook to build this tutorial [Optional]

In order to build this tutorial, you need to install [mdbook](https://crates.io/crates/mdbook) and several plugins.

```bash
cargo install mdbook
cargo install mdbook-bib
cd tutorial
mdbook serve  # dev mode, automatically refresh the local web page on code change
mdbook build  # build deployment in /docs folder, to be recognized by GitHub Pages
```
