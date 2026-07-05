fn main() {
    println!("cargo::rustc-check-cfg=cfg(tarpaulin)");
    tauri_build::build()
}
