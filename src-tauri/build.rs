fn main() {
    link_compiler_rt_on_macos();
    tauri_build::build()
}

/// The prebuilt ONNX Runtime static library (via `ort`, CoreML provider) uses `@available`,
/// which compiles to `___isPlatformVersionAtLeast` from compiler-rt. Rust's linker invocation
/// does not pull in clang's runtime, so release builds fail to link without this.
fn link_compiler_rt_on_macos() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(output) = std::process::Command::new("xcrun")
        .args(["clang", "--print-resource-dir"])
        .output()
    else {
        println!("cargo:warning=xcrun not found; skipping compiler-rt link");
        return;
    };
    let resource_dir = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let lib_dir = std::path::Path::new(&resource_dir).join("lib/darwin");
    if lib_dir.join("libclang_rt.osx.a").is_file() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    } else {
        println!(
            "cargo:warning=libclang_rt.osx.a not found in {}",
            lib_dir.display()
        );
    }
}
