use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
    Section,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Success => "SUCCESS",
            LogLevel::Warning => "WARNING",
            LogLevel::Error => "ERROR",
            LogLevel::Section => "SECTION",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Log(LogEntry),
    Progress {
        current: usize,
        total: usize,
        label: String,
    },
    PcDiagnosisReady(PcDiagnosis),
    PcComparisonReady(Vec<MetricDiff>),
    NetDiagnosisReady(NetDiagnosis),
    NetComparisonReady(Vec<MetricDiff>),
    StartupListUpdated(Vec<StartupItem>),
    TempAnalysisReady(Vec<TempSubfolderInfo>),
    DebloatListUpdated(Vec<DebloatTweakStatus>),
    UpdateCheckReady(Option<crate::updater::UpdateInfo>),
    WingetListReady(Vec<WingetUpgradeItem>),
    AppxListReady(Vec<AppxBloatItem>),
    GamingStatusReady(GamingStatus),
    BatterySmartReady(BatteryAndSmartInfo),
    WeeklyTaskStatusReady(bool),
    ProfileImported(WinCareProfile),
    TaskFinished {
        title: String,
        need_reboot: bool,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TempSubfolderInfo {
    pub name: String,
    pub path: String,
    pub size_mb: f64,
    pub file_count: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DebloatCategory {
    AiAndCopilot,
    PrivacyAndTelemetry,
    ExplorerAndUi,
    SystemAndGaming,
}

impl DebloatCategory {
    pub fn label(&self) -> &'static str {
        match self {
            DebloatCategory::AiAndCopilot => "🤖 IA, Copilot & Recall",
            DebloatCategory::PrivacyAndTelemetry => "🔒 Confidentialité, Pubs & Bing",
            DebloatCategory::ExplorerAndUi => "📁 Explorateur & Interface Win11",
            DebloatCategory::SystemAndGaming => "⚙ Système, Souris & Barre des tâches",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebloatTweakStatus {
    pub id: String,
    pub category: DebloatCategory,
    pub title: String,
    pub description: String,
    pub recommended: bool,
    pub is_applied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RepairTask {
    Dism,
    Sfc,
    Chkdsk,
    WindowsUpdate,
    ResetNetwork,
    ClearTemp,
    WindowsStore,
    CriticalServices,
}

impl RepairTask {
    pub const ALL: [RepairTask; 8] = [
        RepairTask::Dism,
        RepairTask::Sfc,
        RepairTask::WindowsUpdate,
        RepairTask::ResetNetwork,
        RepairTask::ClearTemp,
        RepairTask::WindowsStore,
        RepairTask::CriticalServices,
        RepairTask::Chkdsk,
    ];

    pub fn code(&self) -> u8 {
        match self {
            RepairTask::Dism => 2,
            RepairTask::Sfc => 3,
            RepairTask::Chkdsk => 4,
            RepairTask::WindowsUpdate => 5,
            RepairTask::ResetNetwork => 6,
            RepairTask::ClearTemp => 7,
            RepairTask::WindowsStore => 8,
            RepairTask::CriticalServices => 9,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RepairTask::Dism => "DISM - Réparation de l'image Windows",
            RepairTask::Sfc => "SFC - Vérification des fichiers système",
            RepairTask::Chkdsk => "CHKDSK - Vérification du disque",
            RepairTask::WindowsUpdate => "Réparation Windows Update (Services, Cache, DLLs)",
            RepairTask::ResetNetwork => "Reset Réseau (DNS, Winsock, TCP/IP, Proxy, Pare-feu)",
            RepairTask::ClearTemp => "Nettoyage des fichiers temporaires & Cleanmgr",
            RepairTask::WindowsStore => "Réparation Windows Store & Applications AppX",
            RepairTask::CriticalServices => "Vérification et réparation des services critiques",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            RepairTask::Dism => "CheckHealth, ScanHealth, RestoreHealth et nettoyage WinSxS (/StartComponentCleanup /ResetBase)",
            RepairTask::Sfc => "Analyse et répare l'intégrité des fichiers système protégés (sfc /scannow)",
            RepairTask::Chkdsk => "Vérification rapide de C: et planification optionnelle de chkdsk /F /R /X au redémarrage",
            RepairTask::WindowsUpdate => "Arrête wuauserv/bits/cryptSvc/msiserver, sauvegarde SoftwareDistribution & catroot2, réenregistre 36 DLLs et réinitialise les stratégies",
            RepairTask::ResetNetwork => "Vide le cache DNS, réinitialise Winsock, TCP/IP, proxy WinHTTP et le pare-feu Windows",
            RepairTask::ClearTemp => "Vide %TEMP%, C:\\Windows\\Temp, Prefetch, INetCache, Explorer et lance cleanmgr",
            RepairTask::WindowsStore => "Exécute wsreset.exe et réenregistre les manifestes AppX pour tous les utilisateurs",
            RepairTask::CriticalServices => "Vérifie et reconfigure wuauserv, BITS, CryptSvc, TrustedInstaller, Winmgmt, EventLog, Dhcp et Dnscache",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub ram_mb: u64,
    pub cpu_pct: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskVolumeInfo {
    pub drive: String,
    pub fs: String,
    pub size_gb: f64,
    pub free_gb: f64,
    pub free_pct: f64,
    pub media_type: String,
    pub health: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartupItem {
    pub source: String,
    pub name: String,
    pub enabled: bool,
    pub command: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceRamInfo {
    pub name: String,
    pub display_name: String,
    pub ram_mb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheSizeInfo {
    pub label: String,
    pub path: String,
    pub size_mb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PcDiagnosis {
    pub machine_model: String,
    pub os_caption: String,
    pub uptime_days: f64,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub cpu_threads: usize,
    pub cpu_usage_pct: f64,
    pub ram_total_mb: u64,
    pub ram_free_mb: u64,
    pub ram_used_pct: f64,
    pub sys_free_gb: f64,
    pub ssd_detected: bool,
    pub disks: Vec<DiskVolumeInfo>,
    pub top_processes: Vec<ProcessInfo>,
    pub startup_items: Vec<StartupItem>,
    pub auto_services: Vec<ServiceRamInfo>,
    pub scheduled_tasks_count: usize,
    pub caches: Vec<CacheSizeInfo>,
    pub cache_total_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PcCleanOptions {
    pub clean_user_temp: bool,
    pub clean_sys_temp: bool,
    pub clean_wu_download: bool,
    pub clean_wer_dumps: bool,
    pub empty_recycle_bin: bool,
    pub dism_component_cleanup: bool,
    pub run_cleanmgr: bool,
}

impl Default for PcCleanOptions {
    fn default() -> Self {
        Self {
            clean_user_temp: true,
            clean_sys_temp: true,
            clean_wu_download: true,
            clean_wer_dumps: true,
            empty_recycle_bin: true,
            dism_component_cleanup: false,
            run_cleanmgr: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PcOptimOptions {
    pub high_perf_power: bool,
    pub ultimate_perf_power: bool,
    pub reduce_visual_fx: bool,
    pub enable_storage_sense: bool,
    pub disable_game_bar: bool,
    pub disable_transparency: bool,
    pub disable_sysmain: bool,
    pub optimize_volume_trim: bool,
}

impl Default for PcOptimOptions {
    fn default() -> Self {
        Self {
            high_perf_power: true,
            ultimate_perf_power: false,
            reduce_visual_fx: true,
            enable_storage_sense: true,
            disable_game_bar: true,
            disable_transparency: false,
            disable_sysmain: false,
            optimize_volume_trim: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DnsBenchResult {
    pub resolver: String,
    pub ip: String,
    pub latency_ms: Option<u64>,
    pub ok: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetProcInfo {
    pub pid: u32,
    pub name: String,
    pub connections: usize,
    pub ram_mb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetDiagnosis {
    pub if_index: u32,
    pub adapter_name: String,
    pub adapter_desc: String,
    pub media_type: String,
    pub link_speed: String,
    pub gateway: String,
    pub current_dns: Vec<String>,
    pub is_wifi: bool,
    pub wifi_signal_pct: Option<u32>,
    pub wifi_info_lines: Vec<String>,
    pub ping_gw_ms: Option<f64>,
    pub ping_net_ms: Option<f64>,
    pub ping_loss_count: u32,
    pub uses_nextdns: bool,
    pub nextdns_mode: String,
    pub dns_results: Vec<DnsBenchResult>,
    pub best_dns: Option<DnsBenchResult>,
    pub current_mtu: u32,
    pub optimal_mtu: Option<u32>,
    pub top_net_processes: Vec<NetProcInfo>,
    pub rx_mbps: f64,
    pub tx_mbps: f64,
    pub speedtest_dl_mbps: Option<f64>,
    pub speedtest_ul_mbps: Option<f64>,
    pub speedtest_ping_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetOptimOptions {
    pub apply_best_dns: bool,
    pub apply_optimal_mtu: bool,
    pub reset_winsock_tcp: bool,
    pub flush_dns: bool,
    pub disable_wifi_power_saving: bool,
    pub disable_wu_p2p: bool,
    pub disable_smb1: bool,
    pub run_speedtest_after: bool,
}

impl Default for NetOptimOptions {
    fn default() -> Self {
        Self {
            apply_best_dns: true,
            apply_optimal_mtu: true,
            reset_winsock_tcp: false,
            flush_dns: true,
            disable_wifi_power_saving: true,
            disable_wu_p2p: true,
            disable_smb1: true,
            run_speedtest_after: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RestoreOptions {
    pub restore_power_balanced: bool,
    pub restore_visual_fx: bool,
    pub restore_transparency: bool,
    pub restore_sysmain: bool,
    pub restore_storage_sense: bool,
    pub restore_game_bar: bool,
    pub restore_dns_dhcp: bool,
    pub restore_mtu_1500: bool,
    pub restore_wu_p2p: bool,
    pub restore_wifi_power: bool,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            restore_power_balanced: true,
            restore_visual_fx: true,
            restore_transparency: true,
            restore_sysmain: true,
            restore_storage_sense: false,
            restore_game_bar: false,
            restore_dns_dhcp: true,
            restore_mtu_1500: true,
            restore_wu_p2p: false,
            restore_wifi_power: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricDiff {
    pub metric: String,
    pub before: String,
    pub after: String,
    pub delta: String,
    pub improved: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WingetUpgradeItem {
    pub name: String,
    pub id: String,
    pub current_version: String,
    pub available_version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GamingStatus {
    pub ultimate_perf_active: bool,
    pub ultimate_perf_guid: Option<String>,
    pub hags_enabled: bool,
    pub vbs_enabled: bool,
    pub game_mode_enabled: bool,
    pub shader_cache_size_mb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppxBloatItem {
    pub id: String,
    pub display_name: String,
    pub package_pattern: String,
    pub description_en: String,
    pub description_fr: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SmartDiskHealth {
    pub friendly_name: String,
    pub media_type: String,
    pub health_status: String,
    pub temperature_c: Option<u32>,
    pub wear_pct: Option<u32>,
    pub read_errors: u64,
    pub write_errors: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatteryAndSmartInfo {
    pub has_battery: bool,
    pub battery_name: String,
    pub design_capacity_mwh: u64,
    pub full_charge_capacity_mwh: u64,
    pub battery_health_pct: f64,
    pub smart_disks: Vec<SmartDiskHealth>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WinCareProfile {
    pub version: String,
    pub repair_tasks: Vec<RepairTask>,
    pub pc_clean: PcCleanOptions,
    pub pc_optim: PcOptimOptions,
    pub net_optim: NetOptimOptions,
    pub debloat_applied_ids: Vec<String>,
}