fn main() {
    // fix highs build error on MacOS
    println!("cargo:rustc-link-search=all=/opt/homebrew/opt/libomp/lib");
}
