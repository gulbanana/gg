use tauri_build::Attributes;

fn main() {
    // docs.rs mounts the source tree read-only; tauri_build writes to gen/schemas/ which would panic.
    if std::env::var_os("DOCS_RS").is_some() {
        println!("cargo:rustc-cfg=docsrs");
        return;
    }

    tauri_build::try_build(Attributes::new().capabilities_path_pattern("gen")).unwrap()
}
