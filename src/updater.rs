use crate::runner::Runner;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;

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

/// Supprime au démarrage l'ancien binaire `.old` laissé par une mise à jour précédente.
pub fn cleanup_old_binary() {
    if let Ok(exe) = std::env::current_exe() {
        let old_path = exe.with_extension("exe.old");
        if old_path.exists() {
            let _ = fs::remove_file(old_path);
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

/// Télécharge le nouveau `.exe` depuis GitHub Releases, vérifie l'en-tête PE (`MZ`) et remplace l'exécutable courant.
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

    let new_exe = current_exe.with_extension("exe.new");
    let old_exe = current_exe.with_extension("exe.old");

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
    if let Err(e) = fs::rename(&current_exe, &old_exe) {
        runner
            .logger
            .error(format!("Impossible de renommer l'exécutable en cours : {e}"));
        let _ = fs::remove_file(&new_exe);
        return false;
    }

    if let Err(e) = fs::rename(&new_exe, &current_exe) {
        let _ = fs::rename(&old_exe, &current_exe);
        runner
            .logger
            .error(format!("Impossible d'installer le nouvel exécutable : {e}"));
        return false;
    }

    runner.logger.success(format!(
        "Mise à jour vers v{} installée avec succès ! Relancez WinCare pour utiliser la nouvelle version.",
        info.latest_version
    ));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("v0.2.0"), (0, 2, 0));
        assert_eq!(parse_semver("1.10.3"), (1, 10, 3));
        assert!(parse_semver("v0.3.0") > parse_semver("0.2.0"));
    }
}
