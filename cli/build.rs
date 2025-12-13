use std::env;
use std::path::PathBuf;

fn main() {
    // Point Tauri to the pre-built frontend assets in the parent directory
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let parent_dir = manifest_dir.parent().unwrap();
    let dist_dir = parent_dir.join("dist");
    
    // Verify that dist directory exists
    if !dist_dir.exists() {
        panic!(
            "Frontend assets not found at {:?}.\n\
             For local development: run 'npm install && npm run build' in the root directory.\n\
             For published crate: the dist/ directory must be committed to the repository.",
            dist_dir
        );
    }
    
    // Set cargo directives
    println!("cargo:rerun-if-changed=../dist");
    println!("cargo:rerun-if-changed=Tauri.toml");
    
    tauri_build::build()
}
