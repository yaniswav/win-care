use crate::runner::Runner;
use crate::types::BatteryAndSmartInfo;
use std::path::PathBuf;

/// Scanne l'état de santé de la batterie (WMI) et les compteurs SMART des disques physiques (PowerShell Storage).
pub fn scan_battery_and_smart(runner: &Runner) -> BatteryAndSmartInfo {
    let ps_script = r#"
$staticBat = Get-CimInstance -Namespace root\WMI -ClassName BatteryStaticData -ErrorAction SilentlyContinue | Select-Object -First 1;
$fullBat = Get-CimInstance -Namespace root\WMI -ClassName BatteryFullChargedCapacity -ErrorAction SilentlyContinue | Select-Object -First 1;
$hasBattery = $false;
$batName = '';
$designCap = [uint64]0;
$fullCap = [uint64]0;
$healthPct = [double]0.0;
if ($staticBat -and $fullBat) {
    $hasBattery = $true;
    $batName = if ($staticBat.DeviceName) { [string]$staticBat.DeviceName } else { 'Batterie interne' };
    $designCap = [uint64]$staticBat.DesignedCapacity;
    $fullCap = [uint64]$fullBat.FullChargedCapacity;
};
if (-not $hasBattery) {
    $win32Bat = Get-CimInstance -ClassName Win32_Battery -ErrorAction SilentlyContinue | Select-Object -First 1;
    if ($win32Bat) {
        $hasBattery = $true;
        $batName = if ($win32Bat.Name) { [string]$win32Bat.Name } else { if ($win32Bat.DeviceID) { [string]$win32Bat.DeviceID } else { 'Batterie' } };
        $designCap = if ($win32Bat.DesignCapacity) { [uint64]$win32Bat.DesignCapacity } else { [uint64]0 };
        $fullCap = if ($win32Bat.FullChargeCapacity) { [uint64]$win32Bat.FullChargeCapacity } else { [uint64]0 };
    };
};
if ($hasBattery -and $designCap -gt 0) {
    $healthPct = [math]::Round(([double]$fullCap / [double]$designCap) * 100.0, 1);
};
$disks = @(Get-PhysicalDisk -ErrorAction SilentlyContinue | ForEach-Object {
    $d = $_;
    $r = $d | Get-StorageReliabilityCounter -ErrorAction SilentlyContinue;
    [PSCustomObject]@{
        friendly_name = [string]$d.FriendlyName;
        media_type = [string]$d.MediaType;
        health_status = [string]$d.HealthStatus;
        temperature_c = if ($r -and $r.Temperature) { [uint32]$r.Temperature } else { $null };
        wear_pct = if ($r -and $r.Wear -ne $null) { [uint32]$r.Wear } else { $null };
        read_errors = if ($r -and $r.ReadErrorsTotal -ne $null) { [uint64]$r.ReadErrorsTotal } else { 0 };
        write_errors = if ($r -and $r.WriteErrorsTotal -ne $null) { [uint64]$r.WriteErrorsTotal } else { 0 };
    };
});
[PSCustomObject]@{
    has_battery = $hasBattery;
    battery_name = $batName;
    design_capacity_mwh = $designCap;
    full_charge_capacity_mwh = $fullCap;
    battery_health_pct = $healthPct;
    smart_disks = @($disks);
} | ConvertTo-Json -Compress -Depth 4
"#;

    let output = runner.run_ps_query(ps_script);
    let stdout = output.stdout.trim();
    if stdout.is_empty() {
        runner
            .logger
            .warn("Aucune sortie reçue du diagnostic matériel PowerShell.");
        return BatteryAndSmartInfo::default();
    }

    let json_start = stdout.find('{').unwrap_or(0);
    let json_end = stdout.rfind('}').map(|i| i + 1).unwrap_or(stdout.len());
    let clean_json = &stdout[json_start..json_end];

    let mut parsed_val: serde_json::Value = match serde_json::from_str(clean_json) {
        Ok(v) => v,
        Err(e) => {
            runner
                .logger
                .warn(format!("Échec du décodage JSON matériel : {e}"));
            return BatteryAndSmartInfo::default();
        }
    };

    if let Some(disks) = parsed_val.get_mut("smart_disks") {
        if disks.is_object() {
            *disks = serde_json::Value::Array(vec![disks.clone()]);
        } else if disks.is_null() {
            *disks = serde_json::Value::Array(Vec::new());
        }
    }

    match serde_json::from_value::<BatteryAndSmartInfo>(parsed_val) {
        Ok(info) => info,
        Err(e) => {
            runner
                .logger
                .warn(format!("Échec de mapping en BatteryAndSmartInfo : {e}"));
            BatteryAndSmartInfo::default()
        }
    }
}

/// Génère le rapport HTML officiel Windows de batterie sur le Bureau et l'ouvre dans le navigateur.
pub fn generate_battery_report(runner: &Runner) -> Option<PathBuf> {
    runner
        .logger
        .section("GÉNÉRATION DU RAPPORT DE BATTERIE");
    let user_profile =
        std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".to_string());
    let desktop_dir = PathBuf::from(user_profile).join("Desktop");
    let report_path = desktop_dir.join("WinCare_Battery_Report.html");
    let path_str = report_path.to_string_lossy().to_string();

    if runner.dry_run {
        runner.logger.info(format!(
            "[DRY-RUN] Génération simulée du rapport de batterie : {path_str}"
        ));
        return Some(report_path);
    }

    let out = runner.run_action("powercfg", &["/batteryreport", "/output", &path_str], true);
    if out.success() || report_path.exists() {
        runner
            .logger
            .success(format!("Rapport de batterie généré : {path_str}"));
        let _ = runner.run_action("cmd.exe", &["/C", "start", "", &path_str], false);
        Some(report_path)
    } else {
        runner.logger.warn(format!(
            "Échec de la génération du rapport de batterie : {}",
            out.stderr
        ));
        None
    }
}
