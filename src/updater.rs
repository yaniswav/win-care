use crate::runner::{Runner, CREATE_NO_WINDOW};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub const GITHUB_REPO: &str = "yaniswav/win-care";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub download_url: String,
    pub release_notes: String,
    pub update_available: bool,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Supprime au démarrage les anciens binaires `.old` laissés par les mises à jour précédentes.
pub fn cleanup_old_binary() {
    if let Ok(exe) = std::env::current_exe() {
        let old_path = exe.with_extension("exe.old");
        let _ = fs::remove_file(old_path);
        if let Some(parent) = exe.parent() {
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                        if name.ends_with(".old") && name.to_lowercase().contains("wincare") {
                            let _ = fs::remove_file(p);
                        }
                    }
                }
            }
        }
    }
}

fn parse_semver(v: &str) -> (u32, u32, u32) {
    let clean = v.trim().trim_start_matches(['v', 'V']);
    let mut parts = clean.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Vérifie sur GitHub Releases (`yaniswav/win-care`) si une nouvelle version de WinCare est disponible.
pub fn check_for_update(runner: &Runner) -> Option<UpdateInfo> {
    runner.logger.info(format!(
        "Vérification des mises à jour sur GitHub ({GITHUB_REPO}, version actuelle v{CURRENT_VERSION})..."
    ));

    let api_url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let out = runner.run_query(
        "curl.exe",
        &[
            "-sL",
            "--max-time",
            "10",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: WinCare-Updater",
            &api_url,
        ],
    );

    if !out.success() || out.stdout.trim().is_empty() {
        runner
            .logger
            .warn("Impossible de joindre l'API GitHub Releases.");
        return None;
    }

    let release: GithubRelease = match serde_json::from_str(out.stdout.trim()) {
        Ok(r) => r,
        Err(_) => {
            runner
                .logger
                .info("Aucune release publique trouvée ou réponse GitHub invalide.");
            return None;
        }
    };

    let latest_clean = release.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    let cur_ver = parse_semver(CURRENT_VERSION);
    let new_ver = parse_semver(&latest_clean);
    let update_available = new_ver > cur_ver;

    let download_url = release
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(".exe"))
        .map(|a| a.browser_download_url.clone())
        .unwrap_or_default();

    if update_available {
        runner.logger.success(format!(
            "Nouvelle version disponible : v{latest_clean} (actuelle : v{CURRENT_VERSION})"
        ));
    } else {
        runner.logger.success(format!(
            "WinCare est à jour (v{CURRENT_VERSION})."
        ));
    }

    Some(UpdateInfo {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_clean,
        release_url: release.html_url,
        download_url,
        release_notes: release.body.unwrap_or_default(),
        update_available,
    })
}

/// Résout le chemin cible `.exe` propre même si le processus actuel avait déjà été renommé en `.exe.old`.
fn resolve_target_exe(current_exe: &std::path::Path) -> PathBuf {
    let s = current_exe.to_string_lossy();
    if let Some(stripped) = s.strip_suffix(".old") {
        PathBuf::from(stripped)
    } else {
        current_exe.to_path_buf()
    }
}

/// Télécharge le nouveau `.exe` depuis GitHub Releases, vérifie l'en-tête PE (`MZ`),
/// remplace l'exécutable et relance automatiquement la nouvelle version.
pub fn perform_self_update(runner: &Runner, info: &UpdateInfo) -> bool {
    if info.download_url.is_empty() {
        runner
            .logger
            .warn("Aucun binaire .exe attaché à cette release GitHub.");
        return false;
    }

    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Téléchargement et mise à jour auto simulés depuis : {}",
            info.download_url
        ));
        return true;
    }

    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            runner
                .logger
                .error(format!("Impossible de localiser l'exécutable actuel : {e}"));
            return false;
        }
    };

    let target_exe = resolve_target_exe(&current_exe);
    let pid = std::process::id();
    let new_exe = target_exe.with_extension(format!("exe.{pid}.new"));
    let old_exe = target_exe.with_extension(format!("exe.{pid}.old"));

    runner.logger.info(format!(
        "Téléchargement de WinCare v{}...",
        info.latest_version
    ));

    let new_exe_str = new_exe.to_string_lossy().to_string();
    let dl = runner.run_query(
        "curl.exe",
        &[
            "-fL",
            "--max-time",
            "60",
            "-H",
            "User-Agent: WinCare-Updater",
            "-o",
            &new_exe_str,
            &info.download_url,
        ],
    );

    if !dl.success() || !new_exe.exists() {
        runner
            .logger
            .error("Échec du téléchargement de la mise à jour.");
        let _ = fs::remove_file(&new_exe);
        return false;
    }

    // Vérification de sécurité : taille > 500 Ko et signature Windows PE "MZ"
    let mut header = [0u8; 2];
    let valid_pe = fs::File::open(&new_exe)
        .and_then(|mut f| {
            let len = f.metadata()?.len();
            f.read_exact(&mut header)?;
            Ok(len > 500_000 && header == [0x4D, 0x5A])
        })
        .unwrap_or(false);

    if !valid_pe {
        runner
            .logger
            .error("Le fichier téléchargé n'est pas un exécutable Windows valide.");
        let _ = fs::remove_file(&new_exe);
        return false;
    }

    let _ = fs::remove_file(&old_exe);

    // Tentative de remplacement direct par renommage atomique avec suffixe PID unique
    let direct_swap_ok = if current_exe == target_exe {
        fs::rename(&current_exe, &old_exe).is_ok() && fs::rename(&new_exe, &target_exe).is_ok()
    } else {
        // Le processus courant tourne déjà depuis un `.old` : on peut remplacer `target_exe` directement !
        let _ = fs::remove_file(&target_exe);
        fs::rename(&new_exe, &target_exe).is_ok()
    };

    let target_str = target_exe.to_string_lossy().to_string();
    if direct_swap_ok {
        runner.logger.success(format!(
            "Mise à jour v{} installée ! Redémarrage automatique de WinCare...",
            info.latest_version
        ));
        let _ = Command::new(&target_exe)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        std::process::exit(0);
    }

    // Fallback universel si le fichier cible est verrouillé : script détaché qui attend la fermeture (1s),
    // déplace `new_exe` vers `target_exe` et relance `target_exe`.
    runner.logger.success(format!(
        "Application de la mise à jour v{} et redémarrage automatique...",
        info.latest_version
    ));
    let cmd_line = format!(
        "ping 127.0.0.1 -n 2 >nul & move /Y \"{new_exe_str}\" \"{target_str}\" >nul 2>&1 & start \"\" \"{target_str}\""
    );
    let _ = Command::new("cmd.exe")
        .args(["/C", &cmd_line])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("v0.2.0"), (0, 2, 0));
        assert_eq!(parse_semver("0.10.3"), (0, 10, 3));
        assert!(parse_semver("0.2.0") > parse_semver("0.1.9"));
    }
}
