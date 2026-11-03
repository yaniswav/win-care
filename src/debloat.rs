use crate::runner::Runner;
use crate::types::{DebloatCategory, DebloatTweakStatus};
use std::io;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE};
use winreg::{RegKey, HKEY};

/// Définition statique interne d'un tweak Windows 11.
struct DebloatDef {
    id: &'static str,
    category: DebloatCategory,
    title: &'static str,
    description: &'static str,
    recommended: bool,
    check_fn: fn() -> bool,
    apply_fn: fn(&Runner) -> bool,
    restore_fn: fn(&Runner) -> bool,
}

// -----------------------------------------------------------------------------
// Helpers de registre winreg (avec support strict du dry_run et gestion d'erreurs)
// -----------------------------------------------------------------------------

fn read_reg_dword(root: HKEY, subkey: &str, name: &str) -> Option<u32> {
    let reg_root = RegKey::predef(root);
    reg_root
        .open_subkey_with_flags(subkey, KEY_READ)
        .ok()?
        .get_value::<u32, _>(name)
        .ok()
}

fn read_reg_string(root: HKEY, subkey: &str, name: &str) -> Option<String> {
    let reg_root = RegKey::predef(root);
    reg_root
        .open_subkey_with_flags(subkey, KEY_READ)
        .ok()?
        .get_value::<String, _>(name)
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
            Ok(()) => true,
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

fn write_reg_string(
    runner: &Runner,
    root: HKEY,
    root_str: &str,
    subkey: &str,
    name: &str,
    value: &str,
) -> bool {
    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Registre simulé : {root_str}\\{subkey}\\{name} = \"{value}\" (SZ)"
        ));
        return true;
    }

    let reg_root = RegKey::predef(root);
    match reg_root.create_subkey(subkey) {
        Ok((key, _)) => match key.set_value(name, &value) {
            Ok(()) => true,
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

fn delete_reg_val(
    runner: &Runner,
    root: HKEY,
    root_str: &str,
    subkey: &str,
    name: &str,
) -> bool {
    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Suppression valeur registre simulée : {root_str}\\{subkey}\\{name}"
        ));
        return true;
    }

    let reg_root = RegKey::predef(root);
    match reg_root.open_subkey_with_flags(subkey, KEY_SET_VALUE) {
        Ok(key) => match key.delete_value(name) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => true,
            Err(e) => {
                runner.logger.warn(format!(
                    "Impossible de supprimer {root_str}\\{subkey}\\{name} : {e}"
                ));
                false
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => true,
        Err(e) => {
            runner.logger.warn(format!(
                "Impossible d'ouvrir la clé {root_str}\\{subkey} pour suppression : {e}"
            ));
            false
        }
    }
}

fn delete_reg_key_tree(runner: &Runner, root: HKEY, root_str: &str, subkey: &str) -> bool {
    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Suppression clé registre simulée : {root_str}\\{subkey}"
        ));
        return true;
    }

    let reg_root = RegKey::predef(root);
    match reg_root.delete_subkey_all(subkey) {
        Ok(()) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => true,
        Err(e) => {
            runner.logger.warn(format!(
                "Impossible de supprimer la clé {root_str}\\{subkey} : {e}"
            ));
            false
        }
    }
}

// -----------------------------------------------------------------------------
// 1. Catégorie : AiAndCopilot
// -----------------------------------------------------------------------------

fn check_disable_copilot() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Policies\Microsoft\Windows\WindowsCopilot",
        "TurnOffWindowsCopilot",
    ) == Some(1)
        || read_reg_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot",
            "TurnOffWindowsCopilot",
        ) == Some(1)
}

fn apply_disable_copilot(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "ShowCopilotButton",
        0,
    );
    let r2 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\WindowsCopilot",
        "TurnOffWindowsCopilot",
        1,
    );
    let r3 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot",
        "TurnOffWindowsCopilot",
        1,
    );
    r1 && r2 && r3
}

fn restore_disable_copilot(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "ShowCopilotButton",
        1,
    );
    let r2 = delete_reg_val(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\WindowsCopilot",
        "TurnOffWindowsCopilot",
    );
    let r3 = delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot",
        "TurnOffWindowsCopilot",
    );
    r1 && r2 && r3
}

fn check_disable_ai_recall() -> bool {
    read_reg_dword(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "DisableAIDataAnalysis",
    ) == Some(1)
        || read_reg_dword(
            HKEY_CURRENT_USER,
            r"Software\Policies\Microsoft\Windows\WindowsAI",
            "DisableAIDataAnalysis",
        ) == Some(1)
        || read_reg_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
            "AllowRecallEnablement",
        ) == Some(0)
}

fn apply_disable_ai_recall(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\WindowsAI",
        "DisableAIDataAnalysis",
        1,
    );
    let r2 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "DisableAIDataAnalysis",
        1,
    );
    let r3 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "AllowRecallEnablement",
        0,
    );
    let r4 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "TurnOffSavingSnapshots",
        1,
    );
    r1 && r2 && r3 && r4
}

fn restore_disable_ai_recall(runner: &Runner) -> bool {
    let r1 = delete_reg_val(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\WindowsAI",
        "DisableAIDataAnalysis",
    );
    let r2 = delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "DisableAIDataAnalysis",
    );
    let r3 = delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "AllowRecallEnablement",
    );
    let r4 = delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "TurnOffSavingSnapshots",
    );
    r1 && r2 && r3 && r4
}

fn check_disable_click_to_do() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "ClickToDoEnabled",
    ) == Some(0)
        || read_reg_dword(
            HKEY_CURRENT_USER,
            r"Software\Policies\Microsoft\Windows\WindowsAI",
            "DisableClickToDo",
        ) == Some(1)
        || read_reg_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
            "DisableClickToDo",
        ) == Some(1)
}

fn apply_disable_click_to_do(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "ClickToDoEnabled",
        0,
    );
    let r2 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\WindowsAI",
        "DisableClickToDo",
        1,
    );
    let r3 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "DisableClickToDo",
        1,
    );
    r1 && r2 && r3
}

fn restore_disable_click_to_do(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "ClickToDoEnabled",
        1,
    );
    let r2 = delete_reg_val(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\WindowsAI",
        "DisableClickToDo",
    );
    let r3 = delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsAI",
        "DisableClickToDo",
    );
    r1 && r2 && r3
}

fn check_disable_edge_notepad_paint_ai() -> bool {
    read_reg_dword(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Policies\WindowsNotepad",
        "DisableAIFeatures",
    ) == Some(1)
        || read_reg_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint",
            "DisableCocreator",
        ) == Some(1)
        || read_reg_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Policies\Microsoft\Edge",
            "HubsSidebarEnabled",
        ) == Some(0)
}

fn apply_disable_edge_notepad_paint_ai(runner: &Runner) -> bool {
    let mut ok = true;
    let edge_vals = [
        ("CopilotCDPPageContext", 0),
        ("CopilotPageContext", 0),
        ("HubsSidebarEnabled", 0),
        ("EdgeEntraCopilotPageContext", 0),
        ("EdgeHistoryAISearchEnabled", 0),
        ("ComposeInlineEnabled", 0),
        ("GenAILocalFoundationalModelSettings", 1),
        ("NewTabPageBingChatEnabled", 0),
    ];
    for (name, val) in edge_vals {
        if !write_reg_dword(
            runner,
            HKEY_LOCAL_MACHINE,
            "HKLM",
            r"SOFTWARE\Policies\Microsoft\Edge",
            name,
            val,
        ) {
            ok = false;
        }
    }

    if !write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\WindowsNotepad",
        "DisableAIFeatures",
        1,
    ) {
        ok = false;
    }

    let paint_vals = [
        "DisableCocreator",
        "DisableGenerativeFill",
        "DisableImageCreator",
        "DisableGenerativeErase",
        "DisableRemoveBackground",
    ];
    for name in paint_vals {
        if !write_reg_dword(
            runner,
            HKEY_LOCAL_MACHINE,
            "HKLM",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint",
            name,
            1,
        ) {
            ok = false;
        }
    }
    ok
}

fn restore_disable_edge_notepad_paint_ai(runner: &Runner) -> bool {
    let mut ok = true;
    let edge_vals = [
        "CopilotCDPPageContext",
        "CopilotPageContext",
        "HubsSidebarEnabled",
        "EdgeEntraCopilotPageContext",
        "EdgeHistoryAISearchEnabled",
        "ComposeInlineEnabled",
        "GenAILocalFoundationalModelSettings",
        "NewTabPageBingChatEnabled",
    ];
    for name in edge_vals {
        if !delete_reg_val(
            runner,
            HKEY_LOCAL_MACHINE,
            "HKLM",
            r"SOFTWARE\Policies\Microsoft\Edge",
            name,
        ) {
            ok = false;
        }
    }

    if !delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\WindowsNotepad",
        "DisableAIFeatures",
    ) {
        ok = false;
    }

    let paint_vals = [
        "DisableCocreator",
        "DisableGenerativeFill",
        "DisableImageCreator",
        "DisableGenerativeErase",
        "DisableRemoveBackground",
    ];
    for name in paint_vals {
        if !delete_reg_val(
            runner,
            HKEY_LOCAL_MACHINE,
            "HKLM",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint",
            name,
        ) {
            ok = false;
        }
    }
    ok
}

// -----------------------------------------------------------------------------
// 2. Catégorie : PrivacyAndTelemetry
// -----------------------------------------------------------------------------

fn check_disable_telemetry() -> bool {
    read_reg_dword(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        "AllowTelemetry",
    ) == Some(0)
        || read_reg_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection",
            "AllowTelemetry",
        ) == Some(0)
}

fn apply_disable_telemetry(runner: &Runner) -> bool {
    let mut ok = true;
    ok &= write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        "AllowTelemetry",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection",
        "AllowTelemetry",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
        "Enabled",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Privacy",
        "TailoredExperiencesWithDiagnosticDataEnabled",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy",
        "HasAccepted",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Input\TIPC",
        "Enabled",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\InputPersonalization",
        "RestrictImplicitInkCollection",
        1,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\InputPersonalization",
        "RestrictImplicitTextCollection",
        1,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Personalization\Settings",
        "AcceptedPrivacyPolicy",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "Start_TrackProgs",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\System",
        "PublishUserActivities",
        0,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
        "SubscribedContent-338393Enabled",
        0,
    );
    ok
}

fn restore_disable_telemetry(runner: &Runner) -> bool {
    let mut ok = true;
    ok &= write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        "AllowTelemetry",
        3,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection",
        "AllowTelemetry",
        3,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
        "Enabled",
        1,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Privacy",
        "TailoredExperiencesWithDiagnosticDataEnabled",
        1,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy",
        "HasAccepted",
        1,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Input\TIPC",
        "Enabled",
        1,
    );
    ok &= delete_reg_val(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\InputPersonalization",
        "RestrictImplicitInkCollection",
    );
    ok &= delete_reg_val(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\InputPersonalization",
        "RestrictImplicitTextCollection",
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Personalization\Settings",
        "AcceptedPrivacyPolicy",
        1,
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "Start_TrackProgs",
        1,
    );
    ok &= delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\System",
        "PublishUserActivities",
    );
    ok &= write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
        "SubscribedContent-338393Enabled",
        1,
    );
    ok
}

fn check_disable_bing_search() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Policies\Microsoft\Windows\Explorer",
        "DisableSearchBoxSuggestions",
    ) == Some(1)
        || read_reg_dword(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Search",
            "BingSearchEnabled",
        ) == Some(0)
}

fn apply_disable_bing_search(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\Explorer",
        "DisableSearchBoxSuggestions",
        1,
    );
    let r2 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Search",
        "BingSearchEnabled",
        0,
    );
    let r3 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\Windows Search",
        "AllowCortana",
        0,
    );
    let r4 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\Windows Search",
        "CortanaConsent",
        0,
    );
    r1 && r2 && r3 && r4
}

fn restore_disable_bing_search(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\Explorer",
        "DisableSearchBoxSuggestions",
        0,
    );
    let r2 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Search",
        "BingSearchEnabled",
        1,
    );
    let r3 = delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\Windows Search",
        "AllowCortana",
    );
    let r4 = delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\Windows Search",
        "CortanaConsent",
    );
    r1 && r2 && r3 && r4
}

fn check_disable_windows_suggestions_ads() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
        "SubscribedContent-338388Enabled",
    ) == Some(0)
        || read_reg_dword(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
            "Start_IrisRecommendations",
        ) == Some(0)
        || read_reg_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
            "DisableConsumerAccountStateContent",
        ) == Some(1)
}

fn apply_disable_windows_suggestions_ads(runner: &Runner) -> bool {
    let mut ok = true;
    let cdm_vals = [
        "SubscribedContent-310093Enabled",
        "SubscribedContent-338388Enabled",
        "SubscribedContent-338389Enabled",
        "SubscribedContent-338393Enabled",
        "SubscribedContent-353694Enabled",
        "SubscribedContent-353696Enabled",
        "SubscribedContent-353698Enabled",
        "SystemPaneSuggestionsEnabled",
        "SoftLandingEnabled",
        "SilentInstalledAppsEnabled",
        "SubscribedContent-338387Enabled",
        "RotatingLockScreenOverlayEnabled",
    ];
    for name in cdm_vals {
        if !write_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
            name,
            0,
        ) {
            ok = false;
        }
    }

    let exp_vals = [
        "Start_IrisRecommendations",
        "ShowSyncProviderNotifications",
        "Start_AccountNotifications",
    ];
    for name in exp_vals {
        if !write_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
            name,
            0,
        ) {
            ok = false;
        }
    }

    if !write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
        "DisableConsumerAccountStateContent",
        1,
    ) {
        ok = false;
    }
    if !write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\Explorer",
        "HideRecommendedSection",
        1,
    ) {
        ok = false;
    }
    ok
}

fn restore_disable_windows_suggestions_ads(runner: &Runner) -> bool {
    let mut ok = true;
    let cdm_vals = [
        "SubscribedContent-310093Enabled",
        "SubscribedContent-338388Enabled",
        "SubscribedContent-338389Enabled",
        "SubscribedContent-338393Enabled",
        "SubscribedContent-353694Enabled",
        "SubscribedContent-353696Enabled",
        "SubscribedContent-353698Enabled",
        "SystemPaneSuggestionsEnabled",
        "SoftLandingEnabled",
        "SilentInstalledAppsEnabled",
        "SubscribedContent-338387Enabled",
        "RotatingLockScreenOverlayEnabled",
    ];
    for name in cdm_vals {
        if !write_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
            name,
            1,
        ) {
            ok = false;
        }
    }

    let exp_vals = [
        "Start_IrisRecommendations",
        "ShowSyncProviderNotifications",
        "Start_AccountNotifications",
    ];
    for name in exp_vals {
        if !write_reg_dword(
            runner,
            HKEY_CURRENT_USER,
            "HKCU",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
            name,
            1,
        ) {
            ok = false;
        }
    }

    if !delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\CloudContent",
        "DisableConsumerAccountStateContent",
    ) {
        ok = false;
    }
    if !write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Windows\Explorer",
        "HideRecommendedSection",
        0,
    ) {
        ok = false;
    }
    ok
}

fn check_disable_desktop_spotlight() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Policies\Microsoft\Windows\CloudContent",
        "DisableSpotlightCollectionOnDesktop",
    ) == Some(1)
}

fn apply_disable_desktop_spotlight(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\CloudContent",
        "DisableSpotlightCollectionOnDesktop",
        1,
    )
}

fn restore_disable_desktop_spotlight(runner: &Runner) -> bool {
    delete_reg_val(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Policies\Microsoft\Windows\CloudContent",
        "DisableSpotlightCollectionOnDesktop",
    )
}

// -----------------------------------------------------------------------------
// 3. Catégorie : ExplorerAndUi
// -----------------------------------------------------------------------------

fn check_classic_context_menu() -> bool {
    read_reg_string(
        HKEY_CURRENT_USER,
        r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32",
        "",
    )
    .is_some()
}

fn apply_classic_context_menu(runner: &Runner) -> bool {
    write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32",
        "",
        "",
    )
}

fn restore_classic_context_menu(runner: &Runner) -> bool {
    delete_reg_key_tree(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}",
    )
}

fn check_show_file_extensions() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "HideFileExt",
    ) == Some(0)
}

fn apply_show_file_extensions(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "HideFileExt",
        0,
    )
}

fn restore_show_file_extensions(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "HideFileExt",
        1,
    )
}

fn check_show_hidden_folders() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "Hidden",
    ) == Some(1)
}

fn apply_show_hidden_folders(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "Hidden",
        1,
    )
}

fn restore_show_hidden_folders(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "Hidden",
        2,
    )
}

fn check_launch_explorer_this_pc() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "LaunchTo",
    ) == Some(1)
}

fn apply_launch_explorer_this_pc(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "LaunchTo",
        1,
    )
}

fn restore_launch_explorer_this_pc(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "LaunchTo",
        2,
    )
}

fn check_enable_dark_mode() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    ) == Some(0)
        && read_reg_dword(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "SystemUsesLightTheme",
        ) == Some(0)
}

fn apply_enable_dark_mode(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
        0,
    );
    let r2 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "SystemUsesLightTheme",
        0,
    );
    r1 && r2
}

fn restore_enable_dark_mode(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
        1,
    );
    let r2 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "SystemUsesLightTheme",
        1,
    );
    r1 && r2
}

// -----------------------------------------------------------------------------
// 4. Catégorie : SystemAndGaming
// -----------------------------------------------------------------------------

fn check_enable_end_task_taskbar() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings",
        "TaskbarEndTask",
    ) == Some(1)
}

fn apply_enable_end_task_taskbar(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings",
        "TaskbarEndTask",
        1,
    )
}

fn restore_enable_end_task_taskbar(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings",
        "TaskbarEndTask",
        0,
    )
}

fn check_disable_widgets() -> bool {
    read_reg_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "TaskbarDa",
    ) == Some(0)
        || read_reg_dword(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Policies\Microsoft\Dsh",
            "AllowNewsAndInterests",
        ) == Some(0)
}

fn apply_disable_widgets(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "TaskbarDa",
        0,
    );
    let r2 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Dsh",
        "AllowNewsAndInterests",
        0,
    );
    let r3 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Microsoft\PolicyManager\default\NewsAndInterests\AllowNewsAndInterests",
        "value",
        0,
    );
    r1 && r2 && r3
}

fn restore_disable_widgets(runner: &Runner) -> bool {
    let r1 = write_reg_dword(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "TaskbarDa",
        1,
    );
    let r2 = delete_reg_val(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Policies\Microsoft\Dsh",
        "AllowNewsAndInterests",
    );
    let r3 = write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SOFTWARE\Microsoft\PolicyManager\default\NewsAndInterests\AllowNewsAndInterests",
        "value",
        1,
    );
    r1 && r2 && r3
}

fn check_disable_mouse_accel() -> bool {
    read_reg_string(HKEY_CURRENT_USER, r"Control Panel\Mouse", "MouseSpeed").as_deref()
        == Some("0")
        && read_reg_string(
            HKEY_CURRENT_USER,
            r"Control Panel\Mouse",
            "MouseThreshold1",
        )
        .as_deref()
            == Some("0")
        && read_reg_string(
            HKEY_CURRENT_USER,
            r"Control Panel\Mouse",
            "MouseThreshold2",
        )
        .as_deref()
            == Some("0")
}

fn apply_disable_mouse_accel(runner: &Runner) -> bool {
    let r1 = write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Control Panel\Mouse",
        "MouseSpeed",
        "0",
    );
    let r2 = write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Control Panel\Mouse",
        "MouseThreshold1",
        "0",
    );
    let r3 = write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Control Panel\Mouse",
        "MouseThreshold2",
        "0",
    );
    r1 && r2 && r3
}

fn restore_disable_mouse_accel(runner: &Runner) -> bool {
    let r1 = write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Control Panel\Mouse",
        "MouseSpeed",
        "1",
    );
    let r2 = write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Control Panel\Mouse",
        "MouseThreshold1",
        "6",
    );
    let r3 = write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Control Panel\Mouse",
        "MouseThreshold2",
        "10",
    );
    r1 && r2 && r3
}

fn check_disable_sticky_keys() -> bool {
    read_reg_string(
        HKEY_CURRENT_USER,
        r"Control Panel\Accessibility\StickyKeys",
        "Flags",
    )
    .as_deref()
        == Some("506")
}

fn apply_disable_sticky_keys(runner: &Runner) -> bool {
    write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Control Panel\Accessibility\StickyKeys",
        "Flags",
        "506",
    )
}

fn restore_disable_sticky_keys(runner: &Runner) -> bool {
    write_reg_string(
        runner,
        HKEY_CURRENT_USER,
        "HKCU",
        r"Control Panel\Accessibility\StickyKeys",
        "Flags",
        "510",
    )
}

fn check_disable_fast_startup() -> bool {
    read_reg_dword(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Power",
        "HiberbootEnabled",
    ) == Some(0)
}

fn apply_disable_fast_startup(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Power",
        "HiberbootEnabled",
        0,
    )
}

fn restore_disable_fast_startup(runner: &Runner) -> bool {
    write_reg_dword(
        runner,
        HKEY_LOCAL_MACHINE,
        "HKLM",
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Power",
        "HiberbootEnabled",
        1,
    )
}

// -----------------------------------------------------------------------------
// Collection complète des 18 tweaks Windows 11
// -----------------------------------------------------------------------------

static ALL_TWEAKS: &[DebloatDef] = &[
    // 1. AiAndCopilot
    DebloatDef {
        id: "disable_copilot",
        category: DebloatCategory::AiAndCopilot,
        title: "Désactiver Windows Copilot",
        description: "Masque le bouton Copilot de la barre des tâches et désactive le service Copilot (HKCU & HKLM).",
        recommended: true,
        check_fn: check_disable_copilot,
        apply_fn: apply_disable_copilot,
        restore_fn: restore_disable_copilot,
    },
    DebloatDef {
        id: "disable_ai_recall",
        category: DebloatCategory::AiAndCopilot,
        title: "Désactiver Windows AI Recall",
        description: "Bloque l'analyse des données IA en arrière-plan, la capture d'instantanés et l'activation de Recall.",
        recommended: true,
        check_fn: check_disable_ai_recall,
        apply_fn: apply_disable_ai_recall,
        restore_fn: restore_disable_ai_recall,
    },
    DebloatDef {
        id: "disable_click_to_do",
        category: DebloatCategory::AiAndCopilot,
        title: "Désactiver Click to Do IA",
        description: "Désactive la surcouche d'actions contextuelles interactives assistée par IA de Windows 11.",
        recommended: true,
        check_fn: check_disable_click_to_do,
        apply_fn: apply_disable_click_to_do,
        restore_fn: restore_disable_click_to_do,
    },
    DebloatDef {
        id: "disable_edge_notepad_paint_ai",
        category: DebloatCategory::AiAndCopilot,
        title: "Désactiver l'IA dans Edge, Bloc-notes et Paint",
        description: "Désactive la barre latérale Copilot dans Edge, la génération IA dans le Bloc-notes et Cocreator/ImageCreator dans Paint.",
        recommended: true,
        check_fn: check_disable_edge_notepad_paint_ai,
        apply_fn: apply_disable_edge_notepad_paint_ai,
        restore_fn: restore_disable_edge_notepad_paint_ai,
    },

    // 2. PrivacyAndTelemetry
    DebloatDef {
        id: "disable_telemetry",
        category: DebloatCategory::PrivacyAndTelemetry,
        title: "Désactiver la Télémétrie Windows",
        description: "Désactive la collecte de télémétrie diagnostique, l'identifiant publicitaire, TIPC et l'historique d'activités.",
        recommended: true,
        check_fn: check_disable_telemetry,
        apply_fn: apply_disable_telemetry,
        restore_fn: restore_disable_telemetry,
    },
    DebloatDef {
        id: "disable_bing_search",
        category: DebloatCategory::PrivacyAndTelemetry,
        title: "Désactiver Bing & Cortana dans la recherche",
        description: "Supprime les suggestions de recherche Web Bing et Cortana du menu Démarrer et de la barre des tâches.",
        recommended: true,
        check_fn: check_disable_bing_search,
        apply_fn: apply_disable_bing_search,
        restore_fn: restore_disable_bing_search,
    },
    DebloatDef {
        id: "disable_windows_suggestions_ads",
        category: DebloatCategory::PrivacyAndTelemetry,
        title: "Désactiver les publicités & suggestions Windows",
        description: "Bloque les recommandations d'applications, les pubs Microsoft 365 et les conseils dans Démarrer, Paramètres et Lockscreen.",
        recommended: true,
        check_fn: check_disable_windows_suggestions_ads,
        apply_fn: apply_disable_windows_suggestions_ads,
        restore_fn: restore_disable_windows_suggestions_ads,
    },
    DebloatDef {
        id: "disable_desktop_spotlight",
        category: DebloatCategory::PrivacyAndTelemetry,
        title: "Désactiver Windows Spotlight sur le Bureau",
        description: "Retire l'icône « Windows à la une » (Spotlight) et le rafraîchissement d'images imposé sur le Bureau.",
        recommended: false,
        check_fn: check_disable_desktop_spotlight,
        apply_fn: apply_disable_desktop_spotlight,
        restore_fn: restore_disable_desktop_spotlight,
    },

    // 3. ExplorerAndUi
    DebloatDef {
        id: "classic_context_menu",
        category: DebloatCategory::ExplorerAndUi,
        title: "Menu contextuel classique (Style Windows 10)",
        description: "Restaure le clic droit complet immédiat sans le sous-menu « Afficher plus d'options ».",
        recommended: true,
        check_fn: check_classic_context_menu,
        apply_fn: apply_classic_context_menu,
        restore_fn: restore_classic_context_menu,
    },
    DebloatDef {
        id: "show_file_extensions",
        category: DebloatCategory::ExplorerAndUi,
        title: "Afficher les extensions de fichiers connues",
        description: "Affiche toujours les extensions (.exe, .pdf, .docx...) dans l'Explorateur Windows pour une meilleure sécurité.",
        recommended: true,
        check_fn: check_show_file_extensions,
        apply_fn: apply_show_file_extensions,
        restore_fn: restore_show_file_extensions,
    },
    DebloatDef {
        id: "show_hidden_folders",
        category: DebloatCategory::ExplorerAndUi,
        title: "Afficher les fichiers et dossiers cachés",
        description: "Rend visibles l'ensemble des fichiers, dossiers et lecteurs cachés dans l'Explorateur Windows.",
        recommended: false,
        check_fn: check_show_hidden_folders,
        apply_fn: apply_show_hidden_folders,
        restore_fn: restore_show_hidden_folders,
    },
    DebloatDef {
        id: "launch_explorer_this_pc",
        category: DebloatCategory::ExplorerAndUi,
        title: "Ouvrir l'Explorateur sur « Ce PC »",
        description: "Configure l'Explorateur pour s'ouvrir directement sur « Ce PC » au lieu de la page Accueil / Accès rapide.",
        recommended: true,
        check_fn: check_launch_explorer_this_pc,
        apply_fn: apply_launch_explorer_this_pc,
        restore_fn: restore_launch_explorer_this_pc,
    },
    DebloatDef {
        id: "enable_dark_mode",
        category: DebloatCategory::ExplorerAndUi,
        title: "Activer le Mode Sombre Windows & Applications",
        description: "Bascule l'ensemble du système d'exploitation et des applications compatibles en mode sombre.",
        recommended: false,
        check_fn: check_enable_dark_mode,
        apply_fn: apply_enable_dark_mode,
        restore_fn: restore_enable_dark_mode,
    },

    // 4. SystemAndGaming
    DebloatDef {
        id: "enable_end_task_taskbar",
        category: DebloatCategory::SystemAndGaming,
        title: "Activer « Fin de tâche » dans la barre des tâches",
        description: "Ajoute le bouton « Fin de tâche » au clic droit sur les applications de la barre des tâches pour tuer un processus gelé.",
        recommended: true,
        check_fn: check_enable_end_task_taskbar,
        apply_fn: apply_enable_end_task_taskbar,
        restore_fn: restore_enable_end_task_taskbar,
    },
    DebloatDef {
        id: "disable_widgets",
        category: DebloatCategory::SystemAndGaming,
        title: "Désactiver les Widgets de la barre des tâches",
        description: "Supprime le bouton Widgets/Météo de la barre des tâches et désactive le service Dsh de flux d'actualités.",
        recommended: true,
        check_fn: check_disable_widgets,
        apply_fn: apply_disable_widgets,
        restore_fn: restore_disable_widgets,
    },
    DebloatDef {
        id: "disable_mouse_accel",
        category: DebloatCategory::SystemAndGaming,
        title: "Désactiver l'accélération souris (Gaming 1:1)",
        description: "Désactive « Améliorer la précision du pointeur » pour une sensibilité souris parfaitement linéaire et constante en jeu.",
        recommended: false,
        check_fn: check_disable_mouse_accel,
        apply_fn: apply_disable_mouse_accel,
        restore_fn: restore_disable_mouse_accel,
    },
    DebloatDef {
        id: "disable_sticky_keys",
        category: DebloatCategory::SystemAndGaming,
        title: "Désactiver le raccourci des touches rémanentes",
        description: "Empêche l'ouverture impromptue de la boîte de dialogue Touches rémanentes lors de pressions répétées (5x Maj).",
        recommended: true,
        check_fn: check_disable_sticky_keys,
        apply_fn: apply_disable_sticky_keys,
        restore_fn: restore_disable_sticky_keys,
    },
    DebloatDef {
        id: "disable_fast_startup",
        category: DebloatCategory::SystemAndGaming,
        title: "Désactiver le Démarrage rapide (Fast Startup)",
        description: "Assure un arrêt complet et propre du noyau Windows, évitant les instabilités matérielles et d'uptime faussé.",
        recommended: false,
        check_fn: check_disable_fast_startup,
        apply_fn: apply_disable_fast_startup,
        restore_fn: restore_disable_fast_startup,
    },
];

// -----------------------------------------------------------------------------
// Fonctions publiques de l'API Debloat
// -----------------------------------------------------------------------------

/// Retourne l'état en temps réel de tous les tweaks disponibles.
pub fn list_debloat_tweaks() -> Vec<DebloatTweakStatus> {
    ALL_TWEAKS
        .iter()
        .map(|def| DebloatTweakStatus {
            id: def.id.to_string(),
            category: def.category,
            title: def.title.to_string(),
            description: def.description.to_string(),
            recommended: def.recommended,
            is_applied: (def.check_fn)(),
        })
        .collect()
}

/// Applique (`apply = true`) ou restaure (`apply = false`) un tweak spécifique par son identifiant.
/// Respecte strictement `runner.dry_run` (simule et logge sans écrire dans le Registre).
pub fn set_debloat_tweak(runner: &Runner, id: &str, apply: bool) -> bool {
    let Some(tweak) = ALL_TWEAKS.iter().find(|t| t.id == id) else {
        runner
            .logger
            .warn(format!("Identifiant de tweak Debloat inconnu : '{id}'"));
        return false;
    };

    let action_str = if apply {
        "Application"
    } else {
        "Restauration"
    };
    runner.logger.info(format!(
        "{action_str} de l'optimisation : {} ({})",
        tweak.title, tweak.id
    ));

    let ok = if apply {
        (tweak.apply_fn)(runner)
    } else {
        (tweak.restore_fn)(runner)
    };

    if ok {
        let state_msg = if apply {
            "appliquée avec succès"
        } else {
            "restaurée aux valeurs par défaut"
        };
        runner
            .logger
            .success(format!("Optimisation '{}' {state_msg}.", tweak.title));
    } else {
        runner.logger.warn(format!(
            "Certaines clés pour '{}' n'ont pas pu être configurées (droits administrateur requis).",
            tweak.title
        ));
    }

    ok
}

/// Applique l'ensemble des optimisations marquées `recommended: true`.
pub fn apply_recommended_debloat(runner: &Runner) {
    runner
        .logger
        .section("APPLICATION DU PACK DEBLOAT WINDOWS 11 RECOMMANDÉ");

    let recommended: Vec<&DebloatDef> = ALL_TWEAKS.iter().filter(|t| t.recommended).collect();
    let total = recommended.len();
    let mut success_count = 0usize;

    for (idx, tweak) in recommended.iter().enumerate() {
        runner
            .logger
            .progress(idx + 1, total, format!("Optimisation : {}", tweak.title));
        if set_debloat_tweak(runner, tweak.id, true) {
            success_count += 1;
        }
    }

    runner.logger.success(format!(
        "Application terminée : {success_count}/{total} optimisations recommandées appliquées avec succès."
    ));
}

/// Restaure tous les paramètres aux valeurs d'origine de Windows.
pub fn restore_all_debloat(runner: &Runner) {
    runner
        .logger
        .section("RESTAURATION DE TOUS LES PARAMÈTRES WINDOWS 11 PAR DÉFAUT");

    let total = ALL_TWEAKS.len();
    let mut success_count = 0usize;

    for (idx, tweak) in ALL_TWEAKS.iter().enumerate() {
        runner
            .logger
            .progress(idx + 1, total, format!("Restauration : {}", tweak.title));
        if set_debloat_tweak(runner, tweak.id, false) {
            success_count += 1;
        }
    }

    runner.logger.success(format!(
        "Restauration terminée : {success_count}/{total} réglages rétablis aux valeurs d'origine."
    ));
}

// -----------------------------------------------------------------------------
// Tests unitaires
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::Logger;
    use std::collections::HashSet;

    #[test]
    fn test_list_debloat_tweaks_structure() {
        let tweaks = list_debloat_tweaks();

        // Vérification du nombre total de tweaks
        assert_eq!(
            tweaks.len(),
            18,
            "La liste doit contenir exactement les 18 tweaks spécifiés"
        );

        // Vérification de l'unicité des IDs
        let mut unique_ids = HashSet::new();
        for t in &tweaks {
            assert!(!t.id.is_empty(), "L'id ne doit pas être vide");
            assert!(!t.title.is_empty(), "Le titre ne doit pas être vide");
            assert!(
                !t.description.is_empty(),
                "La description ne doit pas être vide"
            );
            assert!(
                unique_ids.insert(t.id.clone()),
                "L'ID '{}' est dupliqué dans la liste des tweaks",
                t.id
            );
        }

        // Vérification que les 4 catégories sont bien couvertes
        let has_ai = tweaks
            .iter()
            .any(|t| t.category == DebloatCategory::AiAndCopilot);
        let has_privacy = tweaks
            .iter()
            .any(|t| t.category == DebloatCategory::PrivacyAndTelemetry);
        let has_explorer = tweaks
            .iter()
            .any(|t| t.category == DebloatCategory::ExplorerAndUi);
        let has_system = tweaks
            .iter()
            .any(|t| t.category == DebloatCategory::SystemAndGaming);

        assert!(has_ai, "Catégorie AiAndCopilot manquante");
        assert!(has_privacy, "Catégorie PrivacyAndTelemetry manquante");
        assert!(has_explorer, "Catégorie ExplorerAndUi manquante");
        assert!(has_system, "Catégorie SystemAndGaming manquante");
    }

    #[test]
    fn test_dry_run_set_tweak() {
        let logger = Logger::new(None, false);
        let runner = Runner::new(logger, true);

        // Application et restauration simulées de tous les tweaks
        for tweak in ALL_TWEAKS {
            assert!(
                set_debloat_tweak(&runner, tweak.id, true),
                "L'application en dry-run doit réussir pour {}",
                tweak.id
            );
            assert!(
                set_debloat_tweak(&runner, tweak.id, false),
                "La restauration en dry-run doit réussir pour {}",
                tweak.id
            );
        }

        // Tweak inexistant
        assert!(
            !set_debloat_tweak(&runner, "tweak_inexistant_xyz", true),
            "Un tweak inconnu doit retourner false"
        );
    }

    #[test]
    fn test_dry_run_batch_operations() {
        let logger = Logger::new(None, false);
        let runner = Runner::new(logger, true);

        // Ces opérations en dry-run ne doivent pas paniquer et doivent s'exécuter jusqu'au bout
        apply_recommended_debloat(&runner);
        restore_all_debloat(&runner);
    }
}
