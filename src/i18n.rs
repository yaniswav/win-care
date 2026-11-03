use crate::types::{DebloatCategory, RepairTask};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// Détecte la langue active selon la priorité :
    /// 1. Arguments en ligne de commande (--lang fr / --lang=fr / --lang en / --lang=en)
    /// 2. Variable d'environnement WINCARE_LANG (ex: "fr", "fr-FR", "en")
    /// 3. Registre Windows : HKCU\Control Panel\International\LocaleName
    pub fn detect() -> Self {
        // 1. Vérification des arguments CLI
        let args: Vec<String> = std::env::args().collect();
        for (i, arg) in args.iter().enumerate() {
            let lower = arg.to_lowercase();
            if lower == "--lang=fr" {
                return Lang::Fr;
            } else if lower == "--lang=en" {
                return Lang::En;
            } else if lower == "--lang" {
                if let Some(next) = args.get(i + 1) {
                    let next_lower = next.to_lowercase();
                    if next_lower == "fr" {
                        return Lang::Fr;
                    } else if next_lower == "en" {
                        return Lang::En;
                    }
                }
            }
        }

        // 2. Vérification de la variable d'environnement WINCARE_LANG
        if let Ok(env_lang) = std::env::var("WINCARE_LANG") {
            let env_lower = env_lang.trim().to_lowercase();
            if env_lower.starts_with("fr") {
                return Lang::Fr;
            } else if env_lower.starts_with("en") {
                return Lang::En;
            }
        }

        // 3. Lecture du Registre Windows HKCU\Control Panel\International\LocaleName
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey(r"Control Panel\International") {
            if let Ok(locale) = key.get_value::<String, _>("LocaleName") {
                if locale.trim().to_lowercase().starts_with("fr") {
                    return Lang::Fr;
                } else {
                    return Lang::En;
                }
            }
        }

        Lang::default()
    }

    /// Retourne la chaîne correspondant à la langue sélectionnée.
    #[inline]
    pub fn tr<'a>(self, en: &'a str, fr: &'a str) -> &'a str {
        match self {
            Lang::En => en,
            Lang::Fr => fr,
        }
    }

    /// Code ISO court en majuscule ("EN" ou "FR").
    #[inline]
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Fr => "FR",
        }
    }

    /// Bascule entre anglais et français.
    #[inline]
    pub fn toggle(self) -> Self {
        match self {
            Lang::En => Lang::Fr,
            Lang::Fr => Lang::En,
        }
    }
}

/// Titre traduit pour chaque catégorie de debloat.
pub fn debloat_category_title(lang: Lang, cat: DebloatCategory) -> &'static str {
    match cat {
        DebloatCategory::AiAndCopilot => {
            lang.tr("🤖 AI, Copilot & Recall", "🤖 IA, Copilot & Recall")
        }
        DebloatCategory::PrivacyAndTelemetry => {
            lang.tr("🔒 Privacy, Ads & Bing", "🔒 Confidentialité, Pubs & Bing")
        }
        DebloatCategory::ExplorerAndUi => {
            lang.tr("📁 Explorer & Win11 UI", "📁 Explorateur & Interface Win11")
        }
        DebloatCategory::SystemAndGaming => lang.tr(
            "⚙ System, Mouse & Taskbar",
            "⚙ Système, Souris & Barre des tâches",
        ),
    }
}

/// Textes complets (titre, description, note de sécurité) pour chaque tweak debloat.
pub fn debloat_tweak_text(
    lang: Lang,
    id: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    match id {
        "disable_copilot" => Some(match lang {
            Lang::En => (
                "Disable Windows Copilot",
                "Hides the Copilot taskbar button and disables the Copilot service (HKCU & HKLM).",
                "Completely safe. Can be re-enabled anytime without side effects.",
            ),
            Lang::Fr => (
                "Désactiver Windows Copilot",
                "Masque le bouton Copilot de la barre des tâches et désactive le service Copilot (HKCU & HKLM).",
                "Totalement sûr. Réversible à tout moment sans effet indésirable.",
            ),
        }),
        "disable_recall" | "disable_ai_recall" => Some(match lang {
            Lang::En => (
                "Disable Windows AI Recall",
                "Blocks background AI data analysis, snapshot capturing, and Recall enablement.",
                "Safe. Protects privacy by preventing automated screen snapshot archiving.",
            ),
            Lang::Fr => (
                "Désactiver Windows AI Recall",
                "Bloque l'analyse des données IA en arrière-plan, la capture d'instantanés et l'activation de Recall.",
                "Sûr. Protège la vie privée en empêchant l'archivage automatique d'écrans.",
            ),
        }),
        "disable_click_to_do" => Some(match lang {
            Lang::En => (
                "Disable Click to Do AI",
                "Disables the Windows 11 AI-assisted interactive contextual action overlay.",
                "Safe. Reduces background AI service processing overhead.",
            ),
            Lang::Fr => (
                "Désactiver Click to Do IA",
                "Désactive la surcouche d'actions contextuelles interactives assistée par IA de Windows 11.",
                "Sûr. Réduit la charge des services IA d'arrière-plan.",
            ),
        }),
        "disable_edge_notepad_paint_ai" => Some(match lang {
            Lang::En => (
                "Disable AI in Edge, Notepad & Paint",
                "Disables Copilot sidebar in Edge, AI text rewriting in Notepad, and Cocreator in Paint.",
                "Safe. Preserves standard offline notepad and image editing features.",
            ),
            Lang::Fr => (
                "Désactiver l'IA dans Edge, Bloc-notes et Paint",
                "Désactive la barre latérale Copilot dans Edge, la génération IA dans le Bloc-notes et Cocreator dans Paint.",
                "Sûr. Conserve les fonctions classiques de retouche et de prise de note.",
            ),
        }),
        "disable_telemetry" => Some(match lang {
            Lang::En => (
                "Disable Windows Telemetry",
                "Disables diagnostic data collection, advertising ID, TIPC, and customer feedback.",
                "Safe. Windows Update and core services remain fully operational.",
            ),
            Lang::Fr => (
                "Désactiver la Télémétrie Windows",
                "Désactive la collecte de télémétrie diagnostique, l'identifiant publicitaire, TIPC et l'historique d'activités.",
                "Sûr. Windows Update et les composants essentiels continuent de fonctionner normalement.",
            ),
        }),
        "disable_bing_search" => Some(match lang {
            Lang::En => (
                "Disable Bing & Cortana in Windows Search",
                "Removes web search suggestions and Cortana integration from Start Menu and Taskbar search.",
                "Safe. Search becomes local, faster, and respects your privacy.",
            ),
            Lang::Fr => (
                "Désactiver Bing & Cortana dans la recherche",
                "Supprime les suggestions de recherche Web Bing et Cortana du menu Démarrer et de la barre des tâches.",
                "Sûr. Rend la recherche locale plus rapide et privée.",
            ),
        }),
        "disable_lockscreen_tips" => Some(match lang {
            Lang::En => (
                "Disable Lockscreen Tips and Tricks",
                "Stops Windows from displaying trivia, advertisements, and tips on the lock screen.",
                "Safe aesthetic tweak with zero impact on system stability.",
            ),
            Lang::Fr => (
                "Désactiver les astuces de l'écran de verrouillage",
                "Empêche l'affichage de conseils, anecdotes et publicités sur l'écran de verrouillage.",
                "Réglage esthétique sûr sans impact sur la stabilité du système.",
            ),
        }),
        "disable_windows_suggestions" | "disable_windows_suggestions_ads" => Some(match lang {
            Lang::En => (
                "Disable Windows Suggestions & Ads",
                "Blocks app recommendations, promotional offers, and tips in Start, Settings, and Notifications.",
                "Safe. Eliminates promotional clutter without removing any system tools.",
            ),
            Lang::Fr => (
                "Désactiver les publicités & suggestions Windows",
                "Bloque les recommandations d'applications, les pubs Microsoft 365 et les conseils dans Démarrer et Paramètres.",
                "Sûr. Supprime les éléments promotionnels sans affecter vos applications.",
            ),
        }),
        "disable_activity_history" => Some(match lang {
            Lang::En => (
                "Disable Activity History",
                "Prevents Windows from storing user timeline activities and uploading them to Microsoft servers.",
                "Safe privacy enhancement. Does not affect recent file shortcuts in File Explorer.",
            ),
            Lang::Fr => (
                "Désactiver l'historique d'activité",
                "Empêche Windows d'enregistrer et de synchroniser l'historique d'activités des applications.",
                "Amélioration de confidentialité sûre. N'altère pas les récents de l'Explorateur.",
            ),
        }),
        "disable_desktop_spotlight" => Some(match lang {
            Lang::En => (
                "Disable Windows Spotlight on Desktop",
                "Removes the 'Learn about this picture' desktop icon and forced wallpaper rotations.",
                "Safe cosmetic tweak. Allows permanent custom wallpapers.",
            ),
            Lang::Fr => (
                "Désactiver Windows Spotlight sur le Bureau",
                "Retire l'icône « Windows à la une » (Spotlight) et le rafraîchissement d'images imposé sur le Bureau.",
                "Réglage esthétique sûr. Permet de conserver son fond d'écran sans altération.",
            ),
        }),
        "classic_context_menu" => Some(match lang {
            Lang::En => (
                "Classic Context Menu (Windows 10 Style)",
                "Restores the traditional full right-click context menu, bypassing 'Show more options'.",
                "Safe registry tweak. Requires File Explorer restart to take full effect.",
            ),
            Lang::Fr => (
                "Menu contextuel classique (Style Windows 10)",
                "Restaure le clic droit complet immédiat sans le sous-menu « Afficher plus d'options ».",
                "Réglage de registre sûr. Nécessite un redémarrage de l'Explorateur pour s'appliquer.",
            ),
        }),
        "show_file_extensions" => Some(match lang {
            Lang::En => (
                "Show Known File Extensions",
                "Always reveals extensions (.exe, .pdf, .docx, etc.) in File Explorer.",
                "Strongly recommended for security against malicious file spoofing.",
            ),
            Lang::Fr => (
                "Afficher les extensions de fichiers connues",
                "Affiche toujours les extensions (.exe, .pdf, .docx...) dans l'Explorateur Windows pour une meilleure sécurité.",
                "Fortement recommandé pour éviter les pièges de double extension (.pdf.exe).",
            ),
        }),
        "show_hidden_files" | "show_hidden_folders" => Some(match lang {
            Lang::En => (
                "Show Hidden Files & Folders",
                "Makes all hidden system files, directories, and drives visible in File Explorer.",
                "Safe for advanced users. Exercise care when modifying unveiled system files.",
            ),
            Lang::Fr => (
                "Afficher les fichiers et dossiers cachés",
                "Rend visibles l'ensemble des fichiers, dossiers et lecteurs cachés dans l'Explorateur Windows.",
                "Sûr pour les utilisateurs avancés. Attention à ne pas supprimer de fichiers critiques.",
            ),
        }),
        "launch_explorer_this_pc" => Some(match lang {
            Lang::En => (
                "Open Explorer to 'This PC'",
                "Configures File Explorer to open directly to 'This PC' instead of Home / Quick Access.",
                "Completely safe usability preference.",
            ),
            Lang::Fr => (
                "Ouvrir l'Explorateur sur « Ce PC »",
                "Configure l'Explorateur pour s'ouvrir directement sur « Ce PC » au lieu de la page Accueil / Accès rapide.",
                "Préférence ergonomique entièrement sûre.",
            ),
        }),
        "enable_dark_mode" => Some(match lang {
            Lang::En => (
                "Enable Dark Mode (System & Apps)",
                "Switches the Windows desktop shell and compatible applications to dark theme.",
                "Safe visual preference. Does not modify system binaries.",
            ),
            Lang::Fr => (
                "Activer le Mode Sombre Windows & Applications",
                "Bascule l'ensemble du système d'exploitation et des applications compatibles en mode sombre.",
                "Préférence visuelle sûre. Ne modifie aucun fichier système.",
            ),
        }),
        "enable_end_task_taskbar" => Some(match lang {
            Lang::En => (
                "Enable 'End Task' on Taskbar",
                "Adds an 'End Task' option to right-clicked taskbar app buttons to quickly kill frozen apps.",
                "Safe native Windows 11 feature. Avoid closing unsaved documents.",
            ),
            Lang::Fr => (
                "Activer « Fin de tâche » dans la barre des tâches",
                "Ajoute le bouton « Fin de tâche » au clic droit sur les applications de la barre des tâches pour tuer un processus gelé.",
                "Fonction native Windows 11 sûre. Attention à ne pas fermer un document non enregistré.",
            ),
        }),
        "disable_widgets" => Some(match lang {
            Lang::En => (
                "Disable Taskbar Widgets",
                "Removes the Widgets/Weather button from the taskbar and disables the Web Experience feed service.",
                "Safe. Saves RAM and CPU cycles consumed by background web views.",
            ),
            Lang::Fr => (
                "Désactiver les Widgets de la barre des tâches",
                "Supprime le bouton Widgets/Météo de la barre des tâches et désactive le flux d'actualités.",
                "Sûr. Économise de la RAM et des cycles processeur consommés en arrière-plan.",
            ),
        }),
        "disable_mouse_accel" => Some(match lang {
            Lang::En => (
                "Disable Mouse Acceleration (1:1 Gaming Raw Input)",
                "Turns off 'Enhance pointer precision' for a strictly linear and consistent mouse response.",
                "Safe. Ideal for competitive gaming, easily toggled back if desktop muscle memory differs.",
            ),
            Lang::Fr => (
                "Désactiver l'accélération souris (Gaming 1:1)",
                "Désactive « Améliorer la précision du pointeur » pour une sensibilité souris parfaitement linéaire en jeu.",
                "Sûr. Idéal pour les FPS et le jeu compétitif, réactivable instantanément.",
            ),
        }),
        "disable_sticky_keys" => Some(match lang {
            Lang::En => (
                "Disable Sticky Keys Shortcut",
                "Prevents accidental triggering of the Sticky Keys prompt when pressing Shift 5 times.",
                "Safe. Highly recommended for gamers and fast typists.",
            ),
            Lang::Fr => (
                "Désactiver le raccourci des touches rémanentes",
                "Empêche l'ouverture impromptue de la boîte de dialogue Touches rémanentes lors de 5 pressions sur Maj.",
                "Sûr. Fortement recommandé pour les joueurs et les dactylographes rapides.",
            ),
        }),
        "disable_fast_startup" => Some(match lang {
            Lang::En => (
                "Disable Fast Startup (Hybrid Sleep)",
                "Enforces a clean Windows kernel shutdown, resolving hardware glitch and inaccurate uptime.",
                "Safe. Resolves reboot hangs and driver initialization anomalies.",
            ),
            Lang::Fr => (
                "Désactiver le Démarrage rapide (Fast Startup)",
                "Assure un arrêt complet et propre du noyau Windows, évitant les instabilités matérielles et d'uptime faussé.",
                "Sûr. Résout les blocages de redémarrage et les erreurs de mise à jour BIOS/pilotes.",
            ),
        }),
        _ => None,
    }
}

/// Textes traduits (libellé court, sous-titre, corps d'infobulle, note de sécurité) pour les 8 tâches de réparation.
pub fn repair_task_text(
    lang: Lang,
    task: RepairTask,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match task {
        RepairTask::Dism => match lang {
            Lang::En => (
                "DISM Image Repair",
                "Scans and restores Windows component store integrity",
                "Executes DISM /Online /Cleanup-Image (/CheckHealth, /ScanHealth, /RestoreHealth) and deep WinSxS cleanup (/StartComponentCleanup /ResetBase).",
                "Official Microsoft recovery tool. Safe and highly effective against corrupted system packages.",
            ),
            Lang::Fr => (
                "DISM - Image Windows",
                "Répare le magasin de composants Windows en profondeur",
                "Exécute DISM /Online /Cleanup-Image (/CheckHealth, /ScanHealth, /RestoreHealth) et nettoie le dossier WinSxS (/StartComponentCleanup /ResetBase).",
                "Outil officiel Microsoft. Sûr et hautement recommandé en cas d'erreurs système ou de mises à jour bloquées.",
            ),
        },
        RepairTask::Sfc => match lang {
            Lang::En => (
                "SFC System Integrity",
                "Scans and fixes protected system files (sfc /scannow)",
                "Performs a full scan of all protected operating system files and replaces corrupted files with a cached copy.",
                "Safe official Windows tool. Does not affect your personal files or documents.",
            ),
            Lang::Fr => (
                "SFC - Fichiers système",
                "Analyse et répare l'intégrité des fichiers système protégés",
                "Lance sfc /scannow pour vérifier chaque fichier protégé de Windows et remplacer les copies endommagées depuis le cache.",
                "Outil officiel Microsoft entièrement sûr. Ne touche à aucun document ou fichier personnel.",
            ),
        },
        RepairTask::Chkdsk => match lang {
            Lang::En => (
                "CHKDSK Disk Check",
                "Checks NTFS filesystem metadata and schedules repairs",
                "Performs a read-only scan of drive C: and offers to schedule chkdsk /F /R /X upon next system restart if bad blocks are detected.",
                "Safe online scan. Offline repair will require a system reboot.",
            ),
            Lang::Fr => (
                "CHKDSK - Vérification disque",
                "Contrôle les métadonnées NTFS et planifie une réparation",
                "Effectue une vérification rapide en lecture seule de C: et propose de planifier chkdsk /F /R /X au prochain redémarrage en cas d'anomalie.",
                "Analyse en ligne sans risque. La réparation physique nécessite un redémarrage du PC.",
            ),
        },
        RepairTask::WindowsUpdate => match lang {
            Lang::En => (
                "Windows Update Reset",
                "Stops services, clears update cache & re-registers DLLs",
                "Stops wuauserv, bits, cryptSvc, backs up SoftwareDistribution and catroot2, re-registers 36 core system DLLs, and resets update policies.",
                "Safe. Clears stuck or corrupt downloads without removing already installed updates.",
            ),
            Lang::Fr => (
                "Windows Update Reset",
                "Purge le cache, réenregistre 36 DLLs et relance les services",
                "Arrête wuauserv, bits, cryptSvc, sauvegarde SoftwareDistribution & catroot2, réenregistre 36 DLLs et réinitialise les stratégies de mise à jour.",
                "Sûr. Résout les erreurs de téléchargement 0x800... sans désinstaller les mises à jour déjà appliquées.",
            ),
        },
        RepairTask::ResetNetwork => match lang {
            Lang::En => (
                "Network Stack Reset",
                "Flushes DNS, resets Winsock, TCP/IP & Windows Firewall",
                "Clears DNS cache, resets Winsock catalog, resets TCP/IP stack to defaults, clears WinHTTP proxy, and restores Windows Firewall rules.",
                "Safe. May momentarily drop internet connections. Custom static IP or proxy configurations will revert to DHCP defaults.",
            ),
            Lang::Fr => (
                "Reset Réseau & TCP/IP",
                "Vide le cache DNS, réinitialise Winsock, TCP/IP & Pare-feu",
                "Vide le cache DNS, réinitialise le catalogue Winsock, la pile TCP/IP, le proxy WinHTTP et rétablit les règles par défaut du pare-feu.",
                "Sûr. Coupure réseau brève. Si vous utilisiez une IP fixe ou un proxy d'entreprise, ils seront réinitialisés en DHCP automatique.",
            ),
        },
        RepairTask::ClearTemp => match lang {
            Lang::En => (
                "Temp & Cache Cleanup",
                "Empties %TEMP%, Windows Temp, Prefetch & runs cleanmgr",
                "Safely deletes obsolete temporary files from user and system temp folders, clears old prefetch entries, internet caches, and launches Cleanmgr.",
                "Safe. Locked or actively running application files are automatically skipped.",
            ),
            Lang::Fr => (
                "Nettoyage Fichiers Temp",
                "Vide %TEMP%, C:\\Windows\\Temp, Prefetch & lance Cleanmgr",
                "Supprime les fichiers temporaires obsolètes de l'utilisateur et du système, purge le Prefetch, les caches Internet et exécute cleanmgr.",
                "Sûr. Les fichiers actuellement utilisés par des programmes ouverts sont automatiquement ignorés.",
            ),
        },
        RepairTask::WindowsStore => match lang {
            Lang::En => (
                "Microsoft Store Repair",
                "Runs wsreset and re-registers AppX packages",
                "Clears and resets the Microsoft Store cache via wsreset.exe, and repairs corrupted AppX manifest registrations for all user profiles.",
                "Safe. Fixes crashing or opening issues with built-in Windows applications.",
            ),
            Lang::Fr => (
                "Réparation Windows Store",
                "Exécute wsreset.exe et réenregistre les packages AppX",
                "Réinitialise le cache du Microsoft Store via wsreset.exe et réenregistre les manifestes AppX pour réparer les applications Windows intégrées.",
                "Sûr. Résout les crashs au lancement du Store, Calculatrice, Photos ou Paramètres.",
            ),
        },
        RepairTask::CriticalServices => match lang {
            Lang::En => (
                "Critical Services Check",
                "Verifies and restores startup config for essential services",
                "Checks and restores correct startup types (Automatic/Manual) and running status for BITS, CryptSvc, TrustedInstaller, Winmgmt, EventLog, Dhcp, and Dnscache.",
                "Safe. Restores default Microsoft configuration for accidentally disabled core services.",
            ),
            Lang::Fr => (
                "Services Critiques",
                "Contrôle et rétablit la configuration des services vitaux",
                "Vérifie l'état et reconfigure les types de démarrage de wuauserv, BITS, CryptSvc, TrustedInstaller, Winmgmt, EventLog, Dhcp et Dnscache.",
                "Sûr. Restaure la configuration officielle Microsoft si un service essentiel a été désactivé par erreur.",
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_tr_toggle_and_code() {
        assert_eq!(Lang::En.tr("Hello", "Bonjour"), "Hello");
        assert_eq!(Lang::Fr.tr("Hello", "Bonjour"), "Bonjour");

        assert_eq!(Lang::En.toggle(), Lang::Fr);
        assert_eq!(Lang::Fr.toggle(), Lang::En);

        assert_eq!(Lang::En.code(), "EN");
        assert_eq!(Lang::Fr.code(), "FR");
    }

    #[test]
    fn test_all_18_debloat_tweaks_exist_in_en_and_fr() {
        let expected_ids = [
            "disable_copilot",
            "disable_recall",
            "disable_click_to_do",
            "disable_telemetry",
            "disable_bing_search",
            "disable_lockscreen_tips",
            "disable_windows_suggestions",
            "disable_activity_history",
            "classic_context_menu",
            "show_file_extensions",
            "show_hidden_files",
            "launch_explorer_this_pc",
            "enable_dark_mode",
            "enable_end_task_taskbar",
            "disable_widgets",
            "disable_mouse_accel",
            "disable_sticky_keys",
            "disable_fast_startup",
        ];

        assert_eq!(expected_ids.len(), 18);

        for id in expected_ids {
            let en_res = debloat_tweak_text(Lang::En, id);
            assert!(
                en_res.is_some(),
                "Tweak ID '{}' doit être défini en Anglais",
                id
            );
            let (title_en, desc_en, safety_en) = en_res.unwrap();
            assert!(!title_en.is_empty(), "Title EN vide pour {}", id);
            assert!(!desc_en.is_empty(), "Desc EN vide pour {}", id);
            assert!(!safety_en.is_empty(), "Safety EN vide pour {}", id);

            let fr_res = debloat_tweak_text(Lang::Fr, id);
            assert!(
                fr_res.is_some(),
                "Tweak ID '{}' doit être défini en Français",
                id
            );
            let (title_fr, desc_fr, safety_fr) = fr_res.unwrap();
            assert!(!title_fr.is_empty(), "Title FR vide pour {}", id);
            assert!(!desc_fr.is_empty(), "Desc FR vide pour {}", id);
            assert!(!safety_fr.is_empty(), "Safety FR vide pour {}", id);
        }
    }

    #[test]
    fn test_repair_tasks_translations() {
        for task in RepairTask::ALL {
            let (en_short, en_sub, en_body, en_safe) = repair_task_text(Lang::En, task);
            assert!(!en_short.is_empty());
            assert!(!en_sub.is_empty());
            assert!(!en_body.is_empty());
            assert!(!en_safe.is_empty());

            let (fr_short, fr_sub, fr_body, fr_safe) = repair_task_text(Lang::Fr, task);
            assert!(!fr_short.is_empty());
            assert!(!fr_sub.is_empty());
            assert!(!fr_body.is_empty());
            assert!(!fr_safe.is_empty());
        }
    }

    #[test]
    fn test_debloat_categories_titles() {
        let cats = [
            DebloatCategory::AiAndCopilot,
            DebloatCategory::PrivacyAndTelemetry,
            DebloatCategory::ExplorerAndUi,
            DebloatCategory::SystemAndGaming,
        ];

        for cat in cats {
            let en = debloat_category_title(Lang::En, cat);
            let fr = debloat_category_title(Lang::Fr, cat);
            assert!(!en.is_empty());
            assert!(!fr.is_empty());
        }
    }
}
