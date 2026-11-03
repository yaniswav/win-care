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
                "▪ Default: Windows loads the Copilot web app and background runtime on taskbar and shortcut (Win+C).\n▪ If enabled: Hides the Copilot button and blocks Copilot execution via registry policies, saving RAM and CPU.\n▪ If skipped: Copilot remains accessible and active in the background on the taskbar.",
                "Completely safe. Can be re-enabled anytime without side effects.",
            ),
            Lang::Fr => (
                "Désactiver Windows Copilot",
                "▪ Par défaut : Windows charge le bouton et l'environnement d'arrière-plan Copilot dans la barre des tâches (Win+C).\n▪ Si utilisé : Masque le bouton et désactive l'exécution de Copilot via le Registre, libérant de la RAM et du processeur.\n▪ Si non utilisé : Copilot reste actif en arrière-plan et accessible dans la barre des tâches.",
                "Totalement sûr. Réversible à tout moment sans effet indésirable.",
            ),
        }),
        "disable_recall" | "disable_ai_recall" => Some(match lang {
            Lang::En => (
                "Disable Windows AI Recall",
                "▪ Default: Windows AI continuously takes snapshots of your screen every few seconds and indexes activity.\n▪ If enabled: Disables AI data analysis, stops automated screenshot capture, and blocks Recall policies completely.\n▪ If skipped: Screen capture and background indexing services remain active, consuming disk and memory.",
                "Safe. Protects privacy by preventing automated screen snapshot archiving.",
            ),
            Lang::Fr => (
                "Désactiver Windows AI Recall",
                "▪ Par défaut : L'IA de Windows capture en continu des instantanés de l'écran toutes les quelques secondes et indexe vos activités.\n▪ Si utilisé : Désactive l'analyse des données IA, bloque la capture d'écran continue et neutralise les stratégies Recall.\n▪ Si non utilisé : L'indexation et les captures d'écran restent actives en tâche de fond, consommant disque et mémoire.",
                "Sûr. Protège la vie privée en empêchant l'archivage automatique d'écrans.",
            ),
        }),
        "disable_click_to_do" => Some(match lang {
            Lang::En => (
                "Disable Click to Do AI",
                "▪ Default: Windows 11 monitors screen contents to propose contextual AI actions and visual overlays.\n▪ If enabled: Disables the Click to Do AI overlay and halts background vision analysis services.\n▪ If skipped: Contextual AI features stay active in the background, consuming CPU and NPU cycles.",
                "Safe. Reduces background AI service processing overhead.",
            ),
            Lang::Fr => (
                "Désactiver Click to Do IA",
                "▪ Par défaut : Windows 11 analyse le contenu affiché à l'écran pour proposer des actions contextuelles assistées par IA.\n▪ Si utilisé : Désactive la surcouche Click to Do et arrête les services d'analyse visuelle en arrière-plan.\n▪ Si non utilisé : Les fonctions IA contextuelles restent actives en tâche de fond, sollicitant CPU et NPU.",
                "Sûr. Réduit la charge des services IA d'arrière-plan.",
            ),
        }),
        "disable_edge_notepad_paint_ai" => Some(match lang {
            Lang::En => (
                "Disable AI in Edge, Notepad & Paint",
                "▪ Default: Windows integrates Copilot sidebar in Edge, AI text rewriting in Notepad, and Cocreator in Paint.\n▪ If enabled: Disables cloud AI integrations across Edge, Notepad, and Paint, keeping them local and lightweight.\n▪ If skipped: AI sidebar and cloud-assisted generation tools remain active in these applications.",
                "Safe. Preserves standard offline notepad and image editing features.",
            ),
            Lang::Fr => (
                "Désactiver l'IA dans Edge, Bloc-notes et Paint",
                "▪ Par défaut : Windows intègre la barre Copilot dans Edge, la réécriture IA dans le Bloc-notes et Cocreator dans Paint.\n▪ Si utilisé : Désactive les fonctions IA cloud dans ces trois applications pour des outils 100 % légers et locaux.\n▪ Si non utilisé : Les barres latérales et assistants de génération en ligne demeurent actifs.",
                "Sûr. Conserve les fonctions classiques de retouche et de prise de note.",
            ),
        }),
        "disable_telemetry" => Some(match lang {
            Lang::En => (
                "Disable Windows Telemetry",
                "▪ Default: Windows constantly transmits diagnostic logs, usage data, Advertising ID, and telemetry to Microsoft servers.\n▪ If enabled: Minimizes telemetry via Group Policy, stops DiagTrack diagnostics, and turns off advertising ID.\n▪ If skipped: Windows continues sending telemetry, usage telemetry packets, and targeted diagnostics over your network.",
                "Safe. Windows Update and core services remain fully operational.",
            ),
            Lang::Fr => (
                "Désactiver la Télémétrie Windows",
                "▪ Par défaut : Windows transmet en continu des journaux de diagnostic, données d'usage et l'identifiant publicitaire à Microsoft.\n▪ Si utilisé : Réduit la télémétrie au minimum légal via les stratégies, désactive DiagTrack et protège votre vie privée.\n▪ Si non utilisé : Windows continue d'émettre des rapports d'utilisation et de diagnostics en continu sur le réseau.",
                "Sûr. Windows Update et les composants essentiels continuent de fonctionner normalement.",
            ),
        }),
        "disable_bing_search" => Some(match lang {
            Lang::En => (
                "Disable Bing & Cortana in Windows Search",
                "▪ Default: Windows Search queries Bing web servers for every Start Menu search, showing web results and ads.\n▪ If enabled: Restricts Windows Search to local files, folders, and apps only, speeding up search results.\n▪ If skipped: Start Menu searches query Bing over the internet, showing external links and slowing down local queries.",
                "Safe. Search becomes local, faster, and respects your privacy.",
            ),
            Lang::Fr => (
                "Désactiver Bing & Cortana dans la recherche",
                "▪ Par défaut : La recherche du menu Démarrer interroge les serveurs Web Bing à chaque frappe et affiche des pubs en ligne.\n▪ Si utilisé : Restreint la recherche aux fichiers, programmes et paramètres locaux, éliminant les lenteurs réseau.\n▪ Si non utilisé : Le menu Démarrer continue d'envoyer vos requêtes sur Internet et d'afficher des résultats Web.",
                "Sûr. Rend la recherche locale plus rapide et privée.",
            ),
        }),
        "disable_lockscreen_tips" => Some(match lang {
            Lang::En => (
                "Disable Lockscreen Tips and Tricks",
                "▪ Default: Windows displays dynamic rotating tips, promotional banners, and trivia on your lock screen.\n▪ If enabled: Hides all promotional overlays and recommendations on the lock screen for a clean interface.\n▪ If skipped: Windows continues fetching and displaying rotating tips and sponsored text on the lock screen.",
                "Safe aesthetic tweak with zero impact on system stability.",
            ),
            Lang::Fr => (
                "Désactiver les astuces de l'écran de verrouillage",
                "▪ Par défaut : Windows affiche des conseils dynamiques, anecdotes et bannières promotionnelles sur l'écran de verrouillage.\n▪ Si utilisé : Masque toutes les incitations et textes sponsorisés pour un écran de verrouillage épuré.\n▪ Si non utilisé : L'écran de verrouillage continuera d'afficher des astuces et suggestions commerciales périodiques.",
                "Réglage esthétique sûr sans impact sur la stabilité du système.",
            ),
        }),
        "disable_windows_suggestions" | "disable_windows_suggestions_ads" => Some(match lang {
            Lang::En => (
                "Disable Windows Suggestions & Ads",
                "▪ Default: Windows suggests sponsored apps in Start Menu, displays promotional notifications, and ads in Settings.\n▪ If enabled: Blocks suggested apps, promotional notification banners, and welcome experience advertisements.\n▪ If skipped: Windows periodically displays suggested apps in Start Menu and promotional popups in Settings.",
                "Safe. Eliminates promotional clutter without removing any system tools.",
            ),
            Lang::Fr => (
                "Désactiver les publicités & suggestions Windows",
                "▪ Par défaut : Windows suggère des applications sponsorisées dans Démarrer et affiche des bannières pub dans Paramètres.\n▪ Si utilisé : Bloque les recommandations d'applications, les notifications promotionnelles et les bannières publicitaires.\n▪ Si non utilisé : Windows continuera d'insérer des suggestions d'applications et offres commerciales dans l'interface.",
                "Sûr. Supprime les éléments promotionnels sans affecter vos applications.",
            ),
        }),
        "disable_activity_history" => Some(match lang {
            Lang::En => (
                "Disable Activity History",
                "▪ Default: Windows records opened applications, documents, and web sessions, synchronizing them with Microsoft servers.\n▪ If enabled: Stops recording user activity history and prevents cloud synchronization of your app usage.\n▪ If skipped: Windows continues logging detailed application and browsing history timeline entries.",
                "Safe privacy enhancement. Does not affect recent file shortcuts in File Explorer.",
            ),
            Lang::Fr => (
                "Désactiver l'historique d'activité",
                "▪ Par défaut : Windows enregistre la chronologie de vos applications et documents ouverts et les synchronise avec le cloud.\n▪ Si utilisé : Interrompt l'enregistrement de l'historique d'activité et bloque sa synchronisation sur les serveurs distants.\n▪ Si non utilisé : Windows poursuit la traçabilité continue de l'utilisation de vos fichiers et logiciels.",
                "Amélioration de confidentialité sûre. N'altère pas les récents de l'Explorateur.",
            ),
        }),
        "disable_desktop_spotlight" => Some(match lang {
            Lang::En => (
                "Disable Windows Spotlight on Desktop",
                "▪ Default: Windows Spotlight automatically changes desktop wallpapers and places a 'Learn about this picture' icon.\n▪ If enabled: Disables Spotlight on the desktop, removing the desktop icon and keeping your chosen static wallpaper.\n▪ If skipped: Windows periodically downloads new wallpaper images from Bing and displays the desktop Spotlight shortcut.",
                "Safe cosmetic tweak. Allows permanent custom wallpapers.",
            ),
            Lang::Fr => (
                "Désactiver Windows Spotlight sur le Bureau",
                "▪ Par défaut : Windows Spotlight remplace automatiquement le fond d'écran et place une icône d'information sur le Bureau.\n▪ Si utilisé : Désactive Spotlight sur le Bureau, retire l'icône obligatoire et fige votre fond d'écran choisi.\n▪ Si non utilisé : Windows continue de télécharger des images Bing et de modifier régulièrement l'arrière-plan du Bureau.",
                "Réglage esthétique sûr. Permet de conserver son fond d'écran sans altération.",
            ),
        }),
        "classic_context_menu" => Some(match lang {
            Lang::En => (
                "Classic Context Menu (Windows 10 Style)",
                "▪ Default: Windows 11 shows a compact, truncated right-click menu requiring 'Show more options' (Shift+F10) for full tools.\n▪ If enabled: Restores the classic Windows 10 right-click menu with direct access to all actions and third-party tools.\n▪ If skipped: File Explorer keeps the modern condensed two-tier context menu.",
                "Safe registry tweak. Requires File Explorer restart to take full effect.",
            ),
            Lang::Fr => (
                "Menu contextuel classique (Style Windows 10)",
                "▪ Par défaut : Windows 11 affiche un menu clic droit simplifié obligeant à cliquer sur « Afficher plus d'options ».\n▪ Si utilisé : Restaure le menu contextuel traditionnel de Windows 10 avec un accès direct à tous les outils et logiciels tiers.\n▪ Si non utilisé : L'Explorateur conserve le menu à deux niveaux nécessitant un clic supplémentaire.",
                "Réglage de registre sûr. Nécessite un redémarrage de l'Explorateur pour s'appliquer.",
            ),
        }),
        "show_file_extensions" => Some(match lang {
            Lang::En => (
                "Show Known File Extensions",
                "▪ Default: Windows hides extensions (.exe, .pdf, .docx, .vbs) for recognized file types in File Explorer.\n▪ If enabled: Always reveals full file extensions, preventing malicious spoofing like fake PDFs (.pdf.exe).\n▪ If skipped: Extensions stay hidden, making it harder to discern true executable file formats.",
                "Strongly recommended for security against malicious file spoofing.",
            ),
            Lang::Fr => (
                "Afficher les extensions de fichiers connues",
                "▪ Par défaut : Windows masque les extensions des types connus (.exe, .pdf, .docx) dans l'Explorateur.\n▪ Si utilisé : Affiche systématiquement l'extension réelle de chaque fichier, déjouant les pièges de double extension (.pdf.exe).\n▪ Si non utilisé : Les extensions demeurent masquées, augmentant le risque d'exécution accidentelle de fichiers malveillants.",
                "Fortement recommandé pour éviter les pièges de double extension (.pdf.exe).",
            ),
        }),
        "show_hidden_files" | "show_hidden_folders" => Some(match lang {
            Lang::En => (
                "Show Hidden Files & Folders",
                "▪ Default: Windows hides system files and AppData folders from ordinary Explorer navigation.\n▪ If enabled: Makes all hidden folders, system directories, and configuration files immediately visible.\n▪ If skipped: Hidden folders remain concealed, requiring manual path entry to access AppData and system locations.",
                "Safe for advanced users. Exercise care when modifying unveiled system files.",
            ),
            Lang::Fr => (
                "Afficher les fichiers et dossiers cachés",
                "▪ Par défaut : Windows dissimule les dossiers système et répertoires de configuration (ex: AppData) dans l'Explorateur.\n▪ Si utilisé : Rend visibles tous les dossiers et fichiers cachés pour faciliter la navigation technique et le dépannage.\n▪ Si non utilisé : Les dossiers masqués restent invisibles, nécessitant la saisie manuelle de leur chemin absolu.",
                "Sûr pour les utilisateurs avancés. Attention à ne pas supprimer de fichiers critiques.",
            ),
        }),
        "launch_explorer_this_pc" => Some(match lang {
            Lang::En => (
                "Open Explorer to 'This PC'",
                "▪ Default: Opening File Explorer defaults to Home/Quick Access, loading recent files, frequent folders, and cloud items.\n▪ If enabled: Directly opens 'This PC' showing drives, partitions, and storage devices without cloud loading lag.\n▪ If skipped: Explorer opens to Home, showing recent documents and pinned frequent folders.",
                "Completely safe usability preference.",
            ),
            Lang::Fr => (
                "Ouvrir l'Explorateur sur « Ce PC »",
                "▪ Par défaut : L'Explorateur s'ouvre sur Accueil / Accès rapide, chargeant l'historique récent et les liens cloud OneDrive.\n▪ Si utilisé : Ouvre directement la vue « Ce PC » avec l'ensemble des disques et lecteurs sans latence de chargement.\n▪ Si non utilisé : L'Explorateur continue de s'ouvrir sur la page d'accueil avec vos fichiers récents.",
                "Préférence ergonomique entièrement sûre.",
            ),
        }),
        "enable_dark_mode" => Some(match lang {
            Lang::En => (
                "Enable Dark Mode (System & Apps)",
                "▪ Default: Windows default theme uses bright light styling for taskbar, windows, menus, and UWP apps.\n▪ If enabled: Activates dark theme across the Windows desktop shell, Settings, File Explorer, and supported apps.\n▪ If skipped: Windows maintains its current light theme or OEM default visual configuration.",
                "Safe visual preference. Does not modify system binaries.",
            ),
            Lang::Fr => (
                "Activer le Mode Sombre Windows & Applications",
                "▪ Par défaut : Windows utilise le thème clair par défaut pour la barre des tâches, fenêtres et applications.\n▪ Si utilisé : Bascule l'environnement de bureau, l'Explorateur, les Paramètres et les applications en mode sombre reposant.\n▪ Si non utilisé : Le système conserve le thème clair actif ou la configuration visuelle par défaut du constructeur.",
                "Préférence visuelle sûre. Ne modifie aucun fichier système.",
            ),
        }),
        "enable_end_task_taskbar" => Some(match lang {
            Lang::En => (
                "Enable 'End Task' on Taskbar",
                "▪ Default: Taskbar right-click only offers 'Close window', which hangs if the target application is frozen or unresponsive.\n▪ If enabled: Adds an instant 'End Task' option to force-kill frozen processes directly from the taskbar without Task Manager.\n▪ If skipped: Terminating unresponsive programs requires opening Task Manager or using command line tools.",
                "Safe native Windows 11 feature. Avoid closing unsaved documents.",
            ),
            Lang::Fr => (
                "Activer « Fin de tâche » dans la barre des tâches",
                "▪ Par défaut : Le clic droit dans la barre des tâches ne propose que « Fermer la fenêtre », inefficace si l'application est gelée.\n▪ Si utilisé : Ajoute l'option native « Fin de tâche » au clic droit pour forcer l'arrêt immédiat d'un processus sans le Gestionnaire.\n▪ Si non utilisé : L'arrêt d'un logiciel bloqué nécessite d'ouvrir le Gestionnaire des tâches ou d'utiliser la console.",
                "Fonction native Windows 11 sûre. Attention à ne pas fermer un document non enregistré.",
            ),
        }),
        "disable_widgets" => Some(match lang {
            Lang::En => (
                "Disable Taskbar Widgets",
                "▪ Default: Windows runs the Widgets taskbar icon and background Web Experience service, fetching news and MSN feeds.\n▪ If enabled: Hides the Widgets icon and turns off the background news feed service, saving 150-300 MB of RAM.\n▪ If skipped: Widgets service runs in the background, consuming memory and network bandwidth for live updates.",
                "Safe. Saves RAM and CPU cycles consumed by background web views.",
            ),
            Lang::Fr => (
                "Désactiver les Widgets de la barre des tâches",
                "▪ Par défaut : Windows affiche le bouton Widgets/Météo et maintient le service Web Experience actif pour charger les actualités MSN.\n▪ Si utilisé : Masque le bouton Widgets et désactive le flux d'arrière-plan, économisant 150 à 300 Mo de RAM et du réseau.\n▪ Si non utilisé : Le service Widgets continue de s'exécuter en arrière-plan et de télécharger des flux d'actualités.",
                "Sûr. Économise de la RAM et des cycles processeur consommés en arrière-plan.",
            ),
        }),
        "disable_mouse_accel" => Some(match lang {
            Lang::En => (
                "Disable Mouse Acceleration (1:1 Gaming Raw Input)",
                "▪ Default: Windows activates 'Enhance pointer precision', altering cursor speed based on physical mouse movement velocity.\n▪ If enabled: Enforces true 1:1 linear mouse response by disabling cursor acceleration curves, essential for muscle memory in games.\n▪ If skipped: Cursor moves disproportionately faster during swift hand movements, hindering precise crosshair aiming.",
                "Safe. Ideal for competitive gaming, easily toggled back if desktop muscle memory differs.",
            ),
            Lang::Fr => (
                "Désactiver l'accélération souris (Gaming 1:1)",
                "▪ Par défaut : Windows active « Améliorer la précision du pointeur », modifiant la vitesse du curseur selon la vitesse du geste.\n▪ Si utilisé : Établit un ratio 1:1 strictement linéaire en supprimant la courbe d'accélération, idéal pour la mémoire musculaire en jeu.\n▪ Si non utilisé : Le curseur accélère de façon non proportionnelle lors des mouvements vifs, gênant la précision en jeu.",
                "Sûr. Idéal pour les FPS et le jeu compétitif, réactivable instantanément.",
            ),
        }),
        "disable_sticky_keys" => Some(match lang {
            Lang::En => (
                "Disable Sticky Keys Shortcut",
                "▪ Default: Pressing the Shift key 5 times consecutively triggers an accessibility pop-up with a high-pitched beep.\n▪ If enabled: Disables the 5x Shift keyboard shortcut trigger while preserving manual Sticky Keys in Windows accessibility settings.\n▪ If skipped: Rapid tapping of Shift during gaming or typing will interrupt your screen with the Sticky Keys prompt.",
                "Safe. Highly recommended for gamers and fast typists.",
            ),
            Lang::Fr => (
                "Désactiver le raccourci des touches rémanentes",
                "▪ Par défaut : Appuyer 5 fois rapidement sur la touche Maj déclenche une boîte de dialogue bloquante avec un bip sonore.\n▪ Si utilisé : Désactive le raccourci clavier des 5x Maj tout en conservant l'accessibilité disponible dans les Paramètres.\n▪ Si non utilisé : Des appuis répétés sur Maj en jeu ou en saisie rapide feront surgir intempestivement la fenêtre d'alerte.",
                "Sûr. Fortement recommandé pour les joueurs et les dactylographes rapides.",
            ),
        }),
        "disable_fast_startup" => Some(match lang {
            Lang::En => (
                "Disable Fast Startup (Hybrid Sleep)",
                "▪ Default: Windows hibernates the kernel and drivers to disk on shutdown (Fast Startup) instead of performing a clean shutdown.\n▪ If enabled: Forces a true complete shutdown, clearing kernel state, resolving driver glitches, and keeping uptime accurate.\n▪ If skipped: Kernel state persists across shutdowns, accumulating memory leaks, driver bugs, and blocking dual-boot disk access.",
                "Safe. Resolves reboot hangs and driver initialization anomalies.",
            ),
            Lang::Fr => (
                "Désactiver le Démarrage rapide (Fast Startup)",
                "▪ Par défaut : Windows met en veille prolongée le noyau et les pilotes à l'extinction (Fast Startup) au lieu d'un arrêt complet.\n▪ Si utilisé : Force un arrêt système propre et réel, purgeant la mémoire du noyau et résolvant les bugs de pilotes et d'uptime.\n▪ Si non utilisé : Le noyau n'est jamais réinitialisé à l'arrêt, accumulant des instabilités et empêchant l'accès disque en dual-boot.",
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
                "▪ Default: The WinSxS component store acts as the master repository for all Windows system packages and files.\n▪ If enabled: Runs DISM /Online /Cleanup-Image with Health checks, restores corrupted packages, and deep-cleans WinSxS.\n▪ If skipped: Corrupted packages in the store will prevent SFC repairs and block future cumulative updates.",
                "Official Microsoft recovery tool. Safe and highly effective against corrupted system packages.",
            ),
            Lang::Fr => (
                "DISM - Image Windows",
                "Répare le magasin de composants Windows en profondeur",
                "▪ Par défaut : Le magasin WinSxS constitue la source maîtresse de référence pour tous les fichiers et paquets système Windows.\n▪ Si utilisé : Exécute DISM /Online /Cleanup-Image (/ScanHealth, /RestoreHealth) et purge les versions obsolètes de WinSxS.\n▪ Si non utilisé : Les paquets corrompus du magasin empêcheront la réparation SFC et bloqueront les futures mises à jour Windows.",
                "Outil officiel Microsoft. Sûr et hautement recommandé en cas d'erreurs système ou de mises à jour bloquées.",
            ),
        },
        RepairTask::Sfc => match lang {
            Lang::En => (
                "SFC System Integrity",
                "Scans and fixes protected system files (sfc /scannow)",
                "▪ Default: Windows protected system files (DLLs, drivers, executables) ensure OS stability and security.\n▪ If enabled: Executes 'sfc /scannow' to verify checksums of all protected files and automatically restores corrupt files from cache.\n▪ If skipped: Damaged system binaries can trigger random crashes, blue screens (BSOD), or application errors.",
                "Safe official Windows tool. Does not affect your personal files or documents.",
            ),
            Lang::Fr => (
                "SFC - Fichiers système",
                "Analyse et répare l'intégrité des fichiers système protégés",
                "▪ Par défaut : Les fichiers système protégés (DLL, pilotes, exécutables) garantissent la stabilité et la sécurité du système d'exploitation.\n▪ Si utilisé : Lance « sfc /scannow » pour vérifier chaque fichier protégé et remplacer automatiquement les copies corrompues depuis le cache sain.\n▪ Si non utilisé : Des fichiers système altérés peuvent provoquer des plantages aléatoires, écrans bleus (BSOD) ou erreurs d'application.",
                "Outil officiel Microsoft entièrement sûr. Ne touche à aucun document ou fichier personnel.",
            ),
        },
        RepairTask::Chkdsk => match lang {
            Lang::En => (
                "CHKDSK Disk Check",
                "Checks NTFS filesystem metadata and schedules repairs",
                "▪ Default: The NTFS/ReFS file system maintains volume tables, indexes, security descriptors, and cluster allocations.\n▪ If enabled: Performs a safe read-only health check of drive C: and prompts to schedule chkdsk /F /R /X on restart if anomalies are found.\n▪ If skipped: Undetected disk surface corruption or cluster table damage can lead to silent data loss.",
                "Safe online scan. Offline repair will require a system reboot.",
            ),
            Lang::Fr => (
                "CHKDSK - Vérification disque",
                "Contrôle les métadonnées NTFS et planifie une réparation",
                "▪ Par défaut : Le système de fichiers NTFS gère les tables d'allocation, descripteurs de sécurité et métadonnées du volume C:.\n▪ Si utilisé : Effectue une vérification en ligne sans risque du disque C: et propose de programmer chkdsk /F /R /X au redémarrage en cas d'anomalie.\n▪ Si non utilisé : Des erreurs de clusters ou d'index non décelées risquent d'entraîner une perte silencieuse de données.",
                "Analyse en ligne sans risque. La réparation physique nécessite un redémarrage du PC.",
            ),
        },
        RepairTask::WindowsUpdate => match lang {
            Lang::En => (
                "Windows Update Reset",
                "Stops services, clears update cache & re-registers DLLs",
                "▪ Default: Windows Update downloads and queues patches via SoftwareDistribution, catroot2, and BITS services.\n▪ If enabled: Stops update services, resets corrupt download caches, re-registers 36 core update DLLs, and resets update policies.\n▪ If skipped: Corrupted update downloads will continue failing with recurrent 0x800... error codes.",
                "Safe. Clears stuck or corrupt downloads without removing already installed updates.",
            ),
            Lang::Fr => (
                "Windows Update Reset",
                "Purge le cache, réenregistre 36 DLLs et relance les services",
                "▪ Par défaut : Windows Update télécharge et gère les correctifs via SoftwareDistribution, catroot2 et le service BITS.\n▪ Si utilisé : Arrête les services, purge les caches corrompus, réenregistre 36 DLLs système et réinitialise les stratégies de mise à jour.\n▪ Si non utilisé : Les fichiers de mise à jour endommagés continueront d'échouer avec des erreurs récurrentes 0x800...",
                "Sûr. Résout les erreurs de téléchargement 0x800... sans désinstaller les mises à jour déjà appliquées.",
            ),
        },
        RepairTask::ResetNetwork => match lang {
            Lang::En => (
                "Network Stack Reset",
                "Flushes DNS, resets Winsock, TCP/IP & Windows Firewall",
                "▪ Default: Windows networking manages DNS resolution cache, Winsock catalog, TCP/IP routing tables, and firewall rules.\n▪ If enabled: Flushes DNS resolver cache, resets Winsock LSP catalog, resets TCP/IP stack to default, and restores firewall rules.\n▪ If skipped: Corrupt DNS records, rogue proxy settings, or polluted Winsock entries can maintain packet loss and connection drops.",
                "Safe. May momentarily drop internet connections. Custom static IP or proxy configurations will revert to DHCP defaults.",
            ),
            Lang::Fr => (
                "Reset Réseau & TCP/IP",
                "Vide le cache DNS, réinitialise Winsock, TCP/IP & Pare-feu",
                "▪ Par défaut : La pile réseau Windows gère le cache de résolution DNS, le catalogue Winsock, la table de routage TCP/IP et le pare-feu.\n▪ Si utilisé : Vide le cache DNS, réinitialise Winsock et la pile TCP/IP par défaut, purge le proxy WinHTTP et restaure les règles pare-feu.\n▪ Si non utilisé : Des caches DNS obsolètes ou des altérations du protocole Winsock peuvent perpétuer des coupures réseau inexpliquées.",
                "Sûr. Coupure réseau brève. Si vous utilisiez une IP fixe ou un proxy d'entreprise, ils seront réinitialisés en DHCP automatique.",
            ),
        },
        RepairTask::ClearTemp => match lang {
            Lang::En => (
                "Temp & Cache Cleanup",
                "Empties %TEMP%, Windows Temp, Prefetch & runs cleanmgr",
                "▪ Default: Applications and Windows installers accumulate temporary files, crash dumps, and obsolete prefetch logs over time.\n▪ If enabled: Safely wipes user %TEMP%, C:\\Windows\\Temp, stale Prefetch items, and triggers Windows Cleanmgr to free gigabytes.\n▪ If skipped: Gigabytes of obsolete clutter remain on your system drive, reducing free storage and degrading drive performance.",
                "Safe. Locked or actively running application files are automatically skipped.",
            ),
            Lang::Fr => (
                "Nettoyage Fichiers Temp",
                "Vide %TEMP%, C:\\Windows\\Temp, Prefetch & lance Cleanmgr",
                "▪ Par défaut : Les programmes et installations accumulent des journaux, installateurs temporaires et caches Prefetch obsolètes.\n▪ Si utilisé : Purge les dossiers %TEMP%, C:\\Windows\\Temp, les entrées Prefetch périmées et exécute Cleanmgr pour libérer plusieurs gigaoctets.\n▪ Si non utilisé : Les fichiers temporaires s'accumulent inutilement, saturant le disque système C: et dégradant les performances.",
                "Sûr. Les fichiers actuellement utilisés par des programmes ouverts sont automatiquement ignorés.",
            ),
        },
        RepairTask::WindowsStore => match lang {
            Lang::En => (
                "Microsoft Store Repair",
                "Runs wsreset and re-registers AppX packages",
                "▪ Default: Microsoft Store and modern UWP/AppX applications rely on local database caches and per-user package manifests.\n▪ If enabled: Clears the Store cache via wsreset.exe and re-registers all built-in AppX manifests across user profiles.\n▪ If skipped: Broken Store caches can cause native apps (Calculator, Photos, Settings, Store) to instantly crash upon launch.",
                "Safe. Fixes crashing or opening issues with built-in Windows applications.",
            ),
            Lang::Fr => (
                "Réparation Windows Store",
                "Exécute wsreset.exe et réenregistre les packages AppX",
                "▪ Par défaut : Le Microsoft Store et les applications UWP s'appuient sur un cache de données local et des manifestes de paquets AppX.\n▪ Si utilisé : Réinitialise le cache du Store via wsreset.exe et réenregistre tous les manifestes AppX natifs pour restaurer les composants.\n▪ Si non utilisé : Des corruptions du cache du Store peuvent faire crasher instantanément des applications comme Photos, Calculatrice ou Paramètres.",
                "Sûr. Résout les crashs au lancement du Store, Calculatrice, Photos ou Paramètres.",
            ),
        },
        RepairTask::CriticalServices => match lang {
            Lang::En => (
                "Critical Services Check",
                "Verifies and restores startup config for essential services",
                "▪ Default: Essential system services (BITS, CryptSvc, TrustedInstaller, Dhcp, Dnscache, Winmgmt) must run on exact startup types.\n▪ If enabled: Audits critical service configurations, restarts stalled services, and restores default startup modes (Automatic/Manual).\n▪ If skipped: Accidentally disabled core services will cause network failures, update errors, and software installation faults.",
                "Safe. Restores default Microsoft configuration for accidentally disabled core services.",
            ),
            Lang::Fr => (
                "Services Critiques",
                "Contrôle et rétablit la configuration des services vitaux",
                "▪ Par défaut : Les services Windows vitaux (BITS, CryptSvc, TrustedInstaller, Dhcp, Dnscache, Winmgmt) doivent tourner selon un type de démarrage précis.\n▪ Si utilisé : Contrôle et rétablit le type de démarrage conforme (Automatique/Manuel) de chaque service vital et relance ceux arrêtés.\n▪ Si non utilisé : Des services système indispensables désactivés par mégarde bloqueront les connexions réseau, l'installation de programmes ou les mises à jour.",
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
            assert!(desc_en.starts_with("▪ Default:"), "Desc EN format pour {}", id);
            assert!(desc_en.contains("\n▪ If enabled:"), "Desc EN format pour {}", id);
            assert!(desc_en.contains("\n▪ If skipped:"), "Desc EN format pour {}", id);

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
            assert!(desc_fr.starts_with("▪ Par défaut :"), "Desc FR format pour {}", id);
            assert!(desc_fr.contains("\n▪ Si utilisé :"), "Desc FR format pour {}", id);
            assert!(desc_fr.contains("\n▪ Si non utilisé :"), "Desc FR format pour {}", id);
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
            assert!(en_body.starts_with("▪ Default:"), "Body EN format pour {:?}", task);
            assert!(en_body.contains("\n▪ If enabled:"), "Body EN format pour {:?}", task);
            assert!(en_body.contains("\n▪ If skipped:"), "Body EN format pour {:?}", task);

            let (fr_short, fr_sub, fr_body, fr_safe) = repair_task_text(Lang::Fr, task);
            assert!(!fr_short.is_empty());
            assert!(!fr_sub.is_empty());
            assert!(!fr_body.is_empty());
            assert!(!fr_safe.is_empty());
            assert!(fr_body.starts_with("▪ Par défaut :"), "Body FR format pour {:?}", task);
            assert!(fr_body.contains("\n▪ Si utilisé :"), "Body FR format pour {:?}", task);
            assert!(fr_body.contains("\n▪ Si non utilisé :"), "Body FR format pour {:?}", task);
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
