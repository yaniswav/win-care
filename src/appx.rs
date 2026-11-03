use crate::runner::Runner;
use crate::types::AppxBloatItem;

/// Définition statique d'un bloatware ou application préinstallée Windows candidate au nettoyage.
pub struct AppxBloatDef {
    pub id: &'static str,
    pub display_name: &'static str,
    pub package_pattern: &'static str,
    pub description_en: &'static str,
    pub description_fr: &'static str,
}

/// Liste de référence des 14 bloatwares Windows 10/11 les plus courants et superflus.
pub const KNOWN_APPX_BLOATWARE: &[AppxBloatDef] = &[
    AppxBloatDef {
        id: "clipchamp",
        display_name: "Clipchamp Video Editor",
        package_pattern: "*Clipchamp.Clipchamp*",
        description_en: "Pre-installed basic video editor with subscription promotions.",
        description_fr: "Éditeur vidéo intégré par Microsoft avec sollicitations d'abonnement.",
    },
    AppxBloatDef {
        id: "cortana",
        display_name: "Cortana",
        package_pattern: "*Microsoft.549981C3F5F10*",
        description_en: "Deprecated Microsoft voice assistant.",
        description_fr: "Ancien assistant vocal Microsoft, obsolète et inactif.",
    },
    AppxBloatDef {
        id: "bing_news",
        display_name: "Microsoft News / Bing News",
        package_pattern: "*Microsoft.BingNews*",
        description_en: "News and MSN feed reader application.",
        description_fr: "Application d'actualités et agrégateur de flux MSN / Bing.",
    },
    AppxBloatDef {
        id: "bing_weather",
        display_name: "MSN Weather / Bing Météo",
        package_pattern: "*Microsoft.BingWeather*",
        description_en: "Weather forecast and radar application.",
        description_fr: "Application de prévisions météorologiques Bing / MSN.",
    },
    AppxBloatDef {
        id: "solitaire",
        display_name: "Microsoft Solitaire Collection",
        package_pattern: "*Microsoft.MicrosoftSolitaireCollection*",
        description_en: "Pre-installed casual card games bundle containing advertisements.",
        description_fr: "Collection de jeux de cartes préinstallée avec bannières publicitaires.",
    },
    AppxBloatDef {
        id: "sticky_notes",
        display_name: "Microsoft Sticky Notes",
        package_pattern: "*Microsoft.MicrosoftStickyNotes*",
        description_en: "Desktop virtual sticky notes utility.",
        description_fr: "Pense-bêtes virtuels pour le bureau Windows.",
    },
    AppxBloatDef {
        id: "feedback_hub",
        display_name: "Windows Feedback Hub",
        package_pattern: "*Microsoft.WindowsFeedbackHub*",
        description_en: "Telemetry and feedback reporting application.",
        description_fr: "Application d'envoi de commentaires et diagnostics à Microsoft.",
    },
    AppxBloatDef {
        id: "get_help",
        display_name: "Microsoft Get Help",
        package_pattern: "*Microsoft.GetHelp*",
        description_en: "Automated support and online troubleshooting assistant.",
        description_fr: "Application d'assistance et de support en ligne Microsoft.",
    },
    AppxBloatDef {
        id: "get_started",
        display_name: "Windows Tips / Get Started",
        package_pattern: "*Microsoft.Getstarted*",
        description_en: "Onboarding tips and feature discovery recommendations.",
        description_fr: "Astuces de bienvenue, conseils de découverte et suggestions du système.",
    },
    AppxBloatDef {
        id: "maps",
        display_name: "Windows Maps / Cartes",
        package_pattern: "*Microsoft.WindowsMaps*",
        description_en: "Built-in offline and online mapping utility.",
        description_fr: "Application de cartographie et de navigation intégrée.",
    },
    AppxBloatDef {
        id: "people",
        display_name: "Microsoft People / Contacts",
        package_pattern: "*Microsoft.People*",
        description_en: "Address book and contact management application.",
        description_fr: "Gestionnaire de contacts et carnet d'adresses intégré.",
    },
    AppxBloatDef {
        id: "mixed_reality",
        display_name: "Mixed Reality Portal",
        package_pattern: "*Microsoft.MixedReality.Portal*",
        description_en: "Management portal for Windows Mixed Reality headsets.",
        description_fr: "Portail de configuration pour casques de réalité mixte VR/AR.",
    },
    AppxBloatDef {
        id: "zune_video",
        display_name: "Movies & TV / Films et TV",
        package_pattern: "*Microsoft.ZuneVideo*",
        description_en: "Default player and Microsoft Store video rental client.",
        description_fr: "Lecteur vidéo par défaut et boutique de location de films Microsoft.",
    },
    AppxBloatDef {
        id: "teams_personal",
        display_name: "Microsoft Teams (Personal/Chat)",
        package_pattern: "*MicrosoftTeams*",
        description_en: "Consumer version of Microsoft Teams chat embedded in taskbar.",
        description_fr: "Version personnelle de Microsoft Teams intégrée à la barre des tâches.",
    },
];

/// Instancie la liste des bloatwares connus avec `installed: false`.
pub fn get_default_bloatware_list() -> Vec<AppxBloatItem> {
    KNOWN_APPX_BLOATWARE
        .iter()
        .map(|def| AppxBloatItem {
            id: def.id.to_string(),
            display_name: def.display_name.to_string(),
            package_pattern: def.package_pattern.to_string(),
            description_en: def.description_en.to_string(),
            description_fr: def.description_fr.to_string(),
            installed: false,
        })
        .collect()
}

/// Scanne le système via une requête PowerShell rapide pour détecter la présence des bloatwares AppX.
pub fn scan_appx_bloatware(runner: &Runner) -> Vec<AppxBloatItem> {
    runner
        .logger
        .section("SCAN DES BLOATWARES APPS WINDOWS (APPX)");

    let ps_out = runner.run_ps_query("Get-AppxPackage | Select-Object -ExpandProperty Name");
    let installed_names: Vec<String> = ps_out
        .stdout
        .lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty())
        .collect();

    let mut items = get_default_bloatware_list();
    for item in &mut items {
        let pattern_clean = item.package_pattern.replace('*', "").to_lowercase();
        item.installed = installed_names.iter().any(|pkg_name| {
            pkg_name.contains(&pattern_clean)
                || (pattern_clean == "microsoftteams" && pkg_name.contains("msteams"))
        });
    }

    let installed_count = items.iter().filter(|i| i.installed).count();
    runner.logger.info(format!(
        "{} application(s) préinstallée(s) superflue(s) détectée(s) sur {}",
        installed_count,
        items.len()
    ));

    items
}

/// Supprime une liste de paquets AppX et leurs homologues provisionnés (pour tous les nouveaux utilisateurs).
/// Si le Runner est en mode `dry_run`, l'action est seulement simulée et loggée.
/// Retourne le nombre d'applications traitées.
pub fn remove_appx_packages(runner: &Runner, patterns: &[String]) -> usize {
    runner
        .logger
        .section(format!("SUPPRESSION DE {} PAQUET(S) APPX", patterns.len()));
    let mut removed_count = 0;

    for (idx, pattern) in patterns.iter().enumerate() {
        runner
            .logger
            .info(format!("[{}/{}] Suppression AppX : {}", idx + 1, patterns.len(), pattern));

        if runner.dry_run {
            runner
                .logger
                .info(format!("[DRY-RUN] Suppression AppX simulée : {pattern}"));
            removed_count += 1;
        } else {
            let script = format!(
                "Get-AppxPackage '{pattern}' | Remove-AppxPackage -ErrorAction SilentlyContinue; \
                 Get-AppxProvisionedPackage -Online | Where-Object DisplayName -like '{pattern}' | Remove-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue; \
                 if ('{pattern}' -like '*MicrosoftTeams*') {{ \
                     Get-AppxPackage '*MSTeams*' | Remove-AppxPackage -ErrorAction SilentlyContinue; \
                     Get-AppxProvisionedPackage -Online | Where-Object DisplayName -like '*MSTeams*' | Remove-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue; \
                 }}"
            );
            let out = runner.run_ps_action(
                &format!("Suppression AppX {pattern}"),
                &script,
                true,
            );
            if out.success() {
                runner
                    .logger
                    .success(format!("Paquet supprimé avec succès : {pattern}"));
                removed_count += 1;
            } else {
                runner.logger.warn(format!(
                    "Avertissement ou code non nul lors de la suppression de {pattern} (code {})",
                    out.exit_code
                ));
            }
        }
    }

    runner.logger.info(format!(
        "Désinstallation AppX terminée : {}/{} paquet(s) traité(s).",
        removed_count,
        patterns.len()
    ));
    removed_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_bloatware_list_completeness() {
        let list = get_default_bloatware_list();
        assert_eq!(list.len(), 14);

        let expected_ids = [
            "clipchamp",
            "cortana",
            "bing_news",
            "bing_weather",
            "solitaire",
            "sticky_notes",
            "feedback_hub",
            "get_help",
            "get_started",
            "maps",
            "people",
            "mixed_reality",
            "zune_video",
            "teams_personal",
        ];

        for id in expected_ids {
            let found = list.iter().find(|i| i.id == id);
            assert!(found.is_some(), "Manque l'item attendu : {id}");
            let item = found.unwrap();
            assert!(!item.display_name.is_empty());
            assert!(item.package_pattern.starts_with('*'));
            assert!(item.package_pattern.ends_with('*'));
            assert!(!item.description_en.is_empty());
            assert!(!item.description_fr.is_empty());
            assert!(!item.installed);
        }
    }

    #[test]
    fn test_pattern_matching_simulation() {
        let fake_installed = vec![
            "clipchamp.clipchamp".to_string(),
            "microsoft.bingnews".to_string(),
            "microsoft.microsoftstickynotes".to_string(),
            "msteams".to_string(),
        ];

        let mut items = get_default_bloatware_list();
        for item in &mut items {
            let pattern_clean = item.package_pattern.replace('*', "").to_lowercase();
            item.installed = fake_installed.iter().any(|pkg_name| {
                pkg_name.contains(&pattern_clean)
                    || (pattern_clean == "microsoftteams" && pkg_name.contains("msteams"))
            });
        }

        let clipchamp = items.iter().find(|i| i.id == "clipchamp").unwrap();
        assert!(clipchamp.installed);

        let news = items.iter().find(|i| i.id == "bing_news").unwrap();
        assert!(news.installed);

        let sticky = items.iter().find(|i| i.id == "sticky_notes").unwrap();
        assert!(sticky.installed);

        let teams = items.iter().find(|i| i.id == "teams_personal").unwrap();
        assert!(teams.installed);

        let cortana = items.iter().find(|i| i.id == "cortana").unwrap();
        assert!(!cortana.installed);

        let solitaire = items.iter().find(|i| i.id == "solitaire").unwrap();
        assert!(!solitaire.installed);
    }
}
