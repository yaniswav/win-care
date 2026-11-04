use std::path::{Path, PathBuf};
use crate::runner::Runner;
use crate::types::WinCareProfile;

pub fn default_profile_path() -> PathBuf {
    let logs_dir = crate::logger::resolve_logs_dir();
    if let Some(parent) = logs_dir.parent() {
        return parent.join("wincare-profile.json");
    }
    logs_dir.join("wincare-profile.json")
}

pub fn export_profile(runner: &Runner, profile: &WinCareProfile) -> Option<PathBuf> {
    let path = default_profile_path();
    let json = match serde_json::to_string_pretty(profile) {
        Ok(content) => content,
        Err(e) => {
            runner
                .logger
                .error(format!("Erreur lors de la sérialisation du profil : {e}"));
            return None;
        }
    };

    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Export profil simulé vers {}",
            path.display()
        ));
        return Some(path);
    }

    match std::fs::write(&path, json) {
        Ok(_) => {
            runner
                .logger
                .success(format!("Profil exporté avec succès vers {}", path.display()));
            Some(path)
        }
        Err(e) => {
            runner.logger.error(format!(
                "Impossible d'écrire le profil vers {} : {e}",
                path.display()
            ));
            None
        }
    }
}

pub fn import_profile(runner: &Runner, path: &Path) -> Option<WinCareProfile> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            runner.logger.warn(format!(
                "Impossible de lire le fichier de profil {} : {e}",
                path.display()
            ));
            return None;
        }
    };

    match serde_json::from_str::<WinCareProfile>(&content) {
        Ok(profile) => {
            runner.logger.success(format!(
                "Profil importé avec succès depuis {}",
                path.display()
            ));
            Some(profile)
        }
        Err(e) => {
            runner.logger.warn(format!(
                "Erreur de désérialisation du profil {} : {e}",
                path.display()
            ));
            None
        }
    }
}
