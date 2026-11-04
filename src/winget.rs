use crate::runner::Runner;
use crate::types::{WingetPkgState, WingetUpgradeItem, WorkerEvent};

/// Nettoie les séquences d'échappement ANSI pouvant être émises dans la console.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next_c in chars.by_ref() {
                    if next_c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Analyse le tableau texte brut renvoyé par la commande `winget upgrade`.
/// Identifie dynamiquement les offsets des colonnes (Nom, ID, Version, Disponible, Source)
/// à partir de la ligne d'en-tête et de la ligne de tirets de séparation.
pub fn parse_winget_table(output: &str) -> Vec<WingetUpgradeItem> {
    let cleaned = strip_ansi(output);
    let lines: Vec<&str> = cleaned.lines().map(|l| l.trim_end()).collect();

    // Recherche de la ligne de tirets séparatrice (ex: "-------------------")
    let sep_idx = match lines.iter().position(|l| {
        let trimmed = l.trim();
        trimmed.starts_with("---") && trimmed.replace(' ', "").len() >= 10
    }) {
        Some(idx) => idx,
        None => return Vec::new(),
    };

    // La ligne d'en-tête précède la ligne de séparation
    let header_line = if sep_idx > 0 {
        lines[sep_idx - 1]
    } else {
        ""
    };

    // Détermination des offsets de début de chaque colonne (basé sur l'en-tête ou les tirets)
    let header_chars: Vec<char> = header_line.chars().collect();
    let mut col_starts = Vec::new();
    let mut space_run = 0;

    for (i, &c) in header_chars.iter().enumerate() {
        if c == ' ' || c == '\t' {
            space_run += 1;
        } else {
            if i == 0 || space_run >= 2 {
                col_starts.push(i);
            }
            space_run = 0;
        }
    }

    // Repli : si l'en-tête n'a pas pu être découpé en au moins 4 colonnes,
    // on analyse les blocs de tirets de la ligne séparatrice
    if col_starts.len() < 4 {
        col_starts.clear();
        let sep_chars: Vec<char> = lines[sep_idx].chars().collect();
        let mut sep_space_run = 0;
        for (i, &c) in sep_chars.iter().enumerate() {
            if c == '-' {
                if i == 0 || sep_space_run >= 1 {
                    col_starts.push(i);
                }
                sep_space_run = 0;
            } else if c == ' ' || c == '\t' {
                sep_space_run += 1;
            }
        }
    }

    if col_starts.len() < 4 {
        return Vec::new();
    }

    let c0 = col_starts[0];
    let c1 = col_starts[1];
    let c2 = col_starts[2];
    let c3 = col_starts[3];
    let c4 = if col_starts.len() > 4 {
        Some(col_starts[4])
    } else {
        None
    };

    let mut items = Vec::new();

    for raw_line in &lines[sep_idx + 1..] {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with("---") {
            continue;
        }

        // Ignorer les caractères isolés d'animation console (spinners)
        if trimmed.len() == 1 && matches!(trimmed, "-" | "\\" | "/" | "|") {
            continue;
        }

        // Ignorer les lignes informatives de pied de page
        let lower = trimmed.to_lowercase();
        if lower.contains("mises à niveau disponibles")
            || lower.contains("upgrades available")
            || lower.contains("upgrade available")
            || lower.contains("package(s)")
            || lower.contains("paquet(s)")
            || lower.contains("numéros de version")
            || lower.contains("version numbers")
            || lower.contains("include-unknown")
        {
            continue;
        }

        let row_chars: Vec<char> = raw_line.chars().collect();
        if row_chars.len() <= c1 {
            continue;
        }

        let slice_cols = |start: usize, end: usize| -> String {
            if start >= row_chars.len() {
                return String::new();
            }
            let actual_end = end.min(row_chars.len());
            if actual_end > start {
                row_chars[start..actual_end]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string()
            } else {
                String::new()
            }
        };

        let name = slice_cols(c0, c1);
        let id = slice_cols(c1, c2);
        let current_version = slice_cols(c2, c3);
        let available_version = match c4 {
            Some(end) => slice_cols(c3, end),
            None => slice_cols(c3, row_chars.len()),
        };

        // Valider l'entrée : le nom et l'ID doivent être non vides,
        // et un ID de paquet Winget valide ne contient pas d'espaces.
        if !name.is_empty() && !id.is_empty() && !id.contains(' ') {
            items.push(WingetUpgradeItem {
                name,
                id,
                current_version,
                available_version,
                state: WingetPkgState::Available,
            });
        }
    }

    items
}

/// Scanne l'ensemble des logiciels installés pouvant être mis à jour via le gestionnaire de paquets Winget.
pub fn scan_winget_upgrades(runner: &Runner) -> Vec<WingetUpgradeItem> {
    runner
        .logger
        .section("SCAN DES MISES À JOUR LOGICIELS (WINGET)");

    // Exécution directe de winget ou via PowerShell avec encodage UTF-8 forcé
    let out = runner.run_query("winget", &["upgrade", "--accept-source-agreements"]);
    let raw = if out.stdout.trim().is_empty() {
        runner
            .run_ps_query("winget upgrade --accept-source-agreements")
            .stdout
    } else {
        out.stdout
    };

    if raw.trim().is_empty() {
        runner.logger.warn(
            "Winget n'a retourné aucune information (winget indisponible ou aucun paquet à mettre à jour).",
        );
        return Vec::new();
    }

    let items = parse_winget_table(&raw);
    runner
        .logger
        .info(format!("{} mise(s) à jour détectée(s) via Winget", items.len()));
    items
}

/// Filtre et logge les lignes utiles de la sortie winget en ignorant les animations
/// de spinners (- \ | /), les barres de progression ASCII et les pourcentages récurrents.
fn log_filtered_winget_output(logger: &crate::logger::Logger, stdout: &str) {
    for raw_line in stdout.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Ignorer les spinners isolés ou combinaisons d'espaces/spinners
        if trimmed.len() <= 2 && trimmed.chars().all(|c| matches!(c, '-' | '\\' | '/' | '|' | ' ' | '\t')) {
            continue;
        }
        // Ignorer les barres de progression de téléchargement WinGet (blocs ASCII ou pourcentages purs)
        if trimmed.contains('█') || trimmed.contains('░') || trimmed.contains('▒') {
            continue;
        }
        if trimmed.ends_with('%')
            && trimmed
                .chars()
                .take(trimmed.len() - 1)
                .all(|c| c.is_ascii_digit() || c.is_whitespace() || matches!(c, '-' | '\\' | '/' | '|'))
        {
            continue;
        }
        // Ignorer les lignes de tirets ou séparateurs purs
        if trimmed.chars().all(|c| c == '-' || c == '=') && trimmed.len() >= 3 {
            continue;
        }
        logger.info(trimmed);
    }
}

/// Extrait un message court et compréhensible en cas d'échec de winget.
fn extract_winget_error(exit_code: i32, stdout: &str, stderr: &str) -> String {
    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    if combined.contains("en cours d'exécution") || combined.contains("is running") || combined.contains("in use") {
        "Application en cours d'exécution".to_string()
    } else if combined.contains("aucune mise à niveau applicable")
        || combined.contains("no applicable update")
        || combined.contains("déjà à jour")
        || combined.contains("already installed")
    {
        "Déjà à jour ou introuvable".to_string()
    } else if combined.contains("administrateur")
        || combined.contains("administrator")
        || combined.contains("elevation")
        || combined.contains("privilege")
    {
        "Droits administrateur requis".to_string()
    } else if combined.contains("hachage") || combined.contains("hash") {
        "Échec de vérification du hachage".to_string()
    } else if combined.contains("téléchargement") || combined.contains("download") {
        "Échec du téléchargement".to_string()
    } else if exit_code != 0 {
        format!("Code d'erreur : {exit_code}")
    } else {
        "Échec de mise à jour".to_string()
    }
}

/// Met à jour individuellement une sélection de paquets Winget par leurs identifiants.
/// Respecte le mode dry_run du Runner et notifie l'UI en temps réel du statut de chaque paquet.
/// Retourne le nombre de paquets mis à jour avec succès.
pub fn upgrade_winget_packages(runner: &Runner, ids: &[String]) -> usize {
    runner
        .logger
        .section(format!("MISE À JOUR DE {} PAQUET(S) WINGET", ids.len()));

    // Marque d'abord tous les paquets demandés en file d'attente
    for id in ids {
        runner.logger.send_event(WorkerEvent::WingetItemStatus {
            id: id.clone(),
            state: WingetPkgState::Queued,
        });
    }

    let mut success_count = 0;

    for (idx, id) in ids.iter().enumerate() {
        if runner.is_cancelled() {
            runner
                .logger
                .warn("⏹ Mise à jour des paquets WinGet interrompue par l'utilisateur.");
            for rem_id in &ids[idx..] {
                runner.logger.send_event(WorkerEvent::WingetItemStatus {
                    id: rem_id.clone(),
                    state: WingetPkgState::Cancelled,
                });
            }
            break;
        }

        runner.logger.send_event(WorkerEvent::WingetItemStatus {
            id: id.clone(),
            state: WingetPkgState::Upgrading("Téléchargement & installation en cours...".into()),
        });

        runner.logger.progress(
            idx + 1,
            ids.len(),
            format!("[{}/{}] WinGet : installation de {}...", idx + 1, ids.len(), id),
        );
        runner
            .logger
            .info(format!("[{}/{}] Lancement de la mise à jour pour {}", idx + 1, ids.len(), id));

        // stream_to_log = false pour éviter le flood de spinners et barres ASCII
        let out = runner.run_action(
            "winget",
            &[
                "upgrade",
                "--id",
                id.as_str(),
                "--exact",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
            ],
            false,
        );

        log_filtered_winget_output(&runner.logger, &out.stdout);
        if !out.stderr.trim().is_empty() {
            for err_line in out.stderr.lines() {
                let t = err_line.trim();
                if !t.is_empty() {
                    runner.logger.warn(t);
                }
            }
        }

        if runner.is_cancelled() {
            runner
                .logger
                .warn("⏹ Mise à jour interrompue pendant l'opération.");
            for rem_id in &ids[idx..] {
                runner.logger.send_event(WorkerEvent::WingetItemStatus {
                    id: rem_id.clone(),
                    state: WingetPkgState::Cancelled,
                });
            }
            break;
        }

        if out.success() || out.exit_code == 3010 {
            success_count += 1;
            runner.logger.send_event(WorkerEvent::WingetItemStatus {
                id: id.clone(),
                state: WingetPkgState::Updated,
            });
            runner.logger.success(format!(
                "✔ [{}/{}] {} mis à jour avec succès.",
                idx + 1,
                ids.len(),
                id
            ));
        } else {
            let err_msg = extract_winget_error(out.exit_code, &out.stdout, &out.stderr);
            runner.logger.warn(format!(
                "⚠ [{}/{}] Échec pour {} ({})",
                idx + 1,
                ids.len(),
                id,
                err_msg
            ));
            runner.logger.send_event(WorkerEvent::WingetItemStatus {
                id: id.clone(),
                state: WingetPkgState::Failed(err_msg),
            });
        }
    }

    runner.logger.info(format!(
        "Mises à jour terminées : {}/{} paquets mis à jour avec succès.",
        success_count,
        ids.len()
    ));
    success_count
}

/// Lance la mise à jour globale de tous les paquets obsolètes (`winget upgrade --all`).
/// Respecte le mode dry_run du Runner.
pub fn upgrade_all_winget(runner: &Runner) -> bool {
    runner
        .logger
        .section("MISE À JOUR GLOBALE WINGET (--all)");
    let out = runner.run_action(
        "winget",
        &[
            "upgrade",
            "--all",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ],
        true,
    );
    if out.success() {
        runner
            .logger
            .success("Mise à jour globale Winget terminée avec succès.");
        true
    } else {
        runner.logger.warn(format!(
            "Mise à jour globale Winget terminée avec le code de sortie {}",
            out.exit_code
        ));
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_winget_table_french() {
        let sample = r#"
Nom                                                    ID                                     Version                       Disponible                    Source
-----------------------------------------------------------------------------------------------------------------------------------------------------------------
Affinity                                               Canva.Affinity                         3.2.2.4557                    3.3.3.5004                    winget
Apple Mobile Device Support                            Apple.AppleMobileDeviceSupport         19.0.1.27                     19.4.0.10                     winget
Canva                                                  XP8K17RNMM8MTN                         1.126.0                       1.128.0                       msstore
NVIDIA Nsight Compute 2025.2.1                         Nvidia.Nsight.Compute                  < 2026.3.1                    2026.3.1                      winget
58 mises à niveau disponibles.
3 package(s) a(ont) des numéros de version qui ne peuvent pas être déterminés. Utilisez --include-unknown pour afficher tous les résultats.
"#;
        let items = parse_winget_table(sample);
        assert_eq!(items.len(), 4);

        assert_eq!(items[0].name, "Affinity");
        assert_eq!(items[0].id, "Canva.Affinity");
        assert_eq!(items[0].current_version, "3.2.2.4557");
        assert_eq!(items[0].available_version, "3.3.3.5004");
        assert_eq!(items[0].state, WingetPkgState::Available);

        assert_eq!(items[1].name, "Apple Mobile Device Support");
        assert_eq!(items[1].id, "Apple.AppleMobileDeviceSupport");
        assert_eq!(items[1].current_version, "19.0.1.27");
        assert_eq!(items[1].available_version, "19.4.0.10");

        assert_eq!(items[2].name, "Canva");
        assert_eq!(items[2].id, "XP8K17RNMM8MTN");
        assert_eq!(items[2].current_version, "1.126.0");
        assert_eq!(items[2].available_version, "1.128.0");

        assert_eq!(items[3].name, "NVIDIA Nsight Compute 2025.2.1");
        assert_eq!(items[3].id, "Nvidia.Nsight.Compute");
        assert_eq!(items[3].current_version, "< 2026.3.1");
        assert_eq!(items[3].available_version, "2026.3.1");
    }

    #[test]
    fn test_parse_winget_table_english() {
        let sample = r#"
Name                                                   Id                                     Version                       Available                     Source
-----------------------------------------------------------------------------------------------------------------------------------------------------------------
Visual Studio Community 2026                           Microsoft.VisualStudio.Community       18.3.0                        18.11.3                       winget
yt-dlp                                                 yt-dlp.yt-dlp                          2026.03.17                    2026.10.26                    winget
2 upgrades available.
"#;
        let items = parse_winget_table(sample);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "Visual Studio Community 2026");
        assert_eq!(items[0].id, "Microsoft.VisualStudio.Community");
        assert_eq!(items[0].current_version, "18.3.0");
        assert_eq!(items[0].available_version, "18.11.3");

        assert_eq!(items[1].name, "yt-dlp");
        assert_eq!(items[1].id, "yt-dlp.yt-dlp");
        assert_eq!(items[1].current_version, "2026.03.17");
        assert_eq!(items[1].available_version, "2026.10.26");
    }

    #[test]
    fn test_extract_winget_error() {
        assert_eq!(extract_winget_error(0, "App is running right now", ""), "Application en cours d'exécution");
        assert_eq!(extract_winget_error(-1978236885, "No applicable update found", ""), "Déjà à jour ou introuvable");
        assert_eq!(extract_winget_error(1618, "", ""), "Code d'erreur : 1618");
    }

    #[test]
    fn test_parse_winget_table_empty_or_no_upgrades() {
        let sample = "Aucune mise à niveau disponible trouvée.";
        let items = parse_winget_table(sample);
        assert!(items.is_empty());
    }
}
