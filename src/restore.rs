use crate::runner::Runner;
use crate::types::RestoreOptions;
use serde::Deserialize;
use winreg::enums::*;
use winreg::{RegKey, HKEY};

const VISUAL_FX_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects";
const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const STORAGE_POLICY_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\StorageSense\Parameters\StoragePolicy";
const GAME_BAR_KEY: &str = r"Software\Microsoft\GameBar";
const GAME_CONFIG_STORE_KEY: &str = r"System\GameConfigStore";
const DELIVERY_OPT_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization";
const WIFI_SUBGROUP_GUID: &str = "SUB_WIRELESSADAPTER";
const WIFI_POWER_SETTING_GUID: &str = "12bbebe6-58d6-4636-95bb-3217ef867c1a";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DefaultNetAdapter {
    if_index: u32,
    adapter_name: String,
}

pub fn execute_restore(
    runner: &Runner,
    opts: &RestoreOptions,
    if_index: Option<u32>,
    adapter_name: Option<&str>,
) {
    runner
        .logger
        .section("RESTAURATION DES RÉGLAGES PAR DÉFAUT WINDOWS");

    let total_steps = 10usize;

    // Détection automatique de l'interface réseau active si nécessaire pour DNS ou MTU
    let need_net_detect = (opts.restore_dns_dhcp && if_index.unwrap_or(0) == 0)
        || (opts.restore_mtu_1500
            && adapter_name.map(|s| s.trim().is_empty()).unwrap_or(true));

    let detected_net = if need_net_detect {
        detect_default_interface(runner)
    } else {
        DefaultNetAdapter::default()
    };

    let resolved_if_index = if_index
        .filter(|&idx| idx > 0)
        .or_else(|| (detected_net.if_index > 0).then_some(detected_net.if_index));

    let resolved_adapter_name: Option<String> = adapter_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            (!detected_net.adapter_name.is_empty()).then_some(detected_net.adapter_name)
        });

    // 1. Plan d'alimentation équilibré
    runner
        .logger
        .progress(1, total_steps, "Restauration du plan d'alimentation équilibré...");
    if opts.restore_power_balanced {
        let out = runner.run_action("powercfg", &["/setactive", "SCHEME_BALANCED"], true);
        if out.success() {
            runner
                .logger
                .success("Plan d'alimentation remis sur Équilibré (SCHEME_BALANCED).");
        } else {
            runner
                .logger
                .warn("Impossible de réactiver le plan d'alimentation SCHEME_BALANCED.");
        }
    }

    // 2. Effets visuels automatiques
    runner
        .logger
        .progress(2, total_steps, "Restauration des effets visuels par défaut...");
    if opts.restore_visual_fx {
        set_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            VISUAL_FX_KEY,
            "VisualFXSetting",
            0,
            "Effets visuels remis en mode automatique (VisualFXSetting = 0).",
        );
    }

    // 3. Transparence Windows
    runner
        .logger
        .progress(3, total_steps, "Réactivation de la transparence Windows...");
    if opts.restore_transparency {
        set_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            PERSONALIZE_KEY,
            "EnableTransparency",
            1,
            "Transparence Windows réactivée (EnableTransparency = 1).",
        );
    }

    // 4. Service SysMain (Superfetch)
    runner
        .logger
        .progress(4, total_steps, "Réactivation du service SysMain (Superfetch)...");
    if opts.restore_sysmain {
        let out = runner.run_ps_action(
            "Réactivation et démarrage du service SysMain",
            "Set-Service -Name SysMain -StartupType Automatic -ErrorAction SilentlyContinue; Start-Service -Name SysMain -ErrorAction SilentlyContinue",
            true,
        );
        if out.success() {
            runner
                .logger
                .success("Service SysMain remis en démarrage Automatique et démarré.");
        } else {
            runner
                .logger
                .warn("Impossible de reconfigurer ou démarrer le service SysMain.");
        }
    }

    // 5. Storage Sense (Assistant stockage)
    runner
        .logger
        .progress(5, total_steps, "Désactivation de Storage Sense...");
    if opts.restore_storage_sense {
        set_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            STORAGE_POLICY_KEY,
            "01",
            0,
            "Assistant stockage (Storage Sense) remis à l'état désactivé ('01' = 0).",
        );
    }

    // 6. Xbox Game Bar & Mode Jeu
    runner
        .logger
        .progress(6, total_steps, "Réactivation de Game Bar et Game DVR...");
    if opts.restore_game_bar {
        set_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            GAME_BAR_KEY,
            "AutoGameModeEnabled",
            1,
            "Game Bar : AutoGameModeEnabled = 1.",
        );
        set_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            GAME_BAR_KEY,
            "AllowAutoGameMode",
            1,
            "Game Bar : AllowAutoGameMode = 1.",
        );
        set_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            GAME_CONFIG_STORE_KEY,
            "GameDVR_Enabled",
            1,
            "Game DVR réactivé (GameDVR_Enabled = 1).",
        );
    }

    // 7. Serveurs DNS en DHCP automatique
    runner
        .logger
        .progress(7, total_steps, "Réinitialisation des serveurs DNS en DHCP...");
    if opts.restore_dns_dhcp {
        if let Some(idx) = resolved_if_index {
            let ps_cmd = format!(
                "Set-DnsClientServerAddress -InterfaceIndex {idx} -ResetServerAddresses"
            );
            let desc = format!(
                "Réinitialisation des serveurs DNS en DHCP sur l'interface #{idx}"
            );
            let out = runner.run_ps_action(&desc, &ps_cmd, true);
            if out.success() {
                runner.logger.success(format!(
                    "Serveurs DNS remis en automatique (DHCP) sur l'interface #{idx}."
                ));
            } else {
                runner.logger.warn(format!(
                    "Échec de la réinitialisation DNS sur l'interface #{idx}."
                ));
            }
        } else {
            runner
                .logger
                .warn("Aucune interface réseau active détectée pour réinitialiser les DNS.");
        }
    }

    // 8. MTU par défaut (1500)
    runner
        .logger
        .progress(8, total_steps, "Restauration du MTU par défaut (1500)...");
    if opts.restore_mtu_1500 {
        if let Some(name) = resolved_adapter_name.as_deref() {
            let out = runner.run_action(
                "netsh",
                &[
                    "interface",
                    "ipv4",
                    "set",
                    "subinterface",
                    name,
                    "mtu=1500",
                    "store=persistent",
                ],
                true,
            );
            if out.success() {
                runner
                    .logger
                    .success(format!("MTU restauré à 1500 sur la carte '{name}'."));
            } else {
                runner.logger.warn(format!(
                    "Impossible de restaurer le MTU à 1500 sur la carte '{name}'."
                ));
            }
        } else {
            runner
                .logger
                .warn("Aucun adaptateur réseau détecté pour restaurer le MTU à 1500.");
        }
    }

    // 9. Restauration Windows Update P2P (Delivery Optimization)
    runner
        .logger
        .progress(9, total_steps, "Restauration de Delivery Optimization P2P...");
    if opts.restore_wu_p2p {
        delete_reg_value(
            runner,
            HKEY_LOCAL_MACHINE,
            "HKLM",
            DELIVERY_OPT_KEY,
            "DODownloadMode",
            "Restriction Delivery Optimization supprimée (DODownloadMode retiré).",
        );
    }

    // 10. Restauration de l'économie d'énergie Wi-Fi par défaut (3)
    runner
        .logger
        .progress(10, total_steps, "Restauration de l'économie d'énergie Wi-Fi...");
    if opts.restore_wifi_power {
        let r1 = runner.run_action(
            "powercfg",
            &[
                "/setacvalueindex",
                "SCHEME_CURRENT",
                WIFI_SUBGROUP_GUID,
                WIFI_POWER_SETTING_GUID,
                "3",
            ],
            true,
        );
        let r2 = runner.run_action("powercfg", &["/setactive", "SCHEME_CURRENT"], true);
        if r1.success() && r2.success() {
            runner
                .logger
                .success("Économie d'énergie Wi-Fi remise à la valeur par défaut (3).");
        } else {
            runner
                .logger
                .warn("Avertissement lors de la restauration de l'économie d'énergie Wi-Fi.");
        }
    }

    runner
        .logger
        .progress(total_steps, total_steps, "Restauration terminée.");
    runner
        .logger
        .success("Opérations de restauration terminées.");
}

fn detect_default_interface(runner: &Runner) -> DefaultNetAdapter {
    let ps_script = r#"
$activeIf = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue |
    Sort-Object -Property RouteMetric | Select-Object -First 1
if ($activeIf) {
    $ifIndex = [uint32]$activeIf.InterfaceIndex
    $ad = Get-NetAdapter -InterfaceIndex $ifIndex -ErrorAction SilentlyContinue
    $name = if ($ad) { [string]$ad.Name } else { '' }
} else {
    $ad = Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object Status -eq 'Up' | Select-Object -First 1
    $ifIndex = if ($ad) { [uint32]$ad.ifIndex } else { [uint32]0 }
    $name = if ($ad) { [string]$ad.Name } else { '' }
}
[pscustomobject]@{
    if_index     = $ifIndex
    adapter_name = $name
} | ConvertTo-Json -Compress
"#;

    let out = runner.run_ps_query(ps_script);
    let trimmed = out.stdout.trim();
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end >= start {
            if let Ok(parsed) = serde_json::from_str::<DefaultNetAdapter>(&trimmed[start..=end]) {
                return parsed;
            }
        }
    }
    DefaultNetAdapter::default()
}

fn set_reg_dword(
    runner: &Runner,
    root: HKEY,
    root_label: &str,
    subkey: &str,
    name: &str,
    value: u32,
    success_msg: &str,
) {
    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Registre simulé : {root_label}\\{subkey}\\{name} = {value} (DWORD)"
        ));
        return;
    }

    let reg_root = RegKey::predef(root);
    match reg_root.create_subkey(subkey) {
        Ok((key, _)) => match key.set_value(name, &value) {
            Ok(()) => runner.logger.success(success_msg),
            Err(e) => runner.logger.warn(format!(
                "Impossible d'écrire {root_label}\\{subkey}\\{name} : {e}"
            )),
        },
        Err(e) => runner.logger.warn(format!(
            "Impossible d'ouvrir la clé {root_label}\\{subkey} : {e}"
        )),
    }
}

fn delete_reg_value(
    runner: &Runner,
    root: HKEY,
    root_label: &str,
    subkey: &str,
    name: &str,
    success_msg: &str,
) {
    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Suppression valeur registre simulée : {root_label}\\{subkey}\\{name}"
        ));
        return;
    }

    let reg_root = RegKey::predef(root);
    match reg_root.open_subkey_with_flags(subkey, KEY_SET_VALUE) {
        Ok(key) => match key.delete_value(name) {
            Ok(()) => runner.logger.success(success_msg),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                runner.logger.info(format!(
                    "Valeur {root_label}\\{subkey}\\{name} déjà absente."
                ));
            }
            Err(e) => runner.logger.warn(format!(
                "Impossible de supprimer {root_label}\\{subkey}\\{name} : {e}"
            )),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            runner
                .logger
                .info(format!("Clé {root_label}\\{subkey} déjà absente."));
        }
        Err(e) => runner.logger.warn(format!(
            "Impossible d'ouvrir {root_label}\\{subkey} en écriture : {e}"
        )),
    }
}
