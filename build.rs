use embed_manifest::{embed_manifest, manifest::ExecutionLevel, new_manifest};
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc_exe() -> Option<PathBuf> {
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }
    let kits_bin = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(kits_bin) {
        let mut versions: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("x64").join("rc.exe"))
            .filter(|p| p.exists())
            .collect();
        versions.sort();
        if let Some(best) = versions.pop() {
            return Some(best);
        }
    }
    None
}

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let level = if std::env::var_os("CARGO_FEATURE_UAC_MANIFEST").is_some() {
            ExecutionLevel::RequireAdministrator
        } else {
            ExecutionLevel::AsInvoker
        };
        let manifest = new_manifest("Yanis.WinCare").requested_execution_level(level);
        embed_manifest(manifest).expect("Impossible d'intégrer le manifeste Windows");

        // Compilation et intégration de l'icône Windows Explorer (assets/icon.ico)
        let rc_file = Path::new("assets/icon.rc");
        if rc_file.exists() {
            if let (Some(rc_exe), Ok(out_dir)) = (find_rc_exe(), std::env::var("OUT_DIR")) {
                let res_path = Path::new(&out_dir).join("icon.res");
                let status = Command::new(rc_exe)
                    .args([
                        "/nologo",
                        "/fo",
                        &res_path.to_string_lossy(),
                        "assets/icon.rc",
                    ])
                    .status();
                if let Ok(st) = status {
                    if st.success() && res_path.exists() {
                        println!("cargo:rustc-link-arg-bins={}", res_path.display());
                    }
                }
            }
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/icon.rc");
}
