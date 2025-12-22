use tauri_build::Attributes;

fn main() {
    // Detect which binary is being built and set a cfg flag
    if let Ok(bin_name) = std::env::var("CARGO_BIN_NAME") {
        if bin_name == "gg-gui" {
            println!("cargo:rustc-cfg=is_gui_binary");
        }
    }
    
    tauri_build::try_build(Attributes::new().capabilities_path_pattern("gen")).unwrap()
}
