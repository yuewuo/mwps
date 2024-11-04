fn main() {
    // fix highs build error on MacOS
    println!("cargo:rustc-link-search=all=/opt/homebrew/opt/libomp/lib");

    // when embedded visualizer is enabled, build frontend code first
    if cfg!(feature = "embed_visualizer") {
        // respond to frontend code changes
        println!("cargo:rerun-if-changed=./visualize/src"); // the whole src folder
        for file in std::fs::read_dir("./visualize").unwrap() {
            // also files in visualizer folder (but not any folders in it)
            let path = file.unwrap().path().display().to_string();
            if std::fs::metadata(path.as_str()).unwrap().is_file() && !path.ends_with("package-lock.json") {
                println!("cargo:rerun-if-changed={path}");
            }
        }

        if !std::env::var("SKIP_FRONTEND_BUILD").is_ok() {
            assert!(std::process::Command::new("npm")
                .current_dir("./visualize")
                .arg("install")
                .arg("--include=dev")
                .status()
                .unwrap()
                .success());

            assert!(std::process::Command::new("npm")
                .current_dir("./visualize")
                .arg("run")
                .arg("build")
                .status()
                .unwrap()
                .success());
        }
    }
}
