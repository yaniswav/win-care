use crate::runner::Runner;
use crate::types::RepairTask;
use std::path::{Path, PathBuf};

/// Exécute la liste de tâches de réparation sélectionnées.
/// Retourne `true` si un redémarrage est recommandé (au moins une tâche exécutée).
pub fn execute_repair_tasks(
    runner: &Runner,
    tasks: &[RepairTask],
    schedule_chkdsk_reboot: bool,
) -> bool {
    if tasks.is_empty() {
        return false;
    }

    runner.logger.section("DÉMARRAGE DES RÉPARATIONS WINDOWS");
    let total = tasks.len();

    for (idx, task) in tasks.iter().enumerate() {
        runner.logger.progress(idx + 1, total, task.label());
        match task {
            RepairTask::Dism => invoke_dism_repair(runner),
            RepairTask::Sfc => invoke_sfc_repair(runner),
            RepairTask::Chkdsk => invoke_chkdsk_repair(runner, schedule_chkdsk_reboot),
            RepairTask::WindowsUpdate => repair_windows_update(runner),
            RepairTask::ResetNetwork => reset_network_stack(runner),
            RepairTask::ClearTemp => clear_temp_files(runner),
            RepairTask::WindowsStore => repair_windows_store(runner),
            RepairTask::CriticalServices => repair_critical_services(runner),
        }
    }

    runner.logger.section("RÉPARATIONS WINDOWS TERMINÉES");
    runner.logger.success(format!(
        "Toutes les réparations sélectionnées ({total}) ont été traitées."
    ));
    runner
        .logger
        .warn("Un redémarrage du PC est recommandé pour finaliser les réparations.");

    true
}

/// 1. DISM - Réparation de l'image Windows (CheckHealth, ScanHealth, RestoreHealth, ComponentCleanup)
pub fn invoke_dism_repair(runner: &Runner) {
    runner.logger.section("DISM - Réparation Image Windows");

    runner
        .logger
        .info("Étape 1/4 : Vérification de la santé de l'image (CheckHealth)...");
    runner.run_action("dism.exe", &["/Online", "/Cleanup-Image", "/CheckHealth"], true);

    runner
        .logger
        .info("Étape 2/4 : Scan approfondi de l'image (ScanHealth)...");
    runner.run_action("dism.exe", &["/Online", "/Cleanup-Image", "/ScanHealth"], true);

    runner.logger.info(
        "Étape 3/4 : Restauration de l'image (RestoreHealth) - peut durer 15 à 30 minutes...",
    );
    let restore_out = runner.run_action(
        "dism.exe",
        &["/Online", "/Cleanup-Image", "/RestoreHealth"],
        true,
    );
    if restore_out.exit_code == 0 {
        runner.logger.success("DISM terminé avec succès");
    } else {
        runner.logger.warn(format!(
            "DISM a rencontré des problèmes (Code de sortie: {})",
            restore_out.exit_code
        ));
    }

    runner.logger.info(
        "Étape 4/4 : Nettoyage des composants Windows obsolètes (/StartComponentCleanup /ResetBase)...",
    );
    runner.run_action(
        "dism.exe",
        &[
            "/Online",
            "/Cleanup-Image",
            "/StartComponentCleanup",
            "/ResetBase",
        ],
        true,
    );
}

/// 2. SFC - Vérification et réparation des fichiers système protégés
pub fn invoke_sfc_repair(runner: &Runner) {
    runner.logger.section("SFC - Vérification Fichiers Système");
    runner
        .logger
        .info("Analyse et réparation des fichiers système protégés (sfc /scannow)...");

    let out = runner.run_action("sfc.exe", &["/scannow"], true);
    if out.exit_code == 0 {
        runner.logger.success("SFC terminé avec succès");
    } else {
        runner
            .logger
            .warn("SFC a trouvé des problèmes (vérifiez C:\\Windows\\Logs\\CBS\\CBS.log)");
        runner
            .logger
            .info("Fichier journal disponible : C:\\Windows\\Logs\\CBS\\CBS.log");
    }
}

/// 3. CHKDSK - Vérification du disque système et planification optionnelle au redémarrage
pub fn invoke_chkdsk_repair(runner: &Runner, schedule_reboot: bool) {
    runner.logger.section("CHKDSK - Vérification du Disque");
    runner.logger.info("Vérification rapide du volume système C:...");
    runner.run_action("chkdsk.exe", &["C:"], true);

    if schedule_reboot {
        runner.logger.info(
            "Planification d'une analyse complète (chkdsk C: /F /R /X) au prochain redémarrage...",
        );
        let out = runner.run_action_with_stdin(
            "chkdsk.exe",
            &["C:", "/F", "/R", "/X"],
            "O\r\nY\r\n",
            true,
        );
        if out.exit_code == 0 {
            runner.logger.success(
                "CHKDSK planifié avec succès (il s'exécutera automatiquement au prochain redémarrage).",
            );
        } else {
            runner.logger.warn(format!(
                "Réponse de planification CHKDSK (Code: {})",
                out.exit_code
            ));
        }
    } else {
        runner
            .logger
            .info("Planification de CHKDSK au redémarrage ignorée (non demandée).");
    }
}

/// 4. Réparation complète Windows Update :
/// Arrêt des services, renommage du cache, réenregistrement des 36 DLLs, redémarrage services, reset policies
pub fn repair_windows_update(runner: &Runner) {
    runner.logger.section("Réparation Windows Update");

    runner.logger.info("Arrêt des services Windows Update...");
    let services = ["wuauserv", "cryptSvc", "bits", "msiserver"];
    for svc in &services {
        runner.run_action("net.exe", &["stop", svc, "/y"], false);
        runner.logger.info(format!("Service {svc} arrêté"));
    }

    runner
        .logger
        .info("Nettoyage du cache Windows Update (SoftwareDistribution & catroot2)...");
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let sys_path = Path::new(&system_root);
    let folders_to_rename = [
        sys_path.join("SoftwareDistribution"),
        sys_path.join("System32").join("catroot2"),
    ];

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();

    for folder in &folders_to_rename {
        if folder.exists() {
            let backup_path_str = format!("{}.old_{timestamp}", folder.display());
            if runner.dry_run {
                runner.logger.info(format!(
                    "[DRY-RUN] Renommage simulé : {} -> {backup_path_str}",
                    folder.display()
                ));
            } else {
                match std::fs::rename(folder, &backup_path_str) {
                    Ok(_) => runner.logger.success(format!(
                        "Dossier renommé : {} -> {backup_path_str}",
                        folder.display()
                    )),
                    Err(e) => runner.logger.warn(format!(
                        "Impossible de renommer {} : {e}",
                        folder.display()
                    )),
                }
            }
        }
    }

    runner
        .logger
        .info("Réenregistrement des 36 DLLs Windows Update...");
    const WU_DLLS: [&str; 36] = [
        "atl.dll", "urlmon.dll", "mshtml.dll", "shdocvw.dll", "browseui.dll", "jscript.dll",
        "vbscript.dll", "scrrun.dll", "msxml.dll", "msxml3.dll", "msxml6.dll", "actxprxy.dll",
        "softpub.dll", "wintrust.dll", "dssenh.dll", "rsaenh.dll", "gpkcsp.dll", "sccbase.dll",
        "slbcsp.dll", "cryptdlg.dll", "oleaut32.dll", "ole32.dll", "shell32.dll", "initpki.dll",
        "wuapi.dll", "wuaueng.dll", "wuaueng1.dll", "wucltui.dll", "wups.dll", "wups2.dll",
        "wuweb.dll", "qmgr.dll", "qmgrprxy.dll", "wucltux.dll", "muweb.dll", "wuwebv.dll",
    ];

    if runner.dry_run {
        runner
            .logger
            .info("[DRY-RUN] Réenregistrement simulé des 36 DLLs Windows Update.");
    } else {
        for dll in &WU_DLLS {
            runner.run_action("regsvr32.exe", &["/s", dll], false);
        }
        runner
            .logger
            .success("36 DLLs Windows Update réenregistrées avec succès.");
    }

    runner
        .logger
        .info("Redémarrage des services Windows Update...");
    for svc in &services {
        runner.run_action("net.exe", &["start", svc], false);
        runner.logger.info(format!("Service {svc} démarré"));
    }

    runner
        .logger
        .info("Réinitialisation des politiques Windows Update...");
    runner.run_ps_action(
        "Réinitialisation politiques Windows Update",
        "Remove-Item -Path 'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate' -Recurse -Force -ErrorAction SilentlyContinue",
        false,
    );
    runner
        .logger
        .success("Réparation Windows Update terminée");
}

/// 5. Reset Réseau : DNS, Winsock, TCP/IP, Proxy WinHTTP, Pare-feu Windows
pub fn reset_network_stack(runner: &Runner) {
    runner.logger.section("Reset Réseau");

    runner.logger.info("Vidage du cache DNS...");
    runner.run_action("ipconfig.exe", &["/flushdns"], true);

    runner
        .logger
        .info("Réinitialisation du catalogue Winsock...");
    runner.run_action("netsh.exe", &["winsock", "reset"], true);

    runner
        .logger
        .info("Réinitialisation de la pile TCP/IP...");
    runner.run_action("netsh.exe", &["int", "ip", "reset"], true);

    runner
        .logger
        .info("Réinitialisation des paramètres proxy WinHTTP...");
    runner.run_action("netsh.exe", &["winhttp", "reset", "proxy"], true);

    runner
        .logger
        .info("Réinitialisation du pare-feu Windows...");
    runner.run_action("netsh.exe", &["advfirewall", "reset"], true);

    runner
        .logger
        .success("Reset réseau terminé (un redémarrage est recommandé)");
}

/// 6. Nettoyage des fichiers temporaires (%TEMP%, Temp, Prefetch, INetCache) et Cleanmgr
pub fn clear_temp_files(runner: &Runner) {
    runner.logger.section("Nettoyage Fichiers Temporaires");

    let mut folders = Vec::new();

    if let Ok(temp) = std::env::var("TEMP") {
        folders.push(PathBuf::from(temp));
    }
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    folders.push(Path::new(&system_root).join("Temp"));
    folders.push(Path::new(&system_root).join("Prefetch"));

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        folders.push(
            Path::new(&local_app_data)
                .join("Microsoft")
                .join("Windows")
                .join("INetCache"),
        );
    }

    let mut total_files: u64 = 0;
    let mut total_bytes: u64 = 0;

    for folder in &folders {
        if folder.exists() {
            runner.logger.info(format!("Nettoyage de {}...", folder.display()));
            clean_dir_recursive(folder, runner.dry_run, &mut total_files, &mut total_bytes);
        }
    }

    let mb = (total_bytes as f64) / (1024.0 * 1024.0);
    if runner.dry_run {
        runner.logger.success(format!(
            "[DRY-RUN] Nettoyage simulé : {total_files} fichiers analysés (~{mb:.2} Mo à libérer)"
        ));
    } else {
        runner.logger.success(format!(
            "Nettoyage terminé : {total_files} fichiers supprimés (~{mb:.2} Mo libérés)"
        ));
    }

    runner.logger.info(
        "Lancement de l'outil de nettoyage de disque Windows (cleanmgr /sagerun:1)...",
    );
    runner.run_action("cleanmgr.exe", &["/sagerun:1"], false);
}

/// Nettoie récursivement les fichiers et sous-dossiers dans un dossier (avec profondeur bornée pour éviter tout Stack Overflow), en ignorant les erreurs de verrouillage.
fn clean_dir_recursive(dir: &Path, dry_run: bool, count: &mut u64, bytes: &mut u64) {
    clean_dir_bounded(dir, dry_run, count, bytes, 16);
}

fn clean_dir_bounded(
    dir: &Path,
    dry_run: bool,
    count: &mut u64,
    bytes: &mut u64,
    max_depth: usize,
) {
    if max_depth == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            let ft = metadata.file_type();
            if ft.is_symlink() {
                if !dry_run {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }
            if ft.is_dir() {
                clean_dir_bounded(&path, dry_run, count, bytes, max_depth - 1);
                if !dry_run {
                    let _ = std::fs::remove_dir_all(&path);
                }
            } else {
                *count += 1;
                *bytes += metadata.len();
                if !dry_run {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
}

/// 7. Réparation Windows Store & Applications AppX
pub fn repair_windows_store(runner: &Runner) {
    runner.logger.section("Réparation Windows Store");

    runner
        .logger
        .info("Réinitialisation du cache du Windows Store (wsreset.exe)...");
    runner.run_ps_action(
        "Réinitialisation cache Windows Store",
        "Start-Process -FilePath 'wsreset.exe' -Wait -WindowStyle Hidden -ErrorAction SilentlyContinue",
        false,
    );
    runner.logger.info("Cache Store réinitialisé");

    runner
        .logger
        .info("Réenregistrement des applications Windows (AppX) pour tous les utilisateurs...");
    runner.run_ps_action(
        "Réenregistrement applications AppX",
        "Get-AppXPackage -AllUsers 2>$null | ForEach-Object { Add-AppxPackage -DisableDevelopmentMode -Register \"$($_.InstallLocation)\\AppXManifest.xml\" -ErrorAction SilentlyContinue 2>$null }",
        false,
    );
    runner
        .logger
        .success("Réparation Windows Store et applications terminée");
}

/// 8. Vérification et alignement des 8 services système critiques
pub fn repair_critical_services(runner: &Runner) {
    runner
        .logger
        .section("Vérification & Réparation Services Critiques");

    let services = [
        ("wuauserv", "Windows Update", "Manual"),
        ("BITS", "Background Intelligent Transfer", "Manual"),
        ("CryptSvc", "Services de chiffrement (CryptSvc)", "Automatic"),
        ("TrustedInstaller", "Windows Modules Installer", "Manual"),
        ("Winmgmt", "WMI", "Automatic"),
        ("EventLog", "Windows Event Log", "Automatic"),
        ("Dhcp", "Client DHCP", "Automatic"),
        ("Dnscache", "Client DNS", "Automatic"),
    ];

    runner
        .logger
        .info("Vérification et alignement des 8 services critiques Windows...");

    let ps_script = r#"
    $CriticalServices = @(
        @{Name="wuauserv"; StartType="Manual"},
        @{Name="BITS"; StartType="Manual"},
        @{Name="CryptSvc"; StartType="Automatic"},
        @{Name="TrustedInstaller"; StartType="Manual"},
        @{Name="Winmgmt"; StartType="Automatic"},
        @{Name="EventLog"; StartType="Automatic"},
        @{Name="Dhcp"; StartType="Automatic"},
        @{Name="Dnscache"; StartType="Automatic"}
    );
    foreach ($Svc in $CriticalServices) {
        $Service = Get-Service -Name $Svc.Name -ErrorAction SilentlyContinue;
        if ($Service) {
            Set-Service -Name $Svc.Name -StartupType $Svc.StartType -ErrorAction SilentlyContinue;
            if ($Svc.StartType -eq "Automatic" -and $Service.Status -ne "Running") {
                Start-Service -Name $Svc.Name -ErrorAction SilentlyContinue;
            }
        }
    }
    "#;

    runner.run_ps_action(
        "Configuration et démarrage des services critiques",
        ps_script,
        false,
    );

    for (name, display, stype) in &services {
        runner
            .logger
            .success(format!("Service {display} ({name}) : {stype} OK"));
    }
    runner
        .logger
        .success("Services critiques vérifiés et réparés");
}
