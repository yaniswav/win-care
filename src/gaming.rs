use crate::runner::{Runner, CREATE_NO_WINDOW};
use crate::types::GamingStatus;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::{RegKey, HKEY};

// -----------------------------------------------------------------------------
// Helpers de registre winreg
// -----------------------------------------------------------------------------

fn read_reg_dword(root: HKEY, subkey: &str, name: &str) -> Option<u32> {
    let reg_root = RegKey::predef(root);
    reg_root
        .open_subkey_with_flags(subkey, KEY_READ)
        .ok()?
        .get_value::<u32, _>(name)
        .ok()
}

fn write_reg_dword(
    runner: &Runner,
    root: HKEY,
    root_str: &str,
    subkey: &str,
    name: &str,
    value: u32,
) -> bool {
    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Registre simulé : {root_str}\\{subkey}\\{name} = {value} (DWORD)"
        ));
        return true;
    }

    let reg_root = RegKey::predef(root);
    match reg_root.create_subkey(subkey) {
        Ok((key, _)) => match key.set_value(name, &value) {
            Ok(()) => {
                runner.logger.info(format!(
                    "Registre modifié : {root_str}\\{subkey}\\{name} = {value}"
                ));
                true
            }
            Err(e) => {
                runner.logger.warn(format!(
                    "Impossible d'écrire {root_str}\\{subkey}\\{name} : {e}"
                ));
                false
            }
        },
        Err(e) => {
            runner.logger.warn(format!(
                "Impossible d'ouvrir ou créer la clé {root_str}\\{subkey} : {e}"
            ));
            false
        }
    }
}

// -----------------------------------------------------------------------------
// Helpers Caches Shaders
// -----------------------------------------------------------------------------

fn get_shader_cache_dirs() -> Vec<PathBuf> {
    let local = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let user =
                std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".to_string());
            PathBuf::from(user).join(r"AppData\Local")
        });

    vec![
        local.join("NVIDIA").join("DXCache"),
        local.join("NVIDIA").join("GLCache"),
        local.join("AMD").join("DxCache"),
        local.join("AMD").join("OglCache"),
        local.join("D3DSCache"),
    ]
}

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
                if let Ok(m) = entry.metadata() {
                    total = total.saturating_add(m.len());
                }
            } else if ft.is_dir() {
                total = total.saturating_add(folder_size_bytes(&entry.path(), max_depth - 1));
            }
        }
    }
    total
}

fn clean_dir_recursive(dir: &Path) -> (u64, usize, usize) {
    let mut bytes = 0u64;
    let mut removed = 0usize;
    let mut skipped = 0usize;

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return (0, 0, 0),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(ft) = entry.file_type() {
            if ft.is_file() || ft.is_symlink() {
                let sz = entry.metadata().map(|m| m.len()).unwrap_or(0);
                match fs::remove_file(&path) {
                    Ok(()) => {
                        bytes = bytes.saturating_add(sz);
                        removed += 1;
                    }
                    Err(_) => {
                        skipped += 1;
                    }
                }
            } else if ft.is_dir() {
                let (sub_bytes, sub_rem, sub_skip) = clean_dir_recursive(&path);
                bytes = bytes.saturating_add(sub_bytes);
                removed += sub_rem;
                skipped += sub_skip;
                let _ = fs::remove_dir(&path);
            }
        }
    }
    (bytes, removed, skipped)
}

// -----------------------------------------------------------------------------
// Helpers Schéma d'alimentation Ultimate Performance
// -----------------------------------------------------------------------------

fn extract_guid(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        let clean = token.trim_matches(|c: char| {
            c == '(' || c == ')' || c == '{' || c == '}' || c == ':' || c == '*'
        });
        if clean.len() == 36 {
            let parts: Vec<&str> = clean.split('-').collect();
            if parts.len() == 5
                && parts[0].len() == 8
                && parts[1].len() == 4
                && parts[2].len() == 4
                && parts[3].len() == 4
                && parts[4].len() == 12
                && clean.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
            {
                return Some(clean.to_string());
            }
        }
    }
    None
}

fn query_power_schemes(runner: &Runner) -> (Option<String>, bool) {
    let mut cmd = Command::new("powercfg");
    cmd.arg("/list").creation_flags(CREATE_NO_WINDOW);

    let output = match cmd.output() {
        Ok(o) => Runner::decode_windows_bytes(&o.stdout),
        Err(e) => {
            runner
                .logger
                .warn(format!("Impossible d'exécuter powercfg /list : {e}"));
            return (None, false);
        }
    };

    parse_powercfg_list(&output)
}

fn parse_powercfg_list(output: &str) -> (Option<String>, bool) {
    let mut ultimate_guid = None;
    let is_active = false;

    for line in output.lines() {
        let line_trimmed = line.trim();
        let lower = line_trimmed.to_lowercase();
        let is_target = lower.contains("e9a42b02-d5df-448d-aa00-03f14749eb61")
            || lower.contains("ultimate performance")
            || lower.contains("performances ultimes")
            || lower.contains("performances optimales");

        if is_target {
            if let Some(guid) = extract_guid(line_trimmed) {
                let active = line_trimmed.ends_with('*');
                if active {
                    return (Some(guid), true);
                }
                if ultimate_guid.is_none() {
                    ultimate_guid = Some(guid);
                }
            }
        }
    }

    (ultimate_guid, is_active)
}

// -----------------------------------------------------------------------------
// Fonctions publiques de gaming.rs
// -----------------------------------------------------------------------------

/// Interroge l'état actuel des optimisations gaming (HAGS, VBS, Game Mode, Ultimate Plan, Cache shaders).
pub fn query_gaming_status(runner: &Runner) -> GamingStatus {
    // 1. HAGS (Hardware-Accelerated GPU Scheduling)
    let hw_sch = read_reg_dword(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
        "HwSchMode",
    );
    let hags_enabled = hw_sch == Some(2);

    // 2. VBS (Virtualization-Based Security / Memory Integrity)
    let vbs1 = read_reg_dword(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
        "EnableVirtualizationBasedSecurity",
    );
    let vbs2 = read_reg_dword(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
        "Enabled",
    );
    let vbs_enabled = vbs1 == Some(1) || vbs2 == Some(1);

    // 3. Game Mode (AutoGameModeEnabled)
    let gm = read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\GameBar",
        "AutoGameModeEnabled",
    );
    let game_mode_enabled = gm.map(|v| v != 0).unwrap_or(true);

    // 4. Ultimate Performance Plan
    let (ultimate_perf_guid, ultimate_perf_active) = query_power_schemes(runner);

    // 5. GPU Shader Cache Size
    let shader_dirs = get_shader_cache_dirs();
    let total_bytes: u64 = shader_dirs
        .iter()
        .filter(|p| p.is_dir())
        .map(|p| folder_size_bytes(p, 16))
        .sum();
    let shader_cache_size_mb = (total_bytes + 524_288) / 1_048_576;

    GamingStatus {
        ultimate_perf_active,
        ultimate_perf_guid,
        hags_enabled,
        vbs_enabled,
        game_mode_enabled,
        shader_cache_size_mb,
    }
}

/// Nettoie les caches de shaders GPU (NVIDIA DX/GL, AMD Dx/Ogl, D3DSCache).
/// Retourne le nombre de Mo libérés (ou simulés en dry-run).
pub fn clean_gpu_shader_caches(runner: &Runner) -> u64 {
    runner.logger.section("NETTOYAGE DU CACHE DE SHADERS GPU");
    let shader_dirs = get_shader_cache_dirs();

    if runner.dry_run {
        let total_bytes: u64 = shader_dirs
            .iter()
            .filter(|p| p.is_dir())
            .map(|p| folder_size_bytes(p, 16))
            .sum();
        let mb = (total_bytes + 524_288) / 1_048_576;
        runner.logger.info(format!(
            "[DRY-RUN] Nettoyage simulé du cache de shaders GPU : ~{mb} Mo à libérer"
        ));
        return mb;
    }

    let mut total_freed_bytes: u64 = 0;
    let mut total_removed: usize = 0;
    let mut total_skipped: usize = 0;

    for dir in &shader_dirs {
        if !dir.is_dir() {
            continue;
        }
        let (bytes, rem, skip) = clean_dir_recursive(dir);
        total_freed_bytes = total_freed_bytes.saturating_add(bytes);
        total_removed += rem;
        total_skipped += skip;
    }

    let mb = (total_freed_bytes + 524_288) / 1_048_576;
    runner.logger.success(format!(
        "Cache shaders GPU nettoyé : {mb} Mo libérés ({total_removed} fichiers supprimés, {total_skipped} verrouillés/ignorés)."
    ));
    mb
}

/// Duplique le plan d'alimentation Performances Ultimes s'il n'existe pas encore, puis l'active.
pub fn enable_ultimate_power_plan(runner: &Runner) -> bool {
    runner
        .logger
        .section("ACTIVATION DU MODE PERFORMANCES ULTIMES");
    let (existing_guid, is_active) = query_power_schemes(runner);
    if is_active {
        runner
            .logger
            .info("Le plan Performances Ultimes est déjà actif.");
        return true;
    }

    if runner.dry_run {
        runner.logger.info(
            "[DRY-RUN] Duplication et activation simulée du plan d'alimentation Performances Ultimes",
        );
        return true;
    }

    let target_guid = if let Some(guid) = existing_guid {
        guid
    } else {
        runner
            .logger
            .info("Création du schéma d'alimentation Performances Ultimes...");
        let dup = runner.run_action(
            "powercfg",
            &["/duplicatescheme", "e9a42b02-d5df-448d-aa00-03f14749eb61"],
            true,
        );
        if let Some(new_guid) = extract_guid(&dup.stdout) {
            new_guid
        } else {
            let (recheck_guid, _) = query_power_schemes(runner);
            recheck_guid.unwrap_or_else(|| "e9a42b02-d5df-448d-aa00-03f14749eb61".to_string())
        }
    };

    runner
        .logger
        .info(format!("Activation du plan d'alimentation {target_guid}..."));
    let res = runner.run_action("powercfg", &["/setactive", &target_guid], true);
    if res.success() {
        runner
            .logger
            .success("Plan Performances Ultimes activé avec succès.");
        true
    } else {
        runner.logger.warn(format!(
            "Échec de l'activation du plan d'alimentation : {}",
            res.stderr
        ));
        false
    }
}

/// Active ou désactive le HAGS (Hardware-Accelerated GPU Scheduling).
pub fn set_hags(runner: &Runner, enabled: bool) -> bool {
    let val = if enabled { 2 } else { 1 };
    let action_str = if enabled {
        "Activation"
    } else {
        "Désactivation"
    };
    runner.logger.info(format!(
        "{action_str} du HAGS (Hardware-Accelerated GPU Scheduling)..."
    ));
    write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
        "HwSchMode",
        val,
    )
}

/// Active ou désactive VBS (Virtualization-Based Security / Intégrité de la mémoire).
pub fn set_vbs(runner: &Runner, enabled: bool) -> bool {
    let val = if enabled { 1 } else { 0 };
    let action_str = if enabled {
        "Activation"
    } else {
        "Désactivation"
    };
    runner
        .logger
        .info(format!("{action_str} de VBS / Intégrité de la mémoire..."));

    let ok1 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
        "EnableVirtualizationBasedSecurity",
        val,
    );
    let ok2 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
        "Enabled",
        val,
    );

    let launch_type = if enabled { "auto" } else { "off" };
    let bcd_res = runner.run_action(
        "bcdedit",
        &["/set", "hypervisorlaunchtype", launch_type],
        true,
    );

    ok1 && ok2 && (bcd_res.success() || runner.dry_run)
}

/// Active ou désactive le Mode Jeu Windows (Game Mode).
pub fn set_game_mode(runner: &Runner, enabled: bool) -> bool {
    let val = if enabled { 1 } else { 0 };
    let action_str = if enabled {
        "Activation"
    } else {
        "Désactivation"
    };
    runner
        .logger
        .info(format!("{action_str} du Mode Jeu Windows..."));
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\GameBar",
        "AutoGameModeEnabled",
        val,
    )
}
