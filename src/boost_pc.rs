use crate::runner::Runner;
use crate::types::{
    CacheSizeInfo, DiskVolumeInfo, MetricDiff, PcCleanOptions, PcDiagnosis, PcOptimOptions,
    ProcessInfo, ServiceRamInfo, StartupItem, TempSubfolderInfo,
};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use sysinfo::{DiskKind, Disks, System};
use winreg::enums::{RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::{RegKey, RegValue, HKEY};

#[derive(Debug, Default, Deserialize)]
struct PsExtraDiag {
    #[serde(default)]
    model: String,
    #[serde(default)]
    ssd_present: bool,
    #[serde(default)]
    #[serde(deserialize_with = "crate::types::deserialize_vec_or_one")]
    services: Vec<PsServiceRam>,
    #[serde(default)]
    tasks_count: usize,
}

#[derive(Debug, Default, Deserialize)]
struct PsServiceRam {
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    ram_mb: u64,
}

/// Retourne la liste des 9 dossiers de caches surveillés par Boost-PC.
fn get_cache_targets() -> Vec<(&'static str, String)> {
    let user_temp = std::env::var("TEMP").unwrap_or_else(|_| String::from(r"C:\Windows\Temp"));
    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| String::from(r"C:\Windows"));
    let prog_data =
        std::env::var("ProgramData").unwrap_or_else(|_| String::from(r"C:\ProgramData"));
    let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        std::env::var("USERPROFILE")
            .map(|p| format!(r"{p}\AppData\Local"))
            .unwrap_or_else(|_| String::from(r"C:\Users\Default\AppData\Local"))
    });
    let sys_drive = std::env::var("SystemDrive").unwrap_or_else(|_| String::from("C:"));

    vec![
        ("TEMP utilisateur", user_temp),
        ("TEMP système", format!(r"{sys_root}\Temp")),
        (
            "Windows Update DL",
            format!(r"{sys_root}\SoftwareDistribution\Download"),
        ),
        ("Prefetch", format!(r"{sys_root}\Prefetch")),
        (
            "WER ReportQueue",
            format!(r"{prog_data}\Microsoft\Windows\WER\ReportQueue"),
        ),
        (
            "WER ReportArchive",
            format!(r"{prog_data}\Microsoft\Windows\WER\ReportArchive"),
        ),
        ("Crash Dumps", format!(r"{local_appdata}\CrashDumps")),
        ("Recycle Bin", format!(r"{sys_drive}\$Recycle.Bin")),
        (
            "Delivery Optim.",
            format!(r"{sys_root}\SoftwareDistribution\DeliveryOptimization"),
        ),
    ]
}

/// Calcule récursivement la taille d'un dossier en octets avec une limite de profondeur.
fn folder_size_bytes(path: &Path, max_depth: usize) -> u64 {
    if max_depth == 0 || !path.exists() {
        return 0;
    }
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    let mut total: u64 = 0;
    for entry in entries.flatten() {
        if let Ok(ft) = entry.file_type() {
            if ft.is_symlink() {
                continue;
            }
            if ft.is_file() {
                if let Ok(meta) = entry.metadata() {
                    total = total.saturating_add(meta.len());
                }
            } else if ft.is_dir() {
                total = total.saturating_add(folder_size_bytes(&entry.path(), max_depth - 1));
            }
        }
    }
    total
}

/// Calcule la taille en Mo de tous les dossiers de caches.
fn measure_caches() -> (Vec<CacheSizeInfo>, u64) {
    let mut caches = Vec::new();
    let mut total_mb = 0u64;

    for (label, path_str) in get_cache_targets() {
        let bytes = folder_size_bytes(Path::new(&path_str), 16);
        let size_mb = (bytes + 524_288) / 1_048_576;
        total_mb = total_mb.saturating_add(size_mb);
        caches.push(CacheSizeInfo {
            label: label.to_string(),
            path: path_str,
            size_mb,
        });
    }

    caches.sort_by(|a, b| b.size_mb.cmp(&a.size_mb));
    (caches, total_mb)
}

/// Lit le modèle de la machine dans le registre Windows en repli rapide.
fn read_machine_model_from_registry() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let bios = hklm
        .open_subkey_with_flags(r"HARDWARE\DESCRIPTION\System\BIOS", KEY_READ)
        .ok()?;
    let mfg: String = bios.get_value("SystemManufacturer").unwrap_or_default();
    let prod: String = bios.get_value("SystemProductName").unwrap_or_default();
    let combined = format!("{} {}", mfg.trim(), prod.trim()).trim().to_string();
    if combined.is_empty() {
        None
    } else {
        Some(combined)
    }
}

/// Exécute le diagnostic complet du PC (équivalent section 1 de `Boost-PC.ps1`).
pub fn run_pc_diagnosis(runner: &Runner) -> PcDiagnosis {
    runner.logger.section("1. ANALYSE DU PC (Boost-PC)");
    runner
        .logger
        .progress(1, 5, "Analyse matériel, CPU, RAM et processus...");

    let mut sys = System::new_all();
    thread::sleep(Duration::from_millis(250));
    sys.refresh_cpu_all();

    let ram_total_mb = sys.total_memory() / (1024 * 1024);
    let ram_free_mb = sys.available_memory() / (1024 * 1024);
    let ram_used_pct = if ram_total_mb > 0 {
        let used = ram_total_mb.saturating_sub(ram_free_mb);
        ((used as f64 / ram_total_mb as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };

    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| String::from("Processeur Windows"));
    let cpu_cores = sys
        .physical_core_count()
        .unwrap_or_else(|| sys.cpus().len());
    let cpu_threads = sys.cpus().len();
    let cpu_usage_pct = if !sys.cpus().is_empty() {
        let sum: f32 = sys.cpus().iter().map(|c| c.cpu_usage()).sum();
        ((sum as f64 / sys.cpus().len() as f64) * 10.0).round() / 10.0
    } else {
        0.0
    };

    let uptime_days = ((System::uptime() as f64 / 86400.0) * 10.0).round() / 10.0;
    let os_caption = System::long_os_version().unwrap_or_else(|| String::from("Windows"));

    // Top 10 processus par RAM décroissante
    let mut top_processes: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .map(|(pid, proc_info)| ProcessInfo {
            pid: pid.as_u32(),
            name: proc_info.name().to_string_lossy().into_owned(),
            ram_mb: proc_info.memory() / (1024 * 1024),
            cpu_pct: (proc_info.cpu_usage() * 10.0).round() / 10.0,
        })
        .collect();
    top_processes.sort_by(|a, b| b.ram_mb.cmp(&a.ram_mb));
    top_processes.truncate(10);

    runner
        .logger
        .progress(2, 5, "Analyse des disques et volumes...");

    // Volumes et disques fixes
    let sys_drive_env = std::env::var("SystemDrive")
        .unwrap_or_else(|_| String::from("C:"))
        .to_uppercase();
    let disks_list = Disks::new_with_refreshed_list();
    let mut disks = Vec::new();
    let mut ssd_detected = false;
    let mut sys_free_gb = 0.0;

    for d in disks_list.list() {
        if d.is_removable() || d.total_space() == 0 {
            continue;
        }
        let mount = d.mount_point().to_string_lossy().to_string();
        let drive = mount.trim_end_matches('\\').to_string();
        let fs = d.file_system().to_string_lossy().into_owned();
        let size_gb = ((d.total_space() as f64 / 1_073_741_824.0) * 100.0).round() / 100.0;
        let free_gb = ((d.available_space() as f64 / 1_073_741_824.0) * 100.0).round() / 100.0;
        let free_pct =
            ((d.available_space() as f64 / d.total_space() as f64) * 1000.0).round() / 10.0;

        let media_type = match d.kind() {
            DiskKind::SSD => {
                ssd_detected = true;
                String::from("SSD")
            }
            DiskKind::HDD => String::from("HDD"),
            _ => String::from("Fixe"),
        };

        let health = if free_pct < 10.0 {
            String::from("Attention (<10% libre)")
        } else {
            String::from("Sain")
        };

        if drive.eq_ignore_ascii_case(&sys_drive_env) {
            sys_free_gb = free_gb;
            if free_pct < 10.0 {
                runner.logger.warn(format!(
                    "ALERTE : disque système ({drive}) à {free_pct}% libre ({free_gb} Go), impact fort sur les performances."
                ));
            }
        }

        disks.push(DiskVolumeInfo {
            drive,
            fs,
            size_gb,
            free_gb,
            free_pct,
            media_type,
            health,
        });
    }

    runner.logger.progress(
        3,
        5,
        "Lecture des programmes au démarrage (Registre & Startup)...",
    );
    let startup_items = list_startup_items();

    runner.logger.progress(
        4,
        5,
        "Analyse des services Auto et tâches planifiées...",
    );

    let ps_script = r#"
$cs = Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue
$model = if ($cs) { ("{0} {1}" -f $cs.Manufacturer, $cs.Model).Trim() } else { "" }
$ssd = $false
try { $ssd = [bool]((Get-PhysicalDisk -ErrorAction SilentlyContinue).MediaType -contains 'SSD') } catch {}
$svc = Get-CimInstance Win32_Service -ErrorAction SilentlyContinue | Where-Object { $_.StartMode -eq 'Auto' -and $_.State -eq 'Running' -and $_.ProcessId -gt 0 }
$svcList = @(foreach ($s in $svc) {
    $p = Get-Process -Id $s.ProcessId -ErrorAction SilentlyContinue
    if ($p) {
        $dn = [string]$s.DisplayName
        if ($dn.Length -gt 43) { $dn = $dn.Substring(0, 40) + '...' }
        [pscustomobject]@{
            name = [string]$s.Name
            display_name = $dn
            ram_mb = [uint64][math]::Round($p.WorkingSet64 / 1MB, 0)
        }
    }
}) | Sort-Object ram_mb -Descending | Select-Object -First 15
$tasksCount = @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.State -eq 'Ready' -and $_.TaskPath -notlike '\Microsoft\*' }).Count
[pscustomobject]@{
    model = $model
    ssd_present = $ssd
    services = @($svcList)
    tasks_count = [int]$tasksCount
} | ConvertTo-Json -Compress -Depth 4
"#;

    let ps_out = runner.run_ps_query(ps_script);
    let extra: PsExtraDiag = serde_json::from_str(ps_out.stdout.trim()).unwrap_or_default();

    let machine_model = if !extra.model.is_empty() {
        extra.model
    } else {
        read_machine_model_from_registry().unwrap_or_else(|| String::from("PC Windows"))
    };

    if extra.ssd_present {
        ssd_detected = true;
        for d in &mut disks {
            if d.media_type == "Fixe" {
                d.media_type = String::from("SSD");
            }
        }
    }

    let auto_services: Vec<ServiceRamInfo> = extra
        .services
        .into_iter()
        .map(|s| ServiceRamInfo {
            name: s.name,
            display_name: s.display_name,
            ram_mb: s.ram_mb,
        })
        .collect();
    let scheduled_tasks_count = extra.tasks_count;

    runner
        .logger
        .progress(5, 5, "Calcul de la taille des 9 dossiers de caches...");
    let (caches, cache_total_mb) = measure_caches();

    let enabled_startup = startup_items.iter().filter(|i| i.enabled).count();
    let disabled_startup = startup_items.len().saturating_sub(enabled_startup);

    runner.logger.info(format!(
        "Machine : {machine_model} | OS : {os_caption} | Uptime : {uptime_days} j"
    ));
    runner.logger.info(format!(
        "CPU : {cpu_name} ({cpu_cores} cœurs / {cpu_threads} threads) | Charge : {cpu_usage_pct}%"
    ));
    runner.logger.info(format!(
        "RAM : {ram_used_pct}% utilisée ({ram_free_mb} Mo libres / {ram_total_mb} Mo)"
    ));
    runner.logger.info(format!(
        "Disque système libre : {sys_free_gb} Go | SSD détecté : {}",
        if ssd_detected { "Oui" } else { "Non" }
    ));
    runner.logger.info(format!(
        "Démarrage : {} éléments ({enabled_startup} actifs, {disabled_startup} désactivés) | Tâches non-Microsoft : {scheduled_tasks_count}",
        startup_items.len()
    ));
    runner
        .logger
        .info(format!("Caches cumulés : {cache_total_mb} Mo"));

    PcDiagnosis {
        machine_model,
        os_caption,
        uptime_days,
        cpu_name,
        cpu_cores,
        cpu_threads,
        cpu_usage_pct,
        ram_total_mb,
        ram_free_mb,
        ram_used_pct,
        sys_free_gb,
        ssd_detected,
        disks,
        top_processes,
        startup_items,
        auto_services,
        scheduled_tasks_count,
        caches,
        cache_total_mb,
    }
}

/// Mappe la source d'une entrée de démarrage vers sa clé `StartupApproved` correspondante.
fn get_startup_approved_target(source: &str) -> Option<(HKEY, &'static str)> {
    if source.starts_with(r"HKLM\Microsoft\Windows\CurrentVersion\Run") {
        Some((
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
        ))
    } else if source.starts_with(r"HKCU\Microsoft\Windows\CurrentVersion\Run") {
        Some((
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
        ))
    } else if source.starts_with(r"HKLM\WOW6432Node") {
        Some((
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32",
        ))
    } else if source == "Startup folder" {
        Some((
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder",
        ))
    } else if source == "Startup folder (Common)" {
        Some((
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder",
        ))
    } else {
        None
    }
}

/// Vérifie si une entrée de démarrage est active (`true`) ou désactivée (`false` si octet 0 == 0x03).
fn get_startup_status(source: &str, name: &str) -> bool {
    let mut candidates = Vec::new();
    if let Some(primary) = get_startup_approved_target(source) {
        candidates.push(primary);
    }
    candidates.push((
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder",
    ));

    for (hive, subkey_path) in candidates {
        let root = RegKey::predef(hive);
        if let Ok(key) = root.open_subkey_with_flags(subkey_path, KEY_READ) {
            if let Ok(raw) = key.get_raw_value(name) {
                if let Some(&first_byte) = raw.bytes.first() {
                    if first_byte == 0x03 {
                        return false;
                    }
                    if first_byte == 0x02 || first_byte == 0x06 {
                        return true;
                    }
                }
            }
        }
    }
    true
}

/// Liste toutes les applications configurées au démarrage (Registre Run/RunOnce + dossiers Startup).
pub fn list_startup_items() -> Vec<StartupItem> {
    let mut items = Vec::new();

    let reg_sources: [(HKEY, &str, &str); 5] = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            r"HKLM\Microsoft\Windows\CurrentVersion\Run",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
            r"HKLM\Microsoft\Windows\CurrentVersion\RunOnce",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            r"HKCU\Microsoft\Windows\CurrentVersion\Run",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
            r"HKCU\Microsoft\Windows\CurrentVersion\RunOnce",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
            r"HKLM\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        ),
    ];

    for (hive, subkey_path, source_label) in reg_sources {
        let root = RegKey::predef(hive);
        if let Ok(key) = root.open_subkey_with_flags(subkey_path, KEY_READ) {
            for value_res in key.enum_values().flatten() {
                let (name, reg_val) = value_res;
                if name.is_empty() {
                    continue;
                }
                let command = reg_val.to_string();
                let enabled = get_startup_status(source_label, &name);
                items.push(StartupItem {
                    source: source_label.to_string(),
                    name,
                    enabled,
                    command,
                });
            }
        }
    }

    let mut startup_dirs: Vec<(String, &'static str)> = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        startup_dirs.push((
            format!(r"{appdata}\Microsoft\Windows\Start Menu\Programs\Startup"),
            "Startup folder",
        ));
    }
    if let Ok(progdata) = std::env::var("ProgramData") {
        startup_dirs.push((
            format!(r"{progdata}\Microsoft\Windows\Start Menu\Programs\Startup"),
            "Startup folder (Common)",
        ));
    }

    for (dir_str, source_label) in startup_dirs {
        let dir_path = Path::new(&dir_str);
        if let Ok(entries) = fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_file() {
                        let file_name = entry.file_name().to_string_lossy().into_owned();
                        if file_name.eq_ignore_ascii_case("desktop.ini") {
                            continue;
                        }
                        let full_path = entry.path().to_string_lossy().into_owned();
                        let enabled = get_startup_status(source_label, &file_name);
                        items.push(StartupItem {
                            source: String::from(source_label),
                            name: file_name,
                            enabled,
                            command: full_path,
                        });
                    }
                }
            }
        }
    }

    // Tri : Actifs d'abord, puis par nom
    items.sort_by(|a, b| {
        b.enabled
            .cmp(&a.enabled)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    items
}

/// Active ou désactive un élément de démarrage via la clé `StartupApproved` du Gestionnaire des tâches.
pub fn toggle_startup_item(runner: &Runner, item: &StartupItem, enable: bool) -> bool {
    let Some((hive, subkey_path)) = get_startup_approved_target(&item.source) else {
        runner.logger.warn(format!(
            "Source de démarrage non prise en charge pour '{}' : {}",
            item.name, item.source
        ));
        return false;
    };

    let state_label = if enable { "Activé" } else { "Désactivé" };

    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Bascule démarrage '{}' ({}) -> {}",
            item.name, item.source, state_label
        ));
        return true;
    }

    let root = RegKey::predef(hive);
    let (key, _) = match root.create_subkey(subkey_path) {
        Ok(k) => k,
        Err(e) => {
            runner.logger.error(format!(
                "Impossible d'ouvrir la clé StartupApproved ({subkey_path}) pour '{}' : {e}",
                item.name
            ));
            return false;
        }
    };

    let mut bytes = vec![0u8; 12];
    bytes[0] = if enable { 0x02 } else { 0x03 };
    let val = RegValue {
        bytes,
        vtype: RegType::REG_BINARY,
    };

    match key.set_raw_value(&item.name, &val) {
        Ok(()) => {
            runner.logger.success(format!(
                "Élément de démarrage '{}' ({}) marqué comme {state_label}.",
                item.name, item.source
            ));
            true
        }
        Err(e) => {
            runner.logger.error(format!(
                "Échec de modification StartupApproved pour '{}' : {e}",
                item.name
            ));
            false
        }
    }
}

/// Vide le contenu d'un dossier sans supprimer le dossier racine.
fn clear_directory_contents(runner: &Runner, label: &str, dir_path: &str) {
    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Nettoyage du dossier {label} ({dir_path})"
        ));
        return;
    }

    let path = Path::new(dir_path);
    if !path.exists() {
        runner
            .logger
            .info(format!("Dossier {label} ({dir_path}) absent, ignoré."));
        return;
    }

    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => {
            runner
                .logger
                .warn(format!("Impossible de lire {label} ({dir_path}) : {e}"));
            return;
        }
    };

    let mut removed = 0usize;
    let mut skipped = 0usize;

    for entry in entries.flatten() {
        let entry_path = entry.path();
        let res = if entry_path.is_dir() {
            fs::remove_dir_all(&entry_path)
        } else {
            fs::remove_file(&entry_path)
        };
        if res.is_ok() {
            removed += 1;
        } else {
            skipped += 1;
        }
    }

    runner.logger.success(format!(
        "{label} nettoyé ({removed} éléments supprimés, {skipped} verrouillés/ignorés)."
    ));
}

/// Exécute les étapes de nettoyage sélectionnées (équivalent section 2 de `Boost-PC.ps1`).
pub fn run_pc_cleaning(runner: &Runner, opts: &PcCleanOptions) {
    runner.logger.section("2. NETTOYAGE DU PC (Boost-PC)");

    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| String::from(r"C:\Windows"));
    let prog_data =
        std::env::var("ProgramData").unwrap_or_else(|_| String::from(r"C:\ProgramData"));
    let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        std::env::var("USERPROFILE")
            .map(|p| format!(r"{p}\AppData\Local"))
            .unwrap_or_else(|_| String::from(r"C:\Users\Default\AppData\Local"))
    });

    // 2a. TEMP utilisateur
    if opts.clean_user_temp {
        runner.logger.progress(1, 7, "Nettoyage de %TEMP%...");
        let user_temp =
            std::env::var("TEMP").unwrap_or_else(|_| format!(r"{local_appdata}\Temp"));
        clear_directory_contents(runner, "TEMP utilisateur", &user_temp);
    }

    // 2b. TEMP système
    if opts.clean_sys_temp {
        runner
            .logger
            .progress(2, 7, "Nettoyage de C:\\Windows\\Temp...");
        let sys_temp = format!(r"{sys_root}\Temp");
        clear_directory_contents(runner, "TEMP système", &sys_temp);
    }

    // 2c. Cache Windows Update
    if opts.clean_wu_download {
        runner.logger.progress(
            3,
            7,
            "Nettoyage du cache Windows Update (SoftwareDistribution\\Download)...",
        );
        runner
            .logger
            .info("Arrêt temporaire des services wuauserv et bits...");
        let _ = runner.run_action("net.exe", &["stop", "wuauserv", "/y"], false);
        let _ = runner.run_action("net.exe", &["stop", "bits", "/y"], false);

        let wu_dl = format!(r"{sys_root}\SoftwareDistribution\Download");
        clear_directory_contents(runner, "Windows Update Download", &wu_dl);

        runner
            .logger
            .info("Redémarrage des services wuauserv et bits...");
        let _ = runner.run_action("net.exe", &["start", "wuauserv"], false);
        let _ = runner.run_action("net.exe", &["start", "bits"], false);
    }

    // 2d. WER + CrashDumps
    if opts.clean_wer_dumps {
        runner.logger.progress(
            4,
            7,
            "Suppression des rapports d'erreur (WER) et CrashDumps...",
        );
        let wer_queue = format!(r"{prog_data}\Microsoft\Windows\WER\ReportQueue");
        let wer_archive = format!(r"{prog_data}\Microsoft\Windows\WER\ReportArchive");
        let crash_dumps = format!(r"{local_appdata}\CrashDumps");

        clear_directory_contents(runner, "WER ReportQueue", &wer_queue);
        clear_directory_contents(runner, "WER ReportArchive", &wer_archive);
        clear_directory_contents(runner, "Crash Dumps", &crash_dumps);
    }

    // 2e. Corbeille
    if opts.empty_recycle_bin {
        runner
            .logger
            .progress(5, 7, "Vidage de la corbeille sur tous les disques...");
        let out = runner.run_ps_action(
            "Vider la corbeille (Clear-RecycleBin -Force)",
            "Clear-RecycleBin -Force -ErrorAction SilentlyContinue",
            false,
        );
        if out.success() {
            runner.logger.success("Corbeille vidée avec succès.");
        }
    }

    // 2f. DISM StartComponentCleanup
    if opts.dism_component_cleanup {
        runner.logger.progress(
            6,
            7,
            "Nettoyage WinSxS via DISM (/StartComponentCleanup /ResetBase)...",
        );
        runner.logger.info(
            "Exécution de dism.exe /Online /Cleanup-Image /StartComponentCleanup /ResetBase...",
        );
        let out = runner.run_action(
            "dism.exe",
            &[
                "/Online",
                "/Cleanup-Image",
                "/StartComponentCleanup",
                "/ResetBase",
            ],
            true,
        );
        if out.success() {
            runner
                .logger
                .success("Nettoyage des composants WinSxS terminé.");
        } else {
            runner.logger.warn(format!(
                "DISM StartComponentCleanup terminé avec le code {}.",
                out.exit_code
            ));
        }
    }

    // 2g. Cleanmgr automatisé
    if opts.run_cleanmgr {
        runner.logger.progress(
            7,
            7,
            "Exécution du nettoyage de disque Windows (cleanmgr /sagerun:1)...",
        );
        let volume_cache_keys = [
            "Active Setup Temp Folders",
            "BranchCache",
            "Downloaded Program Files",
            "Internet Cache Files",
            "Memory Dump Files",
            "Old ChkDsk Files",
            "Previous Installations",
            "Recycle Bin",
            "Service Pack Cleanup",
            "Setup Log Files",
            "System error memory dump files",
            "System error minidump files",
            "Temporary Files",
            "Temporary Setup Files",
            "Thumbnail Cache",
            "Update Cleanup",
            "Upgrade Discarded Files",
            "Windows Error Reporting Files",
            "Windows ESD installation files",
            "Windows Upgrade Log Files",
        ];

        if runner.dry_run {
            runner.logger.info(
                "[DRY-RUN] Configuration de StateFlags0001 = 2 dans HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VolumeCaches\\*",
            );
        } else {
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            let base_path =
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches";
            for sub in volume_cache_keys {
                let full = format!(r"{base_path}\{sub}");
                if let Ok((k, _)) = hklm.create_subkey(&full) {
                    let _ = k.set_value("StateFlags0001", &2u32);
                }
            }
        }

        let out = runner.run_action("cleanmgr.exe", &["/sagerun:1"], false);
        if out.success() {
            runner
                .logger
                .success("Nettoyage de disque Windows (cleanmgr) terminé.");
        }
    }
}

/// Helper d'écriture DWORD dans HKCU avec support dry-run.
fn set_hkcu_dword(runner: &Runner, subkey: &str, name: &str, value: u32) -> bool {
    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Registre HKCU\\{subkey} : {name} = {value} (DWORD)"
        ));
        return true;
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.create_subkey(subkey) {
        Ok((key, _)) => match key.set_value(name, &value) {
            Ok(()) => true,
            Err(e) => {
                runner.logger.warn(format!(
                    "Impossible d'écrire HKCU\\{subkey}\\{name} : {e}"
                ));
                false
            }
        },
        Err(e) => {
            runner
                .logger
                .warn(format!("Impossible d'ouvrir HKCU\\{subkey} : {e}"));
            false
        }
    }
}

/// Exécute les optimisations système sélectionnées (équivalent section 3 de `Boost-PC.ps1`).
pub fn run_pc_optimizations(runner: &Runner, opts: &PcOptimOptions, diag: &PcDiagnosis) {
    runner.logger.section("3. OPTIMISATION DU PC (Boost-PC)");

    // 3a. Plan d'alimentation Performances élevées
    if opts.high_perf_power {
        runner.logger.progress(
            1,
            8,
            "Activation du plan d'alimentation Performances élevées...",
        );
        let out = runner.run_action("powercfg.exe", &["/setactive", "SCHEME_MIN"], false);
        if out.success() {
            runner
                .logger
                .success("Plan d'alimentation 'Performances élevées' (SCHEME_MIN) activé.");
        }
    }

    // 3a-bis. Plan Ultimate Performance
    if opts.ultimate_perf_power {
        runner.logger.progress(
            2,
            8,
            "Activation du plan d'alimentation Ultimate Performance...",
        );
        crate::gaming::enable_ultimate_power_plan(runner);
    }

    // 3b. Effets visuels
    if opts.reduce_visual_fx {
        runner.logger.progress(
            3,
            8,
            "Réduction des effets visuels (mode meilleures performances)...",
        );
        if set_hkcu_dword(
            runner,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects",
            "VisualFXSetting",
            2,
        ) {
            runner
                .logger
                .success("Effets visuels réglés sur 'Meilleures performances' (VisualFXSetting=2).");
        }
    }

    // 3c. Storage Sense
    if opts.enable_storage_sense {
        runner
            .logger
            .progress(4, 8, "Activation de Storage Sense...");
        let policy_key =
            r"Software\Microsoft\Windows\CurrentVersion\StorageSense\Parameters\StoragePolicy";
        let ok = set_hkcu_dword(runner, policy_key, "01", 1)
            && set_hkcu_dword(runner, policy_key, "04", 1)
            && set_hkcu_dword(runner, policy_key, "08", 1)
            && set_hkcu_dword(runner, policy_key, "32", 30)
            && set_hkcu_dword(runner, policy_key, "128", 30)
            && set_hkcu_dword(runner, policy_key, "256", 30);
        if ok {
            runner
                .logger
                .success("Storage Sense (nettoyage automatique périodique) activé.");
        }
    }

    // 3e. Xbox Game Bar / GameDVR
    if opts.disable_game_bar {
        runner
            .logger
            .progress(5, 8, "Désactivation de la Xbox Game Bar & GameDVR...");
        let ok = set_hkcu_dword(runner, r"Software\Microsoft\GameBar", "AllowAutoGameMode", 0)
            && set_hkcu_dword(
                runner,
                r"Software\Microsoft\GameBar",
                "AutoGameModeEnabled",
                0,
            )
            && set_hkcu_dword(runner, r"System\GameConfigStore", "GameDVR_Enabled", 0);
        if ok {
            runner
                .logger
                .success("Xbox Game Bar et GameDVR désactivés.");
        }
    }

    // 3f. Transparence Windows
    if opts.disable_transparency {
        runner
            .logger
            .progress(6, 8, "Désactivation de la transparence Windows...");
        if set_hkcu_dword(
            runner,
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "EnableTransparency",
            0,
        ) {
            runner.logger.success("Transparence Windows désactivée.");
        }
    }

    // 3g. SysMain (Superfetch) : seulement si SSD détecté ET RAM >= 7500 Mo (~8 Go)
    if opts.disable_sysmain {
        runner
            .logger
            .progress(7, 8, "Vérification et désactivation de SysMain...");
        if diag.ssd_detected && diag.ram_total_mb >= 7500 {
            let out = runner.run_ps_action(
                "Arrêt et désactivation du service SysMain",
                "Stop-Service SysMain -Force -ErrorAction SilentlyContinue; Set-Service SysMain -StartupType Disabled -ErrorAction SilentlyContinue",
                false,
            );
            if out.success() {
                runner
                    .logger
                    .success("Service SysMain (Superfetch) arrêté et désactivé (SSD + RAM >= 8 Go).");
            }
        } else {
            runner.logger.info(format!(
                "SysMain conservé actif (SSD détecté: {}, RAM totale: {} Mo).",
                diag.ssd_detected, diag.ram_total_mb
            ));
        }
    }

    // 3h. Optimize-Volume (TRIM sur SSD / Defrag sur HDD)
    if opts.optimize_volume_trim {
        runner
            .logger
            .progress(8, 8, "Optimisation du volume C: (TRIM / Défragmentation)...");
        let out = runner.run_ps_action(
            "Optimize-Volume -DriveLetter C",
            "Optimize-Volume -DriveLetter C -Verbose -ErrorAction SilentlyContinue",
            true,
        );
        if out.success() {
            runner
                .logger
                .success("Optimisation du volume C: (Optimize-Volume) terminée.");
        }
    }
}

/// Construit une ligne de comparaison Avant / Après.
fn make_diff_f64(
    metric: &str,
    before: f64,
    after: f64,
    unit: &str,
    lower_is_better: bool,
) -> MetricDiff {
    let d = ((after - before) * 10.0).round() / 10.0;
    let sign = if d > 0.0 { "+" } else { "" };
    let improved = if d.abs() < 0.05 {
        None
    } else if lower_is_better {
        Some(d < 0.0)
    } else {
        Some(d > 0.0)
    };
    let arrow = match improved {
        Some(true) => "↑",
        Some(false) => "↓",
        None => "=",
    };

    MetricDiff {
        metric: metric.to_string(),
        before: format!("{before:.1} {unit}"),
        after: format!("{after:.1} {unit}"),
        delta: format!("{sign}{d:.1} {unit} {arrow}"),
        improved,
    }
}

fn make_diff_u64(
    metric: &str,
    before: u64,
    after: u64,
    unit: &str,
    lower_is_better: bool,
) -> MetricDiff {
    let d = after as i64 - before as i64;
    let sign = if d > 0 { "+" } else { "" };
    let improved = if d == 0 {
        None
    } else if lower_is_better {
        Some(d < 0)
    } else {
        Some(d > 0)
    };
    let arrow = match improved {
        Some(true) => "↑",
        Some(false) => "↓",
        None => "=",
    };

    MetricDiff {
        metric: metric.to_string(),
        before: format!("{before} {unit}"),
        after: format!("{after} {unit}"),
        delta: format!("{sign}{d} {unit} {arrow}"),
        improved,
    }
}

/// Re-mesure les métriques clés après nettoyage/optimisation et retourne le tableau comparatif.
pub fn compute_pc_comparison(runner: &Runner, before: &PcDiagnosis) -> Vec<MetricDiff> {
    runner
        .logger
        .section("4. COMPARAISON AVANT / APRÈS (Boost-PC)");

    let mut sys = System::new_all();
    thread::sleep(Duration::from_millis(250));
    sys.refresh_cpu_all();

    let ram_total_mb = sys.total_memory() / (1024 * 1024);
    let after_ram_free_mb = sys.available_memory() / (1024 * 1024);
    let after_ram_used_pct = if ram_total_mb > 0 {
        let used = ram_total_mb.saturating_sub(after_ram_free_mb);
        ((used as f64 / ram_total_mb as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };

    let after_cpu_pct = if !sys.cpus().is_empty() {
        let sum: f32 = sys.cpus().iter().map(|c| c.cpu_usage()).sum();
        ((sum as f64 / sys.cpus().len() as f64) * 10.0).round() / 10.0
    } else {
        0.0
    };

    let sys_drive_env = std::env::var("SystemDrive")
        .unwrap_or_else(|_| String::from("C:"))
        .to_uppercase();
    let disks_list = Disks::new_with_refreshed_list();
    let mut after_sys_free_gb = before.sys_free_gb;
    for d in disks_list.list() {
        let mount = d.mount_point().to_string_lossy().to_string();
        let drive = mount.trim_end_matches('\\');
        if drive.eq_ignore_ascii_case(&sys_drive_env) {
            after_sys_free_gb =
                ((d.available_space() as f64 / 1_073_741_824.0) * 100.0).round() / 100.0;
            break;
        }
    }

    let (_, after_cache_total_mb) = measure_caches();

    let diffs = vec![
        make_diff_u64("RAM libre", before.ram_free_mb, after_ram_free_mb, "Mo", false),
        make_diff_f64(
            "RAM utilisée",
            before.ram_used_pct,
            after_ram_used_pct,
            "%",
            true,
        ),
        make_diff_f64(
            "CPU idle moyen",
            before.cpu_usage_pct,
            after_cpu_pct,
            "%",
            true,
        ),
        make_diff_f64(
            "Disque C: libre",
            before.sys_free_gb,
            after_sys_free_gb,
            "Go",
            false,
        ),
        make_diff_u64(
            "Caches cumulés",
            before.cache_total_mb,
            after_cache_total_mb,
            "Mo",
            true,
        ),
    ];

    for diff in &diffs {
        runner.logger.info(format!(
            "{:<18} | Avant : {:<12} | Après : {:<12} | Delta : {}",
            diff.metric, diff.before, diff.after, diff.delta
        ));
    }

    diffs
}

/// Calcule récursivement la taille en octets et le nombre de fichiers d'un dossier avec limite de profondeur.
fn dir_stats(path: &Path, max_depth: usize) -> (u64, u64) {
    if max_depth == 0 || !path.exists() {
        return (0, 0);
    }
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return (0, 0),
    };

    let mut total_bytes: u64 = 0;
    let mut total_files: u64 = 0;

    for entry in entries.flatten() {
        if let Ok(ft) = entry.file_type() {
            if ft.is_symlink() {
                continue;
            }
            if ft.is_file() {
                if let Ok(meta) = entry.metadata() {
                    total_bytes = total_bytes.saturating_add(meta.len());
                    total_files = total_files.saturating_add(1);
                }
            } else if ft.is_dir() {
                let (sub_bytes, sub_files) = dir_stats(&entry.path(), max_depth.saturating_sub(1));
                total_bytes = total_bytes.saturating_add(sub_bytes);
                total_files = total_files.saturating_add(sub_files);
            }
        }
    }
    (total_bytes, total_files)
}

/// Parcourt les entrées de premier niveau (dossiers et gros fichiers) de %TEMP% (et C:\Windows\Temp),
/// calcule leur taille et nombre de fichiers (profondeur max 20), et retourne les 15 plus lourds (>= 0.1 Mo).
pub fn scan_top_temp_folders(runner: &Runner) -> Vec<TempSubfolderInfo> {
    let mut base_dirs: Vec<PathBuf> = Vec::new();

    if let Ok(user_temp) = std::env::var("TEMP") {
        let p = PathBuf::from(user_temp);
        if p.is_dir() {
            base_dirs.push(p);
        }
    } else if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local_app).join("Temp");
        if p.is_dir() {
            base_dirs.push(p);
        }
    }

    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| String::from(r"C:\Windows"));
    let sys_temp = PathBuf::from(format!(r"{sys_root}\Temp"));
    if sys_temp.is_dir() {
        let already_present = base_dirs.iter().any(|d| {
            d.to_string_lossy()
                .eq_ignore_ascii_case(&sys_temp.to_string_lossy())
        });
        if !already_present {
            base_dirs.push(sys_temp);
        }
    }

    let mut items: Vec<TempSubfolderInfo> = Vec::new();

    for base in &base_dirs {
        let entries = match fs::read_dir(base) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_symlink() {
                continue;
            }

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let path_str = path.to_string_lossy().into_owned();

            if ft.is_dir() {
                let (bytes, count) = dir_stats(&path, 20);
                let size_mb = ((bytes as f64 / 1_048_576.0) * 100.0).round() / 100.0;
                if size_mb >= 0.1 {
                    items.push(TempSubfolderInfo {
                        name,
                        path: path_str,
                        size_mb,
                        file_count: count,
                        is_dir: true,
                    });
                }
            } else if ft.is_file() {
                if let Ok(meta) = entry.metadata() {
                    let bytes = meta.len();
                    let size_mb = ((bytes as f64 / 1_048_576.0) * 100.0).round() / 100.0;
                    if size_mb >= 0.1 {
                        items.push(TempSubfolderInfo {
                            name,
                            path: path_str,
                            size_mb,
                            file_count: 1,
                            is_dir: false,
                        });
                    }
                }
            }
        }
    }

    items.sort_by(|a, b| {
        b.size_mb
            .partial_cmp(&a.size_mb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.file_count.cmp(&a.file_count))
    });
    items.truncate(15);

    runner.logger.info(format!(
        "Top éléments dans %TEMP% analysés : {} éléments lourds détectés.",
        items.len()
    ));

    items
}

/// Vérifie de façon sécurisée qu'un chemin se situe strictement à l'intérieur d'un répertoire temporaire (%TEMP% ou C:\Windows\Temp).
fn is_safe_temp_target(target_str: &str) -> bool {
    let target = Path::new(target_str);

    if target_str.trim().is_empty()
        || target
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return false;
    }

    let mut allowed_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(user_temp) = std::env::var("TEMP") {
        allowed_dirs.push(PathBuf::from(user_temp));
    }
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        allowed_dirs.push(PathBuf::from(local_app).join("Temp"));
    }
    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| String::from(r"C:\Windows"));
    allowed_dirs.push(PathBuf::from(format!(r"{sys_root}\Temp")));

    let norm_target = target_str
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();

    let target_canon = target.canonicalize().ok();

    for base in allowed_dirs {
        if let (Some(tc), Ok(bc)) = (&target_canon, base.canonicalize()) {
            if tc.starts_with(&bc) && tc != &bc {
                return true;
            }
        }

        let mut norm_base = base
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase();
        norm_base.push('\\');

        if norm_target.starts_with(&norm_base) && norm_target.len() > norm_base.len() {
            return true;
        }
    }

    false
}

/// Supprime un dossier ou un fichier cible situé dans %TEMP% ou C:\Windows\Temp avec repli rmdir en cas d'arborescence profonde.
pub fn delete_temp_target(runner: &Runner, target_path: &str) -> bool {
    if !is_safe_temp_target(target_path) {
        runner.logger.warn(format!(
            "Sécurité : refus de suppression du chemin hors %TEMP% : {target_path}"
        ));
        return false;
    }

    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Suppression simulée de : {target_path}"
        ));
        return true;
    }

    let path = Path::new(target_path);
    if !path.exists() {
        runner.logger.info(format!(
            "Cible temporaire introuvable ou déjà supprimée : {target_path}"
        ));
        return true;
    }

    let is_dir = path.is_dir();
    let success = if is_dir {
        match fs::remove_dir_all(path) {
            Ok(()) => true,
            Err(e) => {
                runner.logger.warn(format!(
                    "remove_dir_all échoué pour '{target_path}' ({e}), tentative via cmd.exe rmdir /s /q..."
                ));
                let out = runner.run_action(
                    "cmd.exe",
                    &["/c", "rmdir", "/s", "/q", target_path],
                    false,
                );
                out.success() || !path.exists()
            }
        }
    } else {
        match fs::remove_file(path) {
            Ok(()) => true,
            Err(e) => {
                if let Ok(meta) = fs::metadata(path) {
                    let mut perms = meta.permissions();
                    if perms.readonly() {
                        perms.set_readonly(false);
                        let _ = fs::set_permissions(path, perms);
                        fs::remove_file(path).is_ok()
                    } else {
                        runner.logger.warn(format!(
                            "Impossible de supprimer le fichier {target_path} : {e}"
                        ));
                        false
                    }
                } else {
                    runner.logger.warn(format!(
                        "Impossible de supprimer le fichier {target_path} : {e}"
                    ));
                    false
                }
            }
        }
    };

    if success {
        runner.logger.success(format!(
            "Élément temporaire supprimé avec succès : {target_path}"
        ));
        true
    } else {
        runner.logger.warn(format!(
            "Échec de suppression de l'élément temporaire (verrouillé ou en cours d'utilisation) : {target_path}"
        ));
        false
    }
}

/// Crée un point de restauration système Windows via Checkpoint-Computer (en désactivant la limitation 24h).
pub fn create_system_restore_point(runner: &Runner, description: &str) -> bool {
    let clean_desc = if description.trim().is_empty() {
        "WinCare - Point de restauration"
    } else {
        description.trim()
    };

    let safe_desc = clean_desc
        .replace('\'', "''")
        .replace('\r', "")
        .replace('\n', " ");

    let ps_script = format!(
        r#"try {{
    $srKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore'
    if (-not (Test-Path $srKey)) {{
        New-Item -Path $srKey -Force | Out-Null
    }}
    Set-ItemProperty -Path $srKey -Name 'SystemRestorePointCreationFrequency' -Value 0 -Type DWord -Force -ErrorAction SilentlyContinue
    Enable-ComputerRestore -Drive "$env:SystemDrive\" -ErrorAction SilentlyContinue
    Checkpoint-Computer -Description '{safe_desc}' -RestorePointType 'MODIFY_SETTINGS' -ErrorAction Stop
    exit 0
}} catch {{
    Write-Error $_.Exception.Message
    exit 1
}}"#
    );

    let out = runner.run_ps_action(
        &format!("Création point de restauration système : {clean_desc}"),
        &ps_script,
        false,
    );

    if out.exit_code == 0 {
        runner.logger.success(format!(
            "Point de restauration système créé avec succès : \"{clean_desc}\""
        ));
        true
    } else {
        let err_detail = if !out.stderr.trim().is_empty() {
            out.stderr.trim().to_string()
        } else if !out.stdout.trim().is_empty() {
            out.stdout.trim().to_string()
        } else {
            format!("code {}", out.exit_code)
        };
        runner.logger.warn(format!(
            "Échec de création du point de restauration système \"{clean_desc}\" : {err_detail}"
        ));
        false
    }
}
