use tauri_build::Attributes;

fn main() {
    tauri_build::try_build(Attributes::new().capabilities_path_pattern("gen")).unwrap()
}
