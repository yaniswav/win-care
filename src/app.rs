use crate::i18n::{debloat_category_title, debloat_tweak_text, repair_task_text, Lang};
use crate::logger::Logger;
use crate::runner::Runner;
use crate::types::{
    AppxBloatItem, BatteryAndSmartInfo, DebloatCategory, DebloatTweakStatus, GamingStatus,
    LogEntry, LogLevel, MetricDiff, NetDiagnosis, NetOptimOptions, PcCleanOptions, PcDiagnosis,
    PcOptimOptions, RepairTask, RestoreOptions, StartupItem, TempSubfolderInfo, WinCareProfile,
    WingetPkgState, WingetUpgradeItem, WorkerEvent,
};
use eframe::egui::{self, Color32, RichText};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActiveTab {
    Overview,
    Repair,
    BoostPc,
    BoostNet,
    Debloat,
    Apps,
    GamingHardware,
    Restore,
}

impl ActiveTab {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            ActiveTab::Overview => lang.tr("🏠 Overview", "🏠 Vue d'ensemble"),
            ActiveTab::Repair => lang.tr("🛠 Windows Repair", "🛠 Réparation Windows"),
            ActiveTab::BoostPc => lang.tr("⚡ Boost PC", "⚡ Boost PC"),
            ActiveTab::BoostNet => lang.tr("🌐 Boost Internet", "🌐 Boost Internet"),
            ActiveTab::Debloat => lang.tr("🧹 Debloat & Tweaks", "🧹 Debloat & Tweaks"),
            ActiveTab::Apps => lang.tr("📦 Apps & Bloatware", "📦 Logiciels & Bloatwares"),
            ActiveTab::GamingHardware => lang.tr("🎮 Gaming & Hardware", "🎮 Gaming & Matériel"),
            ActiveTab::Restore => lang.tr("↩ Restore (.RESTORE)", "↩ Restauration (.RESTORE)"),
        }
    }
}

/// Charge la police système Windows `Segoe UI Symbol` (`seguisym.ttf`) en fallback
/// et active l'affichage instantané des infobulles (tooltips) au survol.
fn configure_ui_and_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let sym_path = std::path::Path::new(r"C:\Windows\Fonts\seguisym.ttf");
    if let Ok(bytes) = std::fs::read(sym_path) {
        fonts.font_data.insert(
            "segoe_ui_symbol".to_owned(),
            egui::FontData::from_owned(bytes).into(),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.push("segoe_ui_symbol".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.push("segoe_ui_symbol".to_owned());
        }
        ctx.set_fonts(fonts);
    }

    // Affichage immédiat des tooltips dès que la souris survole un élément (sans délai de 0.5s)
    ctx.style_mut(|style| {
        style.interaction.tooltip_delay = 0.02;
        style.interaction.show_tooltips_only_when_still = false;
    });
}

/// Analyse une ligne de texte d'infobulle pour détecter un préfixe structuré en 3 volets.
pub(crate) fn parse_tooltip_line(line: &str) -> Option<(&'static str, Color32, &str)> {
    if let Some(r) = line.strip_prefix("▪ Default:") {
        Some(("▪ Default:", Color32::from_rgb(150, 200, 255), r))
    } else if let Some(r) = line.strip_prefix("▪ Par défaut :") {
        Some(("▪ Par défaut :", Color32::from_rgb(150, 200, 255), r))
    } else if let Some(r) = line.strip_prefix("▪ Par défaut:") {
        Some(("▪ Par défaut:", Color32::from_rgb(150, 200, 255), r))
    } else if let Some(r) = line.strip_prefix("▪ If enabled:") {
        Some(("▪ If enabled:", Color32::from_rgb(100, 235, 160), r))
    } else if let Some(r) = line.strip_prefix("▪ Si utilisé :") {
        Some(("▪ Si utilisé :", Color32::from_rgb(100, 235, 160), r))
    } else if let Some(r) = line.strip_prefix("▪ Si utilisé:") {
        Some(("▪ Si utilisé:", Color32::from_rgb(100, 235, 160), r))
    } else if let Some(r) = line.strip_prefix("▪ If skipped:") {
        Some(("▪ If skipped:", Color32::from_rgb(235, 190, 110), r))
    } else if let Some(r) = line.strip_prefix("▪ Si non utilisé :") {
        Some(("▪ Si non utilisé :", Color32::from_rgb(235, 190, 110), r))
    } else if let Some(r) = line.strip_prefix("▪ Si non utilisé:") {
        Some(("▪ Si non utilisé:", Color32::from_rgb(235, 190, 110), r))
    } else {
        None
    }
}

/// Affiche une infobulle riche et structurée (Titre + Explication + Niveau de sécurité).
fn show_rich_tooltip(ui: &mut egui::Ui, title: &str, details: &str, safety: &str, is_warning: bool) {
    ui.set_max_width(480.0);
    ui.label(
        RichText::new(title)
            .strong()
            .color(Color32::from_rgb(100, 210, 255)),
    );
    ui.separator();
    for line in details.lines() {
        let (prefix, color, rest) = match parse_tooltip_line(line) {
            Some(res) => res,
            None => {
                ui.label(line);
                ui.add_space(2.0);
                continue;
            }
        };

        let rest_formatted = if !rest.is_empty() && !rest.starts_with(' ') {
            format!(" {}", rest)
        } else {
            rest.to_string()
        };

        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = 480.0;
        RichText::new(prefix)
            .strong()
            .color(color)
            .append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::LEFT);
        RichText::new(rest_formatted)
            .append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::LEFT);
        ui.label(job);
        ui.add_space(2.0);
    }
    ui.add_space(4.0);
    let safety_color = if is_warning {
        Color32::from_rgb(255, 200, 80)
    } else {
        Color32::from_rgb(90, 230, 130)
    };
    ui.label(RichText::new(safety).color(safety_color).strong());
}

/// Affiche une case à cocher accompagnée d'un badge `[?]` visuel et d'une infobulle riche instantanée.
fn checkbox_with_help(
    ui: &mut egui::Ui,
    checked: &mut bool,
    label: &str,
    title: &str,
    details: &str,
    safety: &str,
    is_warning: bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let cb = ui.checkbox(checked, label).on_hover_ui(|ui| {
            show_rich_tooltip(ui, title, details, safety, is_warning);
        });
        if cb.changed() {
            changed = true;
        }
        ui.label(
            RichText::new("[?]")
                .color(Color32::from_rgb(95, 195, 250))
                .small()
                .strong(),
        )
        .on_hover_ui(|ui| {
            show_rich_tooltip(ui, title, details, safety, is_warning);
        });
    });
    changed
}

pub struct WinCareApp {
    pub is_admin: bool,
    pub dry_run: bool,
    pub lang: Lang,
    active_tab: ActiveTab,

    // Canal de communication avec les threads d'exécution
    rx: Receiver<WorkerEvent>,
    is_busy: bool,
    progress_current: usize,
    progress_total: usize,
    progress_label: String,
    need_reboot_banner: bool,

    // Journal en temps réel
    logs: Vec<LogEntry>,
    log_file_path: PathBuf,
    auto_scroll_logs: bool,

    // Onglet Repair
    selected_repairs: HashSet<RepairTask>,
    schedule_chkdsk_reboot: bool,

    // Onglet Boost PC & Temp Analyzer
    pc_diag: Option<PcDiagnosis>,
    pc_clean_opts: PcCleanOptions,
    pc_optim_opts: PcOptimOptions,
    pc_diffs: Vec<MetricDiff>,
    startup_items: Vec<StartupItem>,
    temp_subfolders: Vec<TempSubfolderInfo>,

    // Onglet Boost Internet
    net_diag: Option<NetDiagnosis>,
    net_optim_opts: NetOptimOptions,
    net_diffs: Vec<MetricDiff>,
    include_speedtest: bool,

    // Onglet Debloat Win11
    debloat_tweaks: Vec<DebloatTweakStatus>,

    // Onglet Apps & Bloatware
    winget_items: Vec<WingetUpgradeItem>,
    selected_winget_ids: HashSet<String>,
    winget_has_scanned: bool,
    appx_items: Vec<AppxBloatItem>,
    selected_appx_patterns: HashSet<String>,

    // Onglet Gaming & Hardware
    gaming_status: Option<GamingStatus>,
    battery_smart: Option<BatteryAndSmartInfo>,

    // Maintenance planifiée
    weekly_task_enabled: bool,

    // Auto-update GitHub
    update_info: Option<crate::updater::UpdateInfo>,

    // Onglet Restore
    restore_opts: RestoreOptions,
    session_logger: Logger,
    active_runner: std::sync::Arc<std::sync::Mutex<Option<Runner>>>,
}

struct TaskFinishedGuard {
    runner: Runner,
    title: String,
    pub need_reboot: bool,
}

impl Drop for TaskFinishedGuard {
    fn drop(&mut self) {
        self.runner.logger.send_event(WorkerEvent::TaskFinished {
            title: self.title.clone(),
            need_reboot: self.need_reboot,
        });
    }
}

impl WinCareApp {
    pub fn new(ctx: &egui::Context, is_admin: bool, dry_run: bool) -> Self {
        configure_ui_and_fonts(ctx);

        let lang = Lang::detect();
        let (tx, rx) = mpsc::channel();
        let init_logger = Logger::new(Some(tx.clone()), false);
        let log_file_path = init_logger.log_path.clone();

        let ready_msg = match lang {
            Lang::En => {
                format!("WinCare v{} ready. Hover over any option or [?] badge to view its detailed tooltip.", env!("CARGO_PKG_VERSION"))
            }
            Lang::Fr => {
                format!("WinCare v{} prêt. Survolez n'importe quelle option ou badge [?] pour afficher son infobulle détaillée.", env!("CARGO_PKG_VERSION"))
            }
        };
        init_logger.info(&ready_msg);
        if !is_admin {
            init_logger.warn(match lang {
                Lang::En => "Running without Administrator privileges: some repairs require UAC elevation.",
                Lang::Fr => "Exécuté sans droits Administrateur : certaines réparations nécessitent une élévation UAC.",
            });
        }
        if dry_run {
            init_logger.info(match lang {
                Lang::En => "Simulation mode (--dry-run) active: no system modifications will be applied.",
                Lang::Fr => "Mode Simulation (--dry-run) actif : aucune modification système ne sera appliquée.",
            });
        }

        let mut selected_repairs = HashSet::new();
        for task in RepairTask::ALL {
            selected_repairs.insert(task);
        }

        let startup_items = crate::boost_pc::list_startup_items();
        let debloat_tweaks = crate::debloat::list_debloat_tweaks();
        let appx_items = crate::appx::get_default_bloatware_list();
        let weekly_task_enabled = false;

        // Requêtes en arrière-plan au démarrage pour charger la configuration Gaming et la tâche planifiée
        {
            let runner_init = init_logger.clone();
            let r_init = Runner::new(runner_init, dry_run);
            std::thread::spawn(move || {
                let status = crate::gaming::query_gaming_status(&r_init);
                r_init.logger.send_event(WorkerEvent::GamingStatusReady(status));
                let weekly_ok = crate::scheduler::is_weekly_task_enabled();
                r_init.logger.send_event(WorkerEvent::WeeklyTaskStatusReady(weekly_ok));
            });
        }

        Self {
            is_admin,
            dry_run,
            lang,
            active_tab: ActiveTab::Overview,
            rx,
            is_busy: false,
            progress_current: 0,
            progress_total: 0,
            progress_label: String::new(),
            need_reboot_banner: false,
            logs: Vec::new(),
            log_file_path,
            auto_scroll_logs: true,
            selected_repairs,
            schedule_chkdsk_reboot: false,
            pc_diag: None,
            pc_clean_opts: PcCleanOptions::default(),
            pc_optim_opts: PcOptimOptions::default(),
            pc_diffs: Vec::new(),
            startup_items,
            temp_subfolders: Vec::new(),
            net_diag: None,
            net_optim_opts: NetOptimOptions::default(),
            net_diffs: Vec::new(),
            include_speedtest: false,
            debloat_tweaks,
            winget_items: Vec::new(),
            selected_winget_ids: HashSet::new(),
            winget_has_scanned: false,
            appx_items,
            selected_appx_patterns: HashSet::new(),
            gaming_status: None,
            battery_smart: None,
            weekly_task_enabled,
            update_info: None,
            restore_opts: RestoreOptions::default(),
            session_logger: init_logger,
            active_runner: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn make_runner(&self) -> Runner {
        let r = Runner::new(self.session_logger.clone(), self.dry_run);
        if let Ok(mut guard) = self.active_runner.lock() {
            *guard = Some(r.clone());
        }
        r
    }

    fn cancel_current_task(&mut self) {
        if let Ok(guard) = self.active_runner.lock() {
            if let Some(r) = guard.as_ref() {
                r.cancel();
            }
        }
        self.progress_label = self
            .lang
            .tr("⏹ Cancelling current operation...", "⏹ Annulation de l'opération en cours...")
            .to_string();
    }

    fn poll_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WorkerEvent::Log(entry) => {
                    self.logs.push(entry);
                }
                WorkerEvent::Progress {
                    current,
                    total,
                    label,
                } => {
                    self.progress_current = current;
                    self.progress_total = total;
                    self.progress_label = label;
                }
                WorkerEvent::PcDiagnosisReady(diag) => {
                    self.startup_items = diag.startup_items.clone();
                    self.pc_diag = Some(diag);
                }
                WorkerEvent::PcComparisonReady(diffs) => {
                    self.pc_diffs = diffs;
                }
                WorkerEvent::NetDiagnosisReady(diag) => {
                    self.net_diag = Some(diag);
                }
                WorkerEvent::NetComparisonReady(diffs) => {
                    self.net_diffs = diffs;
                }
                WorkerEvent::StartupListUpdated(items) => {
                    self.startup_items = items;
                }
                WorkerEvent::TempAnalysisReady(folders) => {
                    self.temp_subfolders = folders;
                }
                WorkerEvent::DebloatListUpdated(tweaks) => {
                    self.debloat_tweaks = tweaks;
                }
                WorkerEvent::UpdateCheckReady(info) => {
                    self.update_info = info;
                }
                WorkerEvent::WingetListReady(items) => {
                    self.winget_has_scanned = true;
                    self.selected_winget_ids.clear();
                    for item in &items {
                        self.selected_winget_ids.insert(item.id.clone());
                    }
                    self.winget_items = items;
                }
                WorkerEvent::WingetItemStatus { id, state } => {
                    if let Some(item) = self.winget_items.iter_mut().find(|it| it.id == id) {
                        item.state = state.clone();
                    }
                    if matches!(state, WingetPkgState::Updated) {
                        self.selected_winget_ids.remove(&id);
                    }
                }
                WorkerEvent::AppxListReady(items) => {
                    self.selected_appx_patterns.clear();
                    for item in &items {
                        if item.installed {
                            self.selected_appx_patterns.insert(item.package_pattern.clone());
                        }
                    }
                    self.appx_items = items;
                }
                WorkerEvent::GamingStatusReady(status) => {
                    self.gaming_status = Some(status);
                }
                WorkerEvent::BatterySmartReady(info) => {
                    self.battery_smart = Some(info);
                }
                WorkerEvent::WeeklyTaskStatusReady(enabled) => {
                    self.weekly_task_enabled = enabled;
                }
                WorkerEvent::ProfileImported(prof) => {
                    self.selected_repairs = prof.repair_tasks.into_iter().collect();
                    self.pc_clean_opts = prof.pc_clean;
                    self.pc_optim_opts = prof.pc_optim;
                    self.net_optim_opts = prof.net_optim;
                    let runner = self.make_runner();
                    for id in &prof.debloat_applied_ids {
                        crate::debloat::set_debloat_tweak(&runner, id, true);
                    }
                    self.debloat_tweaks = crate::debloat::list_debloat_tweaks();
                }
                WorkerEvent::TaskFinished { title, need_reboot } => {
                    self.is_busy = false;
                    self.progress_label = format!("{} {title}", self.lang.tr("Finished:", "Terminé :"));
                    if need_reboot {
                        self.need_reboot_banner = true;
                    }
                }
            }
        }
    }

    fn spawn_check_update(&mut self, install_if_available: bool) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Checking for updates on GitHub...",
                "Vérification des mises à jour sur GitHub...",
            )
            .to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("GitHub Update Check", "Vérification MAJ GitHub").to_string(),
                need_reboot: false,
            };
            let info = crate::updater::check_for_update(&runner);
            if install_if_available {
                if let Some(ref upd) = info {
                    if upd.update_available {
                        crate::updater::perform_self_update(&runner, upd);
                    }
                }
            }
            runner
                .logger
                .send_event(WorkerEvent::UpdateCheckReady(info));
        });
    }

    fn spawn_create_restore_point(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Creating Windows System Restore Point...",
                "Création d'un Point de Restauration Windows...",
            )
            .to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("System Restore Point", "Point de Restauration").to_string(),
                need_reboot: false,
            };
            let desc = format!(
                "WinCare Backup ({})",
                chrono::Local::now().format("%Y-%m-%d %H:%M")
            );
            crate::boost_pc::create_system_restore_point(&runner, &desc);
        });
    }

    fn spawn_temp_scan(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Scanning largest %TEMP% subfolders...",
                "Analyse des sous-dossiers les plus lourds dans %TEMP%...",
            )
            .to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("%TEMP% Analysis", "Analyse %TEMP%").to_string(),
                need_reboot: false,
            };
            let items = crate::boost_pc::scan_top_temp_folders(&runner);
            runner.logger.send_event(WorkerEvent::TempAnalysisReady(items));
        });
    }

    fn spawn_delete_temp_paths(&mut self, paths: Vec<String>) {
        if self.is_busy || paths.is_empty() {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = paths.len();
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Deleting selected temporary folders...",
                "Suppression des dossiers temporaires sélectionnés...",
            )
            .to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Targeted %TEMP% Purge", "Purge ciblée %TEMP%").to_string(),
                need_reboot: false,
            };
            let total = paths.len();
            for (idx, p) in paths.iter().enumerate() {
                runner.logger.progress(
                    idx + 1,
                    total,
                    l.tr(
                        &format!("Deleting {p}..."),
                        &format!("Suppression de {p}..."),
                    ),
                );
                crate::boost_pc::delete_temp_target(&runner, p);
            }
            let updated = crate::boost_pc::scan_top_temp_folders(&runner);
            runner
                .logger
                .send_event(WorkerEvent::TempAnalysisReady(updated));
        });
    }

    fn spawn_full_diagnose(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = 3;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Full diagnosis in progress...",
                "Diagnostic complet en cours...",
            )
            .to_string();

        let runner = self.make_runner();
        let speedtest = self.include_speedtest;
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Full Diagnosis", "Diagnostic complet").to_string(),
                need_reboot: false,
            };
            runner.logger.progress(
                1,
                3,
                l.tr(
                    "Scanning PC (Hardware, RAM, Disks, Caches, Startup)...",
                    "Analyse du PC (Hardware, RAM, Disques, Caches, Startup)...",
                ),
            );
            let pc = crate::boost_pc::run_pc_diagnosis(&runner);
            runner.logger.send_event(WorkerEvent::PcDiagnosisReady(pc));

            runner.logger.progress(
                2,
                3,
                l.tr(
                    "Identifying largest folders in %TEMP%...",
                    "Identification des plus gros dossiers dans %TEMP%...",
                ),
            );
            let top_temp = crate::boost_pc::scan_top_temp_folders(&runner);
            runner
                .logger
                .send_event(WorkerEvent::TempAnalysisReady(top_temp));

            runner.logger.progress(
                3,
                3,
                l.tr(
                    "Analyzing Internet connection (Route, Ping, DNS, MTU)...",
                    "Analyse de la connexion Internet (Route, Ping, DNS, MTU)...",
                ),
            );
            let net = crate::boost_net::run_net_diagnosis(&runner, speedtest);
            runner.logger.send_event(WorkerEvent::NetDiagnosisReady(net));
        });
    }

    fn spawn_repair(&mut self, tasks: Vec<RepairTask>, schedule_chkdsk: bool) {
        if self.is_busy || tasks.is_empty() {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = tasks.len();
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Windows repair in progress...",
                "Réparation Windows en cours...",
            )
            .to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let mut guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Windows Repair", "Réparation Windows").to_string(),
                need_reboot: false,
            };
            let need_reboot =
                crate::repair::execute_repair_tasks(&runner, &tasks, schedule_chkdsk);
            guard.need_reboot = need_reboot;
        });
    }

    fn spawn_pc_diagnose(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 2;
        let l = self.lang;
        self.progress_label = l
            .tr("PC diagnosis in progress...", "Analyse du PC en cours...")
            .to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("PC Diagnosis", "Diagnostic PC").to_string(),
                need_reboot: false,
            };
            runner.logger.progress(
                1,
                2,
                l.tr(
                    "Hardware & system diagnosis...",
                    "Diagnostic matériel & système...",
                ),
            );
            let diag = crate::boost_pc::run_pc_diagnosis(&runner);
            runner.logger.send_event(WorkerEvent::PcDiagnosisReady(diag));

            runner.logger.progress(
                2,
                2,
                l.tr(
                    "Scanning largest %TEMP% subfolders...",
                    "Scan des plus gros sous-dossiers %TEMP%...",
                ),
            );
            let top_temp = crate::boost_pc::scan_top_temp_folders(&runner);
            runner
                .logger
                .send_event(WorkerEvent::TempAnalysisReady(top_temp));
        });
    }

    fn spawn_pc_boost(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = 4;
        let l = self.lang;
        self.progress_label = l
            .tr("PC Boost in progress...", "Boost PC en cours...")
            .to_string();

        let runner = self.make_runner();
        let clean_opts = self.pc_clean_opts.clone();
        let optim_opts = self.pc_optim_opts.clone();
        let existing_diag = self.pc_diag.clone();

        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("PC Boost", "Boost PC").to_string(),
                need_reboot: false,
            };
            runner.logger.progress(
                1,
                4,
                l.tr(
                    "Measuring initial PC state...",
                    "Mesure de l'état initial du PC...",
                ),
            );
            let before =
                existing_diag.unwrap_or_else(|| crate::boost_pc::run_pc_diagnosis(&runner));
            runner
                .logger
                .send_event(WorkerEvent::PcDiagnosisReady(before.clone()));

            runner.logger.progress(
                2,
                4,
                l.tr(
                    "Cleaning caches and temporary files...",
                    "Nettoyage des caches et fichiers temporaires...",
                ),
            );
            crate::boost_pc::run_pc_cleaning(&runner, &clean_opts);

            runner.logger.progress(
                3,
                4,
                l.tr(
                    "Applying system optimizations...",
                    "Application des optimisations système...",
                ),
            );
            crate::boost_pc::run_pc_optimizations(&runner, &optim_opts, &before);

            runner.logger.progress(
                4,
                4,
                l.tr(
                    "Calculating Before / After comparison...",
                    "Calcul du comparatif Avant / Après...",
                ),
            );
            let diffs = crate::boost_pc::compute_pc_comparison(&runner, &before);
            runner
                .logger
                .send_event(WorkerEvent::PcComparisonReady(diffs));

            let after = crate::boost_pc::run_pc_diagnosis(&runner);
            runner.logger.send_event(WorkerEvent::PcDiagnosisReady(after));

            let top_temp = crate::boost_pc::scan_top_temp_folders(&runner);
            runner
                .logger
                .send_event(WorkerEvent::TempAnalysisReady(top_temp));
        });
    }

    fn spawn_net_diagnose(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Network & DNS diagnosis in progress...",
                "Diagnostic réseau & DNS en cours...",
            )
            .to_string();

        let runner = self.make_runner();
        let speedtest = self.include_speedtest;
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Internet Diagnosis", "Diagnostic Internet").to_string(),
                need_reboot: false,
            };
            let diag = crate::boost_net::run_net_diagnosis(&runner, speedtest);
            runner.logger.send_event(WorkerEvent::NetDiagnosisReady(diag));
        });
    }

    fn spawn_net_boost(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = 3;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Internet optimization in progress...",
                "Optimisation Internet en cours...",
            )
            .to_string();

        let runner = self.make_runner();
        let optim_opts = self.net_optim_opts.clone();
        let existing_diag = self.net_diag.clone();
        let speedtest = self.include_speedtest;

        std::thread::spawn(move || {
            let mut guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Boost Internet", "Boost Internet").to_string(),
                need_reboot: false,
            };
            runner.logger.progress(
                1,
                3,
                l.tr(
                    "Initial network diagnosis...",
                    "Diagnostic réseau initial...",
                ),
            );
            let before = existing_diag
                .unwrap_or_else(|| crate::boost_net::run_net_diagnosis(&runner, speedtest));
            runner
                .logger
                .send_event(WorkerEvent::NetDiagnosisReady(before.clone()));

            runner.logger.progress(
                2,
                3,
                l.tr(
                    "Applying network optimizations...",
                    "Application des optimisations réseau...",
                ),
            );
            let need_reboot =
                crate::boost_net::run_net_optimizations(&runner, &optim_opts, &before);
            guard.need_reboot = need_reboot;

            runner.logger.progress(
                3,
                3,
                l.tr(
                    "Comparing Before / After...",
                    "Comparaison Avant / Après...",
                ),
            );
            let diffs = crate::boost_net::compute_net_comparison(
                &runner,
                &before,
                optim_opts.run_speedtest_after,
            );
            runner
                .logger
                .send_event(WorkerEvent::NetComparisonReady(diffs));
        });
    }

    fn spawn_debloat_batch(&mut self, recommended_only: bool) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = if recommended_only {
            l.tr(
                "Applying Recommended Windows 11 Debloat Pack...",
                "Application du Pack Debloat Windows 11 Recommandé...",
            )
            .to_string()
        } else {
            l.tr(
                "Restoring Windows 11 default settings...",
                "Restauration des paramètres Windows 11 par défaut...",
            )
            .to_string()
        };

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Windows 11 Debloat", "Debloat Windows 11").to_string(),
                need_reboot: false,
            };
            if recommended_only {
                crate::debloat::apply_recommended_debloat(&runner);
            } else {
                crate::debloat::restore_all_debloat(&runner);
            }
            let updated = crate::debloat::list_debloat_tweaks();
            runner
                .logger
                .send_event(WorkerEvent::DebloatListUpdated(updated));
        });
    }

    fn spawn_restore(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Restoring default settings...",
                "Restauration des paramètres par défaut...",
            )
            .to_string();

        let runner = self.make_runner();
        let opts = self.restore_opts.clone();
        let if_index = self.net_diag.as_ref().map(|d| d.if_index);
        let adapter_name = self.net_diag.as_ref().map(|d| d.adapter_name.clone());

        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("System Restore", "Restauration système").to_string(),
                need_reboot: false,
            };
            crate::restore::execute_restore(&runner, &opts, if_index, adapter_name.as_deref());
        });
    }

    fn spawn_export_profile(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = l
            .tr("Exporting profile...", "Exportation du profil...")
            .to_string();
        let runner = self.make_runner();
        let applied_debloat: Vec<String> = self
            .debloat_tweaks
            .iter()
            .filter(|t| t.is_applied)
            .map(|t| t.id.clone())
            .collect();
        let prof = WinCareProfile {
            version: env!("CARGO_PKG_VERSION").to_string(),
            repair_tasks: self.selected_repairs.iter().copied().collect(),
            pc_clean: self.pc_clean_opts.clone(),
            pc_optim: self.pc_optim_opts.clone(),
            net_optim: self.net_optim_opts.clone(),
            debloat_applied_ids: applied_debloat,
        };
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Profile Export", "Export profil").to_string(),
                need_reboot: false,
            };
            crate::profile::export_profile(&runner, &prof);
        });
    }

    fn spawn_import_profile(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = l
            .tr("Importing profile...", "Importation du profil...")
            .to_string();
        let runner = self.make_runner();
        let default_path = crate::profile::default_profile_path();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Profile Import", "Import profil").to_string(),
                need_reboot: false,
            };
            if let Some(prof) = crate::profile::import_profile(&runner, &default_path) {
                runner.logger.send_event(WorkerEvent::ProfileImported(prof));
            }
        });
    }

    fn spawn_toggle_weekly_task(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let target = !self.weekly_task_enabled;
        let l = self.lang;
        self.progress_label = if target {
            l.tr(
                "Activating weekly maintenance task...",
                "Activation de la tâche de maintenance hebdomadaire...",
            )
            .to_string()
        } else {
            l.tr(
                "Disabling weekly maintenance task...",
                "Désactivation de la tâche de maintenance hebdomadaire...",
            )
            .to_string()
        };
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l
                    .tr(
                        "Weekly Maintenance Task",
                        "Tâche de maintenance hebdomadaire",
                    )
                    .to_string(),
                need_reboot: false,
            };
            let ok = crate::scheduler::set_weekly_task(&runner, target);
            if ok {
                runner
                    .logger
                    .send_event(WorkerEvent::WeeklyTaskStatusReady(target));
            } else {
                let current = crate::scheduler::is_weekly_task_enabled();
                runner
                    .logger
                    .send_event(WorkerEvent::WeeklyTaskStatusReady(current));
            }
        });
    }

    fn spawn_scan_winget(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.winget_has_scanned = true;
        self.progress_current = 1;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Scanning for software upgrades via WinGet...",
                "Recherche des mises à jour logicielles via WinGet...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("WinGet Scan", "Scan WinGet").to_string(),
                need_reboot: false,
            };
            let items = crate::winget::scan_winget_upgrades(&runner);
            runner.logger.send_event(WorkerEvent::WingetListReady(items));
        });
    }

    fn spawn_upgrade_winget(&mut self, ids: Vec<String>) {
        if self.is_busy || ids.is_empty() {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = ids.len();
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Upgrading selected packages via WinGet...",
                "Mise à jour des paquets sélectionnés via WinGet...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("WinGet Upgrade", "Mise à jour WinGet").to_string(),
                need_reboot: false,
            };
            crate::winget::upgrade_winget_packages(&runner, &ids);
        });
    }

    fn spawn_upgrade_all_winget(&mut self) {
        if self.is_busy {
            return;
        }
        if !self.winget_items.is_empty() {
            let all_ids: Vec<String> = self
                .winget_items
                .iter()
                .filter(|it| it.state != WingetPkgState::Updated)
                .map(|it| it.id.clone())
                .collect();
            for id in &all_ids {
                self.selected_winget_ids.insert(id.clone());
            }
            if !all_ids.is_empty() {
                self.spawn_upgrade_winget(all_ids);
            }
            return;
        }

        self.is_busy = true;
        self.winget_has_scanned = true;
        self.progress_current = 0;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Scanning & upgrading all packages via WinGet...",
                "Recherche et mise à jour de tous les logiciels via WinGet...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l
                    .tr("WinGet Upgrade All", "Mise à jour globale WinGet")
                    .to_string(),
                need_reboot: false,
            };
            let items = crate::winget::scan_winget_upgrades(&runner);
            runner
                .logger
                .send_event(WorkerEvent::WingetListReady(items.clone()));
            let all_ids: Vec<String> = items.into_iter().map(|it| it.id).collect();
            if !all_ids.is_empty() {
                crate::winget::upgrade_winget_packages(&runner, &all_ids);
            }
        });
    }

    fn spawn_scan_appx(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Scanning installed Windows AppX bloatware...",
                "Analyse des Bloatwares Windows AppX installés...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("AppX Bloatware Scan", "Scan Bloatwares AppX").to_string(),
                need_reboot: false,
            };
            let items = crate::appx::scan_appx_bloatware(&runner);
            runner.logger.send_event(WorkerEvent::AppxListReady(items));
        });
    }

    fn spawn_remove_appx(&mut self, patterns: Vec<String>) {
        if self.is_busy || patterns.is_empty() {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = patterns.len();
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Uninstalling selected Windows AppX bloatware...",
                "Désinstallation des Bloatwares Windows AppX sélectionnés...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l
                    .tr("AppX Bloatware Removal", "Suppression Bloatwares AppX")
                    .to_string(),
                need_reboot: false,
            };
            crate::appx::remove_appx_packages(&runner, &patterns);
            let items = crate::appx::scan_appx_bloatware(&runner);
            runner.logger.send_event(WorkerEvent::AppxListReady(items));
        });
    }

    fn spawn_refresh_gaming_status(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Querying gaming configuration and shader caches...",
                "Vérification des réglages gaming et caches shaders...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Gaming Status", "État Gaming").to_string(),
                need_reboot: false,
            };
            let status = crate::gaming::query_gaming_status(&runner);
            runner.logger.send_event(WorkerEvent::GamingStatusReady(status));
        });
    }

    fn spawn_clean_shader_caches(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Purging GPU shader caches (NVIDIA, AMD, DirectX)...",
                "Purge des caches de shaders GPU (NVIDIA, AMD, DirectX)...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l
                    .tr("GPU Shader Caches Purge", "Purge Caches Shaders GPU")
                    .to_string(),
                need_reboot: false,
            };
            crate::gaming::clean_gpu_shader_caches(&runner);
            let status = crate::gaming::query_gaming_status(&runner);
            runner.logger.send_event(WorkerEvent::GamingStatusReady(status));
        });
    }

    fn spawn_enable_ultimate_plan(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Activating Ultimate Performance power scheme...",
                "Activation du mode Performances Ultimes...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l
                    .tr("Ultimate Performance Scheme", "Mode Performances Ultimes")
                    .to_string(),
                need_reboot: false,
            };
            crate::gaming::enable_ultimate_power_plan(&runner);
            let status = crate::gaming::query_gaming_status(&runner);
            runner.logger.send_event(WorkerEvent::GamingStatusReady(status));
        });
    }

    fn spawn_set_hags(&mut self, enabled: bool) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = if enabled {
            l.tr("Enabling HAGS...", "Activation de HAGS...").to_string()
        } else {
            l.tr("Disabling HAGS...", "Désactivation de HAGS...").to_string()
        };
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("HAGS Setting", "Réglage HAGS").to_string(),
                need_reboot: true,
            };
            crate::gaming::set_hags(&runner, enabled);
            let status = crate::gaming::query_gaming_status(&runner);
            runner.logger.send_event(WorkerEvent::GamingStatusReady(status));
        });
    }

    fn spawn_set_vbs(&mut self, enabled: bool) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = if enabled {
            l.tr("Enabling VBS...", "Activation de VBS...").to_string()
        } else {
            l.tr("Disabling VBS...", "Désactivation de VBS...").to_string()
        };
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l
                    .tr(
                        "VBS / Memory Integrity Setting",
                        "Réglage VBS / Intégrité mémoire",
                    )
                    .to_string(),
                need_reboot: true,
            };
            crate::gaming::set_vbs(&runner, enabled);
            let status = crate::gaming::query_gaming_status(&runner);
            runner.logger.send_event(WorkerEvent::GamingStatusReady(status));
        });
    }

    fn spawn_set_game_mode(&mut self, enabled: bool) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = if enabled {
            l.tr(
                "Enabling Windows Game Mode...",
                "Activation du Mode Jeu Windows...",
            )
            .to_string()
        } else {
            l.tr(
                "Disabling Windows Game Mode...",
                "Désactivation du Mode Jeu Windows...",
            )
            .to_string()
        };
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Windows Game Mode", "Mode Jeu Windows").to_string(),
                need_reboot: false,
            };
            crate::gaming::set_game_mode(&runner, enabled);
            let status = crate::gaming::query_gaming_status(&runner);
            runner.logger.send_event(WorkerEvent::GamingStatusReady(status));
        });
    }

    fn spawn_scan_battery_smart(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Scanning battery health and disk S.M.A.R.T. counters...",
                "Diagnostic de la santé batterie et compteurs S.M.A.R.T. disques...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l
                    .tr("Battery & S.M.A.R.T. Diagnosis", "Diagnostic Batterie & SMART")
                    .to_string(),
                need_reboot: false,
            };
            let info = crate::hardware::scan_battery_and_smart(&runner);
            runner.logger.send_event(WorkerEvent::BatterySmartReady(info));
        });
    }

    fn spawn_generate_battery_report(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        let l = self.lang;
        self.progress_label = l
            .tr(
                "Generating Windows battery report on Desktop...",
                "Génération du rapport de batterie sur le Bureau...",
            )
            .to_string();
        let runner = self.make_runner();
        std::thread::spawn(move || {
            let _guard = TaskFinishedGuard {
                runner: runner.clone(),
                title: l.tr("Battery Report", "Rapport de batterie").to_string(),
                need_reboot: false,
            };
            crate::hardware::generate_battery_report(&runner);
        });
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new(format!("⚡ WinCare v{}", env!("CARGO_PKG_VERSION")))
                    .strong()
                    .color(Color32::from_rgb(90, 200, 250)),
            );
            ui.label(
                RichText::new(l.tr(
                    "— Repair, Boost & Win11 Debloat",
                    "— Réparation, Boost & Debloat Win11",
                ))
                .weak(),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Bouton de bascule de langue bien visible
                let lang_btn_text = match self.lang {
                    Lang::En => "🌐 [EN] | FR",
                    Lang::Fr => "🌐 EN | [FR]",
                };
                if ui
                    .button(RichText::new(lang_btn_text).strong().color(Color32::from_rgb(110, 220, 255)))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Language / Langue", "Langue / Language"),
                            l.tr(
                                "Click to switch interface language between English and French instantly.",
                                "Cliquez pour basculer instantanément la langue de l'interface entre le français et l'anglais.",
                            ),
                            l.tr("✔ Instant switch", "✔ Bascule instantanée"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.lang = self.lang.toggle();
                }

                ui.separator();

                ui.checkbox(&mut self.dry_run, l.tr("Simulation (Dry-Run)", "Mode Simulation (Dry-Run)"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Simulation Mode (--dry-run)", "Mode Simulation (--dry-run)"),
                            l.tr(
                                "When enabled, WinCare will not perform any changes on your system: it only logs the commands that would be executed.",
                                "Quand cette case est cochée, WinCare n'effectue aucune modification sur votre PC : il affiche uniquement dans le journal en bas les commandes qui auraient été lancées.",
                            ),
                            l.tr(
                                "✔ Ideal for safe testing without touching the system",
                                "✔ Idéal pour tester l'outil sans rien toucher au système",
                            ),
                            false,
                        );
                    });

                ui.separator();

                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("★ Create Restore Point", "★ Créer Point de Restauration")),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Windows System Restore Point", "Point de Restauration Système Windows"),
                            l.tr(
                                "Calls Checkpoint-Computer to create an official Windows restore point before applying changes.",
                                "Appelle Checkpoint-Computer pour créer un point de restauration officiel Windows avant vos modifications.",
                            ),
                            l.tr(
                                "✔ Recommended before cleaning or debloating",
                                "✔ Recommandé avant un gros nettoyage ou debloat",
                            ),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_create_restore_point();
                }

                ui.separator();

                let upd_available = self
                    .update_info
                    .as_ref()
                    .map(|u| u.update_available)
                    .unwrap_or(false);
                if upd_available {
                    let ver = self
                        .update_info
                        .as_ref()
                        .map(|u| u.latest_version.clone())
                        .unwrap_or_default();
                    let btn_text = match l {
                        Lang::En => format!("⬆ Update v{ver}"),
                        Lang::Fr => format!("⬆ Installer MAJ v{ver}"),
                    };
                    if ui
                        .add_enabled(
                            !self.is_busy,
                            egui::Button::new(
                                RichText::new(btn_text)
                                    .color(Color32::from_rgb(90, 235, 135))
                                    .strong(),
                            ),
                        )
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Install GitHub Update", "Installer la mise à jour GitHub"),
                                l.tr(
                                    "Downloads new WinCare.exe release from GitHub Releases, checks its executable signature and updates the current binary.",
                                    "Télécharge automatiquement la nouvelle version de WinCare.exe depuis GitHub Releases, vérifie sa signature exécutable et remplace le fichier actuel.",
                                ),
                                l.tr(
                                    "✔ Official release from yaniswav/win-care",
                                    "✔ Mise à jour officielle depuis yaniswav/win-care",
                                ),
                                false,
                            );
                        })
                        .clicked()
                    {
                        self.spawn_check_update(true);
                    }
                } else if ui
                    .add_enabled(!self.is_busy, egui::Button::new(l.tr("↻ Check Updates", "↻ Vérifier MAJ")))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Check for Updates (GitHub Releases)", "Vérifier les mises à jour (GitHub Releases)"),
                            l.tr(
                                "Queries https://github.com/yaniswav/win-care/releases/latest to see if a newer version of WinCare.exe is available.",
                                "Interroge https://github.com/yaniswav/win-care/releases/latest pour savoir si une version plus récente de WinCare.exe est disponible.",
                            ),
                            l.tr(
                                "✔ No personal data is sent",
                                "✔ Aucune donnée personnelle envoyée",
                            ),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_check_update(false);
                }

                ui.separator();

                if self.is_admin {
                    ui.label(
                        RichText::new(l.tr("✔ ADMIN (UAC)", "✔ Administrateur"))
                            .color(Color32::from_rgb(80, 220, 120))
                            .strong(),
                    )
                    .on_hover_text(l.tr(
                        "WinCare is running with full Administrator privileges.",
                        "WinCare tourne avec les privilèges Administrateur complets.",
                    ));
                } else if ui
                    .button(
                        RichText::new(l.tr(
                            "⚠ NON-ADMIN — Relaunch as Admin",
                            "⚠ Mode Standard — Relancer en Admin",
                        ))
                        .color(Color32::from_rgb(255, 200, 70)),
                    )
                    .on_hover_text(l.tr(
                        "Click here to relaunch WinCare with Windows Administrator prompt (UAC), required for DISM, SFC and system repairs.",
                        "Cliquez ici pour relancer WinCare avec l'invite Administrateur Windows (UAC), requise pour DISM, SFC et le nettoyage système.",
                    ))
                    .clicked()
                {
                    if crate::relaunch_as_admin() {
                        std::process::exit(0);
                    }
                }
            });
        });

        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::Overview,
                ActiveTab::Overview.label(l),
            )
            .on_hover_text(l.tr(
                "Overview and 1-click launch for diagnostics or complete repairs.",
                "Vue d'ensemble et lancement en 1 clic des diagnostics ou réparations complètes.",
            ));
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::Repair,
                ActiveTab::Repair.label(l),
            )
            .on_hover_text(l.tr(
                "The 8 Windows system repair tools (DISM, SFC, Windows Update, Network, Store, Services, CHKDSK).",
                "Les 8 outils de réparation système Windows (DISM, SFC, Windows Update, Réseau, Store, Services, CHKDSK).",
            ));
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::BoostPc,
                ActiveTab::BoostPc.label(l),
            )
            .on_hover_text(l.tr(
                "Cache cleanup, top %TEMP% subfolders analyzer, performance optimizations, and startup programs management.",
                "Nettoyage des caches, analyseur des plus gros dossiers %TEMP%, optimisations de performance et gestion des programmes au démarrage.",
            ));
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::BoostNet,
                ActiveTab::BoostNet.label(l),
            )
            .on_hover_text(l.tr(
                "DNS servers speed benchmark (with NextDNS protection), optimal MTU calculation, and Wi-Fi / TCP optimization.",
                "Benchmark de vitesse des serveurs DNS (avec protection NextDNS), calcul du MTU optimal et optimisation Wi-Fi / TCP.",
            ));
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::Debloat,
                ActiveTab::Debloat.label(l),
            )
            .on_hover_text(l.tr(
                "Disable Copilot, Windows Recall, Bing, Telemetry, Widgets, and restore classic Windows 10 context menu.",
                "Désactivation de Copilot, Windows Recall, Bing, Télémétrie, Widgets et activation du clic-droit classique Windows 10.",
            ));
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::Apps,
                ActiveTab::Apps.label(l),
            )
            .on_hover_text(l.tr(
                "Winget package updater and pre-installed Windows AppX bloatware uninstaller.",
                "Mise à jour des logiciels via WinGet et désinstallation des Bloatwares AppX préinstallés.",
            ));
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::GamingHardware,
                ActiveTab::GamingHardware.label(l),
            )
            .on_hover_text(l.tr(
                "Gaming performance tweaks, GPU shader caches cleaner, battery health, and disk SMART monitoring.",
                "Optimisations gaming, nettoyage des caches shaders GPU, santé batterie et surveillance SMART des disques.",
            ));
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::Restore,
                ActiveTab::Restore.label(l),
            )
            .on_hover_text(l.tr(
                "Restore all settings changed by Boost PC or Boost Internet back to factory defaults.",
                "Permet de remettre à l'état d'origine tous les réglages modifiés par Boost PC ou Boost Internet.",
            ));
        });

        if self.is_busy || !self.progress_label.is_empty() {
            ui.add_space(4.0);
            let frac = if self.progress_total > 0 {
                (self.progress_current as f32 / self.progress_total as f32).clamp(0.0, 1.0)
            } else if self.is_busy {
                0.5
            } else {
                1.0
            };
            ui.horizontal(|ui| {
                if self.is_busy {
                    if ui
                        .button(
                            RichText::new(l.tr("⏹ Cancel", "⏹ Annuler"))
                                .color(Color32::from_rgb(255, 110, 110))
                                .strong(),
                        )
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Cancel Active Operation", "Annuler l'opération en cours"),
                                l.tr(
                                    "Immediately stops the running command or WinGet upgrade.\n▪ Default: Task runs until completion.\n▪ If enabled: Terminates the active child process (e.g. winget.exe) and aborts remaining queued items.\n▪ If skipped: Operation continues normally.",
                                    "Arrête immédiatement la commande ou la mise à jour WinGet en cours.\n▪ Par défaut : La tâche s'exécute jusqu'à la fin.\n▪ Si utilisé : Stoppe immédiatement le sous-processus actif (ex: winget.exe) et annule les éléments restants.\n▪ Si non utilisé : L'opération se poursuit normalement.",
                                ),
                                l.tr("✔ Safe immediate stop", "✔ Arrêt immédiat sécurisé"),
                                true,
                            );
                        })
                        .clicked()
                    {
                        self.cancel_current_task();
                    }
                }
                ui.add(
                    egui::ProgressBar::new(frac)
                        .text(&self.progress_label)
                        .animate(self.is_busy),
                );
            });
        }

        if self.need_reboot_banner {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(l.tr(
                        "⚠ A Windows restart is recommended to complete repairs.",
                        "⚠ Un redémarrage de Windows est recommandé pour finaliser les réparations.",
                    ))
                    .color(Color32::from_rgb(255, 190, 60))
                    .strong(),
                );
                if ui
                    .small_button(l.tr("Dismiss", "Masquer"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Dismiss Banner", "Masquer la bannière"),
                            l.tr(
                                "Hides the restart notification banner.\n▪ Default: Banner is visible while reboot is recommended.\n▪ If enabled: Closes the notification banner until next repair.\n▪ If skipped: Banner remains visible as a reminder.",
                                "Masque le bandeau d'alerte de redémarrage.\n▪ Par défaut : Le bandeau est affiché tant qu'un redémarrage est conseillé.\n▪ Si utilisé : Ferme ce bandeau jusqu'à la prochaine opération.\n▪ Si non utilisé : Le bandeau reste affiché en rappel.",
                            ),
                            l.tr(
                                "✔ Does not restart or alter Windows state",
                                "✔ Ne modifie aucun paramètre de Windows",
                            ),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.need_reboot_banner = false;
                }
            });
        }
    }

    fn render_overview_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.heading(l.tr("Dashboard & Quick Actions", "Tableau de bord & Actions rapides"));
        ui.label(l.tr(
            "💡 Tip: Hover over any option or [?] badge to view its detailed explanation instantly.",
            "💡 Astuce : Survolez n'importe quelle option ou icône [?] pour afficher instantanément son explication détaillée.",
        ));
        ui.add_space(8.0);

        ui.columns(4, |cols| {
            cols[0].group(|ui| {
                ui.heading(
                    RichText::new(l.tr("▶ Full Diagnosis", "▶ Diagnostic Complet"))
                        .color(Color32::from_rgb(100, 210, 255)),
                );
                ui.label(l.tr(
                    "Read-only scan: PC info + top %TEMP% folders + Internet connection (Route, Ping, DNS, MTU).",
                    "Analyse PC + Top dossiers %TEMP% + Connexion Internet (Route, Ping, DNS, MTU) en lecture seule.",
                ));
                ui.add_space(4.0);
                checkbox_with_help(
                    ui,
                    &mut self.include_speedtest,
                    l.tr("Include Ookla Speedtest", "Inclure Speedtest Ookla"),
                    l.tr("Ookla Speedtest Bandwidth Test", "Test de débit Ookla Speedtest"),
                    l.tr(
                        "▪ Default: Bandwidth speed testing is skipped during diagnosis to save time.\n▪ If enabled: Runs speedtest.exe to measure real Download and Upload bandwidth.\n▪ If skipped: Only ping, routing, DNS benchmark, and MTU will be evaluated.",
                        "▪ Par défaut : Le test de bande passante est ignoré lors du diagnostic pour un résultat rapide.\n▪ Si utilisé : Exécute speedtest.exe pour mesurer votre vitesse réelle de téléchargement (Download) et d'envoi (Upload).\n▪ Si non utilisé : Seuls le ping, la route, le benchmark DNS et le MTU sont évalués.",
                    ),
                    l.tr("ℹ Adds ~25 seconds to diagnosis", "ℹ Ajoute environ 25 secondes au diagnostic"),
                    false,
                );
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("▶ Run Full Diagnosis", "▶ Lancer le Diagnostic Complet")),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Full Diagnosis (100% Read-Only)", "Diagnostic Complet (100 % Lecture Seule)"),
                            l.tr(
                                "Scans hardware, RAM, CPU, disk free space, 9 cache folders, top %TEMP% subfolders, startup programs, ping, DNS servers, and MTU.",
                                "Analyse votre matériel, la RAM, le CPU, l'espace disque, les 9 dossiers de caches, les sous-dossiers les plus lourds de %TEMP%, les programmes au démarrage, le ping, les serveurs DNS et le MTU.",
                            ),
                            l.tr("✔ 100% safe: does not modify any files or settings", "✔ 100 % sans danger : ne modifie aucun fichier ni réglage"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_full_diagnose();
                }
            });

            cols[1].group(|ui| {
                ui.heading(
                    RichText::new("⚙ Repair-Windows-Auto")
                        .color(Color32::from_rgb(120, 230, 140)),
                );
                ui.label(l.tr(
                    "Automatically runs all 8 system repairs (DISM, SFC, Windows Update, Network, Temp, Store, Services, CHKDSK).",
                    "Exécute automatiquement les 8 réparations système (DISM, SFC, Windows Update, Réseau, Temp, Store, Services, CHKDSK).",
                ));
                ui.add_space(4.0);
                ui.label(RichText::new(l.tr("Live % streaming progress.", "Streaming en direct de la progression %.")).weak());
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("▶ Full Auto Repair", "▶ Réparation Auto Complète")),
                    )
                    .on_hover_ui(|ui| {
                        let repair_labels: Vec<&str> = RepairTask::ALL
                            .into_iter()
                            .map(|t| repair_task_text(l, t).0)
                            .collect();
                        let details = match l {
                            Lang::En => format!("Sequentially executes: 1. {}, 2. {}, 3. {}, 4. {}, 5. {}, 6. {}, 7. {}, 8. {}.",
                                repair_labels[0], repair_labels[1], repair_labels[2], repair_labels[3],
                                repair_labels[4], repair_labels[5], repair_labels[6], repair_labels[7]),
                            Lang::Fr => format!("Enchaîne automatiquement : 1. {}, 2. {}, 3. {}, 4. {}, 5. {}, 6. {}, 7. {}, 8. {}.",
                                repair_labels[0], repair_labels[1], repair_labels[2], repair_labels[3],
                                repair_labels[4], repair_labels[5], repair_labels[6], repair_labels[7]),
                        };
                        show_rich_tooltip(
                            ui,
                            l.tr("Full Automatic Repair (Repair-Windows-Auto)", "Réparation Automatique Complète (Repair-Windows-Auto)"),
                            &details,
                            l.tr("⚠ Takes 20 to 45 minutes (you may lock PC with Win+L)", "⚠ Dure 20 à 45 minutes (vous pouvez verrouiller le PC avec Win+L)"),
                            true,
                        );
                    })
                    .clicked()
                {
                    self.spawn_repair(RepairTask::ALL.to_vec(), true);
                }
            });

            cols[2].group(|ui| {
                ui.heading(
                    RichText::new(l.tr("⚡ Safe Boost Pack", "⚡ Pack Boost Safe")).color(Color32::from_rgb(255, 200, 90)),
                );
                ui.label(l.tr(
                    "Safe cleanup (%TEMP%, caches) and recommended PC + Internet optimizations with Before/After diff.",
                    "Nettoyage sécurisé (%TEMP%, caches) et optimisations recommandées PC + Internet avec diff Avant/Après.",
                ));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.is_busy, egui::Button::new(l.tr("⚡ Boost PC", "⚡ Boost PC")))
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Safe PC Boost", "Boost PC Safe"),
                                l.tr(
                                    "Cleans selected temp files and caches, applies Windows responsiveness optimizations, and shows Before / After diff.",
                                    "Vide les fichiers temporaires et caches sélectionnés, applique les optimisations de réactivité Windows et affiche le comparatif Avant / Après.",
                                ),
                                l.tr("✔ All optimizations can be reversed in Restore tab", "✔ Toutes les optimisations sont réversibles dans l'onglet Restauration"),
                                false,
                            );
                        })
                        .clicked()
                    {
                        self.active_tab = ActiveTab::BoostPc;
                        self.spawn_pc_boost();
                    }
                    if ui
                        .add_enabled(!self.is_busy, egui::Button::new(l.tr("🌐 Boost Net", "🌐 Boost Net")))
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Safe Internet Boost", "Boost Internet Safe"),
                                l.tr(
                                    "Benchmarks DNS resolvers, applies fastest (unless NextDNS active), sets optimal unfragmented MTU, flushes DNS cache and optimizes Wi-Fi adapter.",
                                    "Teste les résolveurs DNS, applique le plus rapide (sauf si NextDNS est actif), règle le MTU optimal sans fragmentation, vide le cache DNS et optimise la carte Wi-Fi.",
                                ),
                                l.tr("✔ Automatically preserves NextDNS if detected", "✔ Préserve automatiquement NextDNS s'il est détecté"),
                                false,
                            );
                        })
                        .clicked()
                    {
                        self.active_tab = ActiveTab::BoostNet;
                        self.spawn_net_boost();
                    }
                });
            });

            cols[3].group(|ui| {
                ui.heading(
                    RichText::new(l.tr("★ Win11 Debloat", "★ Win11 Debloat")).color(Color32::from_rgb(210, 140, 255)),
                );
                let active_count = self.debloat_tweaks.iter().filter(|t| t.is_applied).count();
                let debloat_sub_text = match l {
                    Lang::En => format!(
                        "Disables Copilot, Recall, Telemetry, Bing, Widgets, and enables classic right-click ({active_count}/{} active).",
                        self.debloat_tweaks.len()
                    ),
                    Lang::Fr => format!(
                        "Désactive Copilot, Recall, Télémétrie, Bing, Widgets et active le clic-droit classique ({active_count}/{} actifs).",
                        self.debloat_tweaks.len()
                    ),
                };
                ui.label(debloat_sub_text);
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("★ Apply Recommended Debloat Pack", "★ Appliquer Pack Debloat Recommandé")),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Recommended Windows 11 Debloat Pack", "Pack Debloat Windows 11 Recommandé"),
                            l.tr(
                                "Applies recommended tweaks: disables Copilot, Recall, Click to Do, AI in Edge/Paint/Notepad, Telemetry, Bing search, Win11 ads, Widgets, Sticky Keys, and enables Win10 classic right-click + file extensions + End Task.",
                                "Active les 13 tweaks recommandés : désactive Copilot, Recall, Click to Do, IA Edge/Paint/Notepad, Télémétrie, Bing dans Démarrer, Pubs Win11, Widgets, Touches rémanentes 5x Maj, et active le menu clic-droit classique Win10 + extensions de fichiers + Fin de tâche.",
                            ),
                            l.tr("✔ Reversible anytime with 1 click", "✔ Annulable à tout moment en 1 clic"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.active_tab = ActiveTab::Debloat;
                    self.spawn_debloat_batch(true);
                }
            });
        });

        ui.add_space(12.0);
        ui.separator();
        ui.heading(l.tr("Detected System State Summary", "Résumé de l'état détecté"));

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(RichText::new(l.tr("• System, Hardware & Temp Storage", "• Système, Matériel & Stockage Temporaire")).strong());
                if let Some(d) = &self.pc_diag {
                    ui.label(format!("{}: {}", l.tr("Model", "Modèle"), d.machine_model));
                    ui.label(format!("OS: {} ({} {:.1} {})", d.os_caption, l.tr("Uptime:", "Uptime :"), d.uptime_days, l.tr("d", "j")));
                    ui.label(format!(
                        "CPU: {} ({} {} / {} threads) — {}: {:.1}%",
                        d.cpu_name, d.cpu_cores, l.tr("cores", "cœurs"), d.cpu_threads, l.tr("Load", "Charge"), d.cpu_usage_pct
                    ));
                    ui.label(format!(
                        "RAM: {} {} / {} MB ({:.1}% {})",
                        d.ram_free_mb, l.tr("MB free", "Mo libres"), d.ram_total_mb, d.ram_used_pct, l.tr("used", "utilisée")
                    ));
                    ui.label(format!(
                        "{} : {:.2} GB | SSD : {}",
                        l.tr("System disk free", "Disque système libre"),
                        d.sys_free_gb,
                        if d.ssd_detected { l.tr("Yes", "Oui") } else { l.tr("No", "Non") }
                    ));
                    ui.label(
                        RichText::new(format!(
                            "{}: {} MB ({:.2} GB)",
                            l.tr("Cleanable caches detected", "Caches nettoyables détectés"),
                            d.cache_total_mb,
                            d.cache_total_mb as f64 / 1024.0
                        ))
                        .color(Color32::from_rgb(255, 200, 80)),
                    );
                } else {
                    ui.label(
                        RichText::new(l.tr("No PC diagnosis run in this session.", "Aucun diagnostic PC exécuté dans cette session.")).weak(),
                    );
                }
            });

            cols[1].group(|ui| {
                ui.label(RichText::new(l.tr("• Network & Connectivity", "• Réseau & Connectivité")).strong());
                if let Some(n) = &self.net_diag {
                    ui.label(format!(
                        "{}: {} ({})",
                        l.tr("Active adapter", "Carte active"), n.adapter_name, n.link_speed
                    ));
                    ui.label(format!(
                        "{}: {} | {}: {} ({}: {:?})",
                        l.tr("Gateway", "Passerelle"), n.gateway,
                        l.tr("Current MTU", "MTU actuel"), n.current_mtu,
                        l.tr("optimal", "optimal"), n.optimal_mtu
                    ));
                    ui.label(format!("{}: {}", l.tr("Current DNS", "DNS actuels"), n.current_dns.join(", ")));
                    if n.uses_nextdns {
                        ui.label(
                            RichText::new(format!("✔ NextDNS {} ({})", l.tr("active", "actif"), n.nextdns_mode))
                                .color(Color32::from_rgb(220, 130, 255)),
                        );
                    }
                    if let Some(best) = &n.best_dns {
                        ui.label(format!(
                            "{}: {} ({}) {} {:?} ms",
                            l.tr("Fastest DNS", "DNS le plus rapide"),
                            best.resolver, best.ip, l.tr("in", "en"), best.latency_ms
                        ));
                    }
                    ui.label(format!(
                        "Ping 1.1.1.1 : {:?} ms ({}: {}/4)",
                        n.ping_net_ms, l.tr("losses", "pertes"), n.ping_loss_count
                    ));
                } else {
                    ui.label(
                        RichText::new(l.tr("No Internet diagnosis run in this session.", "Aucun diagnostic Internet exécuté dans cette session."))
                            .weak(),
                    );
                }
            });
        });

        ui.add_space(10.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new(l.tr(
                        "⏰ Scheduled Maintenance & Config Profile (.JSON)",
                        "⏰ Maintenance Automatique & Profil (.JSON)",
                    ))
                    .color(Color32::from_rgb(120, 210, 255)),
                );
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let switch_text = if self.weekly_task_enabled {
                    RichText::new(l.tr(
                        "⏰ Weekly Maintenance (Sunday 12:00) [ON]",
                        "⏰ Maintenance Hebdomadaire (Dimanche 12h) [ON]",
                    ))
                    .color(Color32::from_rgb(80, 225, 130))
                    .strong()
                } else {
                    RichText::new(l.tr(
                        "⏰ Weekly Maintenance (Sunday 12:00) [OFF]",
                        "⏰ Maintenance Hebdomadaire (Dimanche 12h) [OFF]",
                    ))
                    .color(Color32::from_rgb(180, 180, 180))
                };

                let task_tooltip_title = l.tr(
                    "Scheduled Weekly Maintenance (Sunday 12:00)",
                    "Maintenance Hebdomadaire Automatique (Dimanche 12h00)",
                );
                let task_tooltip_details = l.tr(
                    "▪ Default: No automatic maintenance task is registered in Windows Task Scheduler.\n▪ If enabled: Schedules a silent background maintenance every Sunday at 12:00 PM (runs --boost-pc without elevation prompt).\n▪ If skipped: Maintenance must be run manually from WinCare whenever desired.",
                    "▪ Par défaut : Aucune tâche planifiée automatique n'est enregistrée dans le Planificateur Windows.\n▪ Si utilisé : Enregistre une tâche hebdomadaire silencieuse chaque dimanche à 12h00 (lance --boost-pc automatiquement en tâche de fond).\n▪ Si non utilisé : Vous effectuez les maintenances manuellement depuis WinCare quand vous le souhaitez.",
                );
                let task_tooltip_safety = l.tr(
                    "✔ Runs silently in the background with normal privileges",
                    "✔ Exécution silencieuse et autonome avec privilèges normaux",
                );

                if ui
                    .add_enabled(!self.is_busy, egui::Button::new(switch_text))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            task_tooltip_title,
                            task_tooltip_details,
                            task_tooltip_safety,
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_toggle_weekly_task();
                }

                ui.label(
                    RichText::new("[?]")
                        .color(Color32::from_rgb(95, 195, 250))
                        .small()
                        .strong(),
                )
                .on_hover_ui(|ui| {
                    show_rich_tooltip(
                        ui,
                        task_tooltip_title,
                        task_tooltip_details,
                        task_tooltip_safety,
                        false,
                    );
                });

                ui.separator();

                let export_tooltip_title =
                    l.tr("Export Configuration Profile", "Exporter le profil de configuration");
                let export_tooltip_details = l.tr(
                    "▪ Default: Settings and tweak selections are only preserved in memory during current session.\n▪ If enabled: Exports current repair choices, PC clean & optim options, network settings, and active debloat tweaks to Desktop\\wincare-profile.json.\n▪ If skipped: Settings are not saved to an external JSON configuration file.",
                    "▪ Par défaut : Les sélections d'options et de tweaks sont conservées uniquement en mémoire pendant la session.\n▪ Si utilisé : Exporte vos choix de réparations, options de nettoyage, optimisation réseau et tweaks debloat actifs vers Bureau\\wincare-profile.json.\n▪ Si non utilisé : Les réglages ne sont pas sauvegardés dans un fichier JSON externe.",
                );
                let export_tooltip_safety = l.tr(
                    "✔ Portable JSON backup file for rapid deployment on other PCs",
                    "✔ Fichier JSON facilement transférable pour configurer rapidement d'autres machines",
                );

                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr(
                            "💾 Export my profile (Desktop\\wincare-profile.json)",
                            "💾 Exporter mon profil (Desktop\\wincare-profile.json)",
                        )),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            export_tooltip_title,
                            export_tooltip_details,
                            export_tooltip_safety,
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_export_profile();
                }

                ui.label(
                    RichText::new("[?]")
                        .color(Color32::from_rgb(95, 195, 250))
                        .small()
                        .strong(),
                )
                .on_hover_ui(|ui| {
                    show_rich_tooltip(
                        ui,
                        export_tooltip_title,
                        export_tooltip_details,
                        export_tooltip_safety,
                        false,
                    );
                });

                let import_tooltip_title =
                    l.tr("Import Configuration Profile", "Importer un profil de configuration");
                let import_tooltip_details = l.tr(
                    "▪ Default: Retains current in-app selections and active system settings.\n▪ If enabled: Loads and restores repair tasks, PC clean/optim options, network options, and automatically applies debloat tweaks from Desktop\\wincare-profile.json.\n▪ If skipped: Retains currently loaded configuration without loading from disk.",
                    "▪ Par défaut : Conserve les réglages actuellement configurés dans l'interface.\n▪ Si utilisé : Charge et restaure les tâches de réparation, options PC/réseau et réapplique automatiquement les tweaks debloat listés dans Bureau\\wincare-profile.json.\n▪ Si non utilisé : Conserve la configuration active sans modifier les cases cochées.",
                );
                let import_tooltip_safety = l.tr(
                    "✔ Instantly applies all saved tweaks and options",
                    "✔ Restaure instantanément l'intégralité de vos options favorites",
                );

                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr(
                            "📂 Import a profile (Desktop\\wincare-profile.json)",
                            "📂 Importer un profil (Desktop\\wincare-profile.json)",
                        )),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            import_tooltip_title,
                            import_tooltip_details,
                            import_tooltip_safety,
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_import_profile();
                }

                ui.label(
                    RichText::new("[?]")
                        .color(Color32::from_rgb(95, 195, 250))
                        .small()
                        .strong(),
                )
                .on_hover_ui(|ui| {
                    show_rich_tooltip(
                        ui,
                        import_tooltip_title,
                        import_tooltip_details,
                        import_tooltip_safety,
                        false,
                    );
                });
            });
        });
    }

    fn render_repair_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.horizontal(|ui| {
            ui.heading(l.tr(
                "Windows Repair (Interactive & Auto — Real-Time Streaming)",
                "Réparation Windows (Interactif & Auto — Streaming Temps Réel)",
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(l.tr("Uncheck all", "Tout décocher"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Uncheck All Repairs", "Tout décocher"),
                            l.tr(
                                "Clears the selection of all repair tasks.\n▪ Default: All repairs are selected.\n▪ If enabled: Deselects all repair tools so you can pick only specific ones.\n▪ If skipped: Keeps current selection intact.",
                                "Désélectionne l'ensemble des tâches de réparation.\n▪ Par défaut : Toutes les réparations sont cochées.\n▪ Si utilisé : Décoche tous les outils afin de choisir uniquement les réparations souhaitées.\n▪ Si non utilisé : Conserve la sélection actuelle.",
                            ),
                            l.tr("✔ UI selection only, does not affect system", "✔ Modification d'affichage uniquement"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.selected_repairs.clear();
                }
                if ui
                    .button(l.tr("Check all", "Tout cocher"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Check All Repairs", "Tout cocher"),
                            l.tr(
                                "Selects all 8 Windows system repair tasks.\n▪ Default: All repairs selected for complete health check.\n▪ If enabled: Re-selects DISM, SFC, Windows Update, Network, Store, Services and CHKDSK.\n▪ If skipped: Keeps current selection intact.",
                                "Sélectionne l'ensemble des 8 outils de réparation système.\n▪ Par défaut : Toutes les réparations sont sélectionnées pour une remise en état complète.\n▪ Si utilisé : Recoche DISM, SFC, Windows Update, Réseau, Store, Services et CHKDSK.\n▪ Si non utilisé : Conserve la sélection actuelle.",
                            ),
                            l.tr("✔ UI selection only, does not affect system", "✔ Modification d'affichage uniquement"),
                            false,
                        );
                    })
                    .clicked()
                {
                    for t in RepairTask::ALL {
                        self.selected_repairs.insert(t);
                    }
                }
            });
        });
        ui.label(l.tr(
            "Select system repair tools to run in order (hover [?] for details):",
            "Sélectionnez les outils de réparation système à exécuter dans l'ordre (survolez [?] pour les détails) :",
        ));
        ui.add_space(6.0);

        for task in RepairTask::ALL {
            ui.push_id(task.code(), |ui| {
                let (short_label, subtitle, tooltip_body, tooltip_safety) = repair_task_text(l, task);
                let mut checked = self.selected_repairs.contains(&task);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        if checkbox_with_help(
                            ui,
                            &mut checked,
                            &format!("[{}] {}", task.code(), short_label),
                            short_label,
                            tooltip_body,
                            tooltip_safety,
                            false,
                        ) {
                            if checked {
                                self.selected_repairs.insert(task);
                            } else {
                                self.selected_repairs.remove(&task);
                            }
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(!self.is_busy, egui::Button::new(l.tr("Run alone", "Exécuter seul")))
                                .on_hover_text(l.tr(
                                    "Immediately runs only this repair task.",
                                    "Lance uniquement cette réparation tout de suite.",
                                ))
                                .clicked()
                            {
                                self.spawn_repair(vec![task], self.schedule_chkdsk_reboot);
                            }
                        });
                    });
                    ui.label(RichText::new(subtitle).weak());

                    if task == RepairTask::Chkdsk {
                        checkbox_with_help(
                            ui,
                            &mut self.schedule_chkdsk_reboot,
                            l.tr(
                                "Also schedule a full CHKDSK (chkdsk C: /F /R /X) upon next restart",
                                "Planifier également un CHKDSK complet (chkdsk C: /F /R /X) au prochain redémarrage",
                            ),
                            l.tr(
                                "Schedule CHKDSK /F /R /X on Next Reboot",
                                "Planification CHKDSK /F /R /X au redémarrage",
                            ),
                            l.tr(
                                "Schedules a deep integrity check of drive C: filesystem and bad sectors upon next Windows startup.\n▪ Default: No boot check scheduled (normal fast boot).\n▪ If enabled: Windows will scan drive C: at startup, fixing directory errors and recovering bad sectors.\n▪ If skipped: Drive C: is only checked online in read-only mode (scan).",
                                "Programme une vérification complète du système de fichiers et des secteurs physiques du disque C: lors du prochain démarrage de Windows.\n▪ Par défaut : Aucun scan au boot (démarrage rapide préservé).\n▪ Si utilisé : Windows analysera et réparera le disque C: au démarrage, corrigeant les clusters défectueux.\n▪ Si non utilisé : Le disque C: est uniquement vérifié en ligne en lecture seule (scan).",
                            ),
                            l.tr(
                                "⚠ Next PC boot will take 15 to 45 minutes while scanning the drive",
                                "⚠ Le prochain démarrage du PC prendra 15 à 45 minutes pendant l'analyse du disque",
                            ),
                            true,
                        );
                    }
                });
            });
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let ordered: Vec<RepairTask> = RepairTask::ALL
                .into_iter()
                .filter(|t| self.selected_repairs.contains(t))
                .collect();

            let run_btn_text = match l {
                Lang::En => format!("▶ Run {} Selected Repairs", ordered.len()),
                Lang::Fr => format!("▶ Lancer les {} réparations sélectionnées", ordered.len()),
            };

            if ui
                .add_enabled(
                    !self.is_busy && !ordered.is_empty(),
                    egui::Button::new(
                        RichText::new(run_btn_text)
                            .strong(),
                    ),
                )
                .on_hover_ui(|ui| {
                    show_rich_tooltip(
                        ui,
                        l.tr("Launch Windows Repairs", "Lancement des réparations Windows"),
                        l.tr(
                            "Executes all checked repair tasks in sequential order.\n▪ Default: Runs native Windows maintenance utilities (DISM, SFC, Windows Update, Store, Services, CHKDSK).\n▪ If enabled: Repairs corrupted system files, resets component store, and fixes system services.\n▪ If skipped: No repairs are executed.",
                            "Exécute toutes les réparations cochées de manière séquentielle.\n▪ Par défaut : Lance les utilitaires officiels de maintenance Windows (DISM, SFC, Windows Update, Store, Services, CHKDSK).\n▪ Si utilisé : Répare les fichiers système corrompus, réinitialise le magasin des composants et relance les services vitaux.\n▪ Si non utilisé : Aucune réparation n'est effectuée.",
                        ),
                        l.tr("✔ Safe official Windows maintenance commands", "✔ Commandes officielles de maintenance Windows sans risque"),
                        false,
                    );
                })
                .clicked()
            {
                self.spawn_repair(ordered, self.schedule_chkdsk_reboot);
            }
        });
    }

    fn render_boost_pc_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.horizontal(|ui| {
            ui.heading(l.tr(
                "Boost PC — Analysis, %TEMP% Cleanup, Optimizations & Startup",
                "Boost PC — Analyse, Nettoyage %TEMP%, Optimisations & Startup",
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(
                            RichText::new(l.tr("▶ Apply Cleanup & Optimizations", "▶ Appliquer Nettoyage & Optimisations")).strong(),
                        ),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Apply Cleanup & Optimizations", "Appliquer Nettoyage & Optimisations"),
                            l.tr(
                                "Executes all checked options in '1. Cleanup Options' and '2. System Optimizations', then calculates RAM and disk space freed.",
                                "Exécute toutes les cases cochées dans les colonnes « 1. Options de Nettoyage » et « 2. Optimisations Système », puis calcule le gain de RAM et d'espace disque.",
                            ),
                            l.tr("✔ Respects all your checked boxes strictly", "✔ Respecte scrupuleusement vos cases cochées"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_pc_boost();
                }
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("▶ Scan PC & %TEMP%", "▶ Analyser le PC & %TEMP%")),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("PC Diagnosis & %TEMP% Scan (Read-only)", "Diagnostic PC & Scan %TEMP% (Lecture seule)"),
                            l.tr(
                                "Scans CPU, RAM, disks, 9 cache folders, and identifies the 15 largest subfolders in %TEMP%.",
                                "Scanne l'état du processeur, de la mémoire RAM, des disques, des 9 dossiers de caches et identifie les 15 sous-dossiers les plus lourds dans %TEMP%.",
                            ),
                            l.tr("✔ 100% read-only: does not delete anything", "✔ 100 % en lecture seule : ne supprime rien"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_pc_diagnose();
                }
            });
        });
        ui.add_space(6.0);

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(
                    RichText::new(l.tr(
                        "• 1. Cleanup Options (hover [?] for details)",
                        "• 1. Options de Nettoyage (survolez [?] pour les détails)",
                    ))
                    .strong(),
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.clean_user_temp,
                    l.tr("Clean user %TEMP%", "Vider %TEMP% utilisateur"),
                    l.tr("User %TEMP% Folder Cleanup", "Nettoyage du dossier %TEMP% utilisateur"),
                    l.tr(
                        "▪ Default: Windows stores temporary caches and application installer files in %LOCALAPPDATA%\\Temp indefinitely.\n▪ If enabled: Deletes unneeded user temporary files older than 24h, freeing disk space safely.\n▪ If skipped: Keeps temporary files and installer remnants on your system drive.",
                        "▪ Par défaut : Windows conserve indéfiniment les caches et résidus d'installation dans %LOCALAPPDATA%\\Temp.\n▪ Si utilisé : Supprime les fichiers temporaires de session utilisateur de plus de 24h, libérant de l'espace disque.\n▪ Si non utilisé : Conserve l'intégralité des résidus et fichiers temporaires sur votre disque système.",
                    ),
                    l.tr("✔ 100% safe: files in use by active applications are automatically skipped", "✔ 100 % sans danger : les fichiers utilisés par vos applications ouvertes sont automatiquement ignorés"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.clean_sys_temp,
                    l.tr("Clean C:\\Windows\\Temp system", "Vider C:\\Windows\\Temp système"),
                    l.tr("System C:\\Windows\\Temp Folder Cleanup", "Nettoyage du dossier C:\\Windows\\Temp"),
                    l.tr(
                        "▪ Default: System services, drivers, and background installers leave temporary logs in C:\\Windows\\Temp.\n▪ If enabled: Cleans orphan system temporary files not locked by active Windows processes.\n▪ If skipped: Leaves system temporary files and driver logs accumulating in C:\\Windows\\Temp.",
                        "▪ Par défaut : Les services système, pilotes et installeurs stockent des résidus dans C:\\Windows\\Temp.\n▪ Si utilisé : Nettoie les fichiers temporaires orphelins non verrouillés par les processus Windows actifs.\n▪ Si non utilisé : Laisse les résidus système et journaux de pilotes s'accumuler dans C:\\Windows\\Temp.",
                    ),
                    l.tr("✔ 100% safe", "✔ 100 % sans danger"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.clean_wu_download,
                    l.tr("Clean Windows Update cache (SoftwareDistribution\\Download)", "Vider le cache Windows Update (SoftwareDistribution\\Download)"),
                    l.tr("Windows Update Download Cache Purge", "Purge du cache de téléchargement Windows Update"),
                    l.tr(
                        "▪ Default: Downloaded Windows Update installation packages remain in SoftwareDistribution\\Download.\n▪ If enabled: Stops the update service, deletes cached packages, and restarts Windows Update smoothly.\n▪ If skipped: Keeps downloaded update files, which may consume several gigabytes of disk space.",
                        "▪ Par défaut : Les paquets d'installation téléchargés par Windows Update restent stockés sur le disque.\n▪ Si utilisé : Interrompt le service, purge les paquets téléchargés obsolètes, puis redémarre Windows Update.\n▪ Si non utilisé : Conserve les paquets de mise à jour téléchargés, occupant souvent plusieurs gigaoctets.",
                    ),
                    l.tr("✔ 100% safe: often frees several GB and fixes stuck updates", "✔ 100 % sans danger : libère souvent plusieurs Go et répare les mises à jour bloquées"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.clean_wer_dumps,
                    l.tr("Delete error reports (WER) and CrashDumps", "Supprimer les rapports d'erreur (WER) et CrashDumps"),
                    l.tr("Crash Reports Cleanup (WER & CrashDumps)", "Suppression des rapports de plantage (WER & CrashDumps)"),
                    l.tr(
                        "▪ Default: Windows Error Reporting queues reports and crash dumps in memory logs indefinitely.\n▪ If enabled: Deletes crash memory dump files (.dmp) and error reporting archives to reclaim disk space.\n▪ If skipped: Retains past crash dump files and error queues on the system partition.",
                        "▪ Par défaut : Windows Error Reporting archive les journaux d'erreurs et fichiers de vidage mémoire.\n▪ Si utilisé : Supprime les fichiers de plantage (.dmp) et les archives de rapports d'erreurs accumulés.\n▪ Si non utilisé : Conserve l'ensemble des rapports d'erreurs et fichiers de crash sur la partition système.",
                    ),
                    l.tr("✔ 100% safe", "✔ 100 % sans danger"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.empty_recycle_bin,
                    l.tr("Empty Recycle Bin on all drives", "Vider la corbeille de tous les disques"),
                    l.tr("Windows Recycle Bin Emptying", "Vidage de la Corbeille Windows"),
                    l.tr(
                        "▪ Default: Deleted files remain stored in the Recycle Bin on all local and external drives.\n▪ If enabled: Empties the Recycle Bin permanently across all detected drives in one operation.\n▪ If skipped: Leaves existing files in the Recycle Bin until manually emptied.",
                        "▪ Par défaut : Les fichiers supprimés restent stockés dans la Corbeille de chaque lecteur.\n▪ Si utilisé : Purge définitivement le contenu de la Corbeille sur l'ensemble des disques connectés.\n▪ Si non utilisé : Conserve tous les éléments actuellement présents dans la Corbeille.",
                    ),
                    l.tr("⚠ Warning: uncheck this if you store files in your Recycle Bin!", "⚠ Attention : décochez cette case si vous conservez des fichiers dans votre Corbeille !"),
                    true,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.dism_component_cleanup,
                    l.tr("Deep WinSxS cleanup (DISM /StartComponentCleanup /ResetBase — slow)", "Nettoyer le WinSxS (DISM /StartComponentCleanup /ResetBase — lent)"),
                    l.tr("WinSxS Component Store Deep Cleanup", "Nettoyage approfondi du magasin de composants WinSxS"),
                    l.tr(
                        "▪ Default: WinSxS keeps historical backup copies of superseded system components after every update.\n▪ If enabled: Runs DISM /StartComponentCleanup /ResetBase to remove obsolete components (slow, frees 2-8 GB).\n▪ If skipped: Retains old component versions, allowing uninstallation of previous updates.",
                        "▪ Par défaut : WinSxS conserve les copies de sauvegarde de tous les composants remplacés par les mises à jour.\n▪ Si utilisé : Exécute DISM /StartComponentCleanup /ResetBase pour purger les composants obsolètes (lent, gain 2-8 Go).\n▪ Si non utilisé : Conserve les versions antérieures de composants, permettant de désinstaller d'anciennes mises à jour.",
                    ),
                    l.tr("ℹ Unchecked by default because it takes 5 to 15 minutes (can free 2 to 8 GB)", "ℹ Décoché par défaut car l'opération dure 5 à 15 minutes (permet de gagner 2 à 8 Go)"),
                    true,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.run_cleanmgr,
                    l.tr("Run Windows Disk Cleanup (silent cleanmgr)", "Lancer le nettoyage de disque Windows (cleanmgr silencieux)"),
                    l.tr("Official Microsoft Disk Cleanup (cleanmgr.exe)", "Nettoyage de disque officiel Microsoft (cleanmgr.exe)"),
                    l.tr(
                        "▪ Default: Standard Windows Disk Cleanup utility requires manual wizard selection and interaction.\n▪ If enabled: Automatically activates 20 Microsoft safe cleanup flags and executes cleanmgr silently.\n▪ If skipped: Does not invoke Windows Disk Cleanup utility.",
                        "▪ Par défaut : L'utilitaire de nettoyage de disque Windows nécessite une sélection manuelle par l'utilisateur.\n▪ Si utilisé : Configure automatiquement 20 règles de nettoyage sécurisées et exécute cleanmgr en arrière-plan.\n▪ Si non utilisé : N'exécute pas l'outil de nettoyage de disque Windows officiel.",
                    ),
                    l.tr("✔ 100% safe", "✔ 100 % sans danger"),
                    false,
                );
            });

            cols[1].group(|ui| {
                ui.label(
                    RichText::new(l.tr(
                        "⚡ 2. System Optimizations (hover [?] for details)",
                        "⚡ 2. Optimisations Système (survolez [?] pour les détails)",
                    ))
                    .strong(),
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.high_perf_power,
                    l.tr("Power Plan: High Performance (SCHEME_MIN)", "Plan d'alimentation : Performances élevées (SCHEME_MIN)"),
                    l.tr("High Performance Power Profile", "Profil d'alimentation « Performances élevées »"),
                    l.tr(
                        "▪ Default: Windows uses the Balanced power plan, dynamically throttling CPU clocks to save energy.\n▪ If enabled: Activates High Performance plan (SCHEME_MIN), keeping CPU ready with zero downclock lag.\n▪ If skipped: Retains current power plan without modifying CPU frequency management.",
                        "▪ Par défaut : Windows utilise le mode Équilibré, réduisant dynamiquement la fréquence CPU pour économiser l'énergie.\n▪ Si utilisé : Active le plan Performances élevées (SCHEME_MIN) pour éliminer les micro-latences de réveil processeur.\n▪ Si non utilisé : Conserve votre mode d'alimentation actuel sans modifier la gestion du processeur.",
                    ),
                    l.tr("✔ Recommended on desktop or plugged-in laptop (reversible in Restore tab)", "✔ Recommandé sur PC fixe ou PC portable branché (annulable dans Restauration)"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.ultimate_perf_power,
                    l.tr("Enable 'Ultimate Performance' plan (created if missing)", "Activer le plan 'Ultimate Performance' (créé si absent)"),
                    l.tr("Hidden 'Ultimate Performance' Profile", "Profil caché « Performances optimales » (Ultimate Performance)"),
                    l.tr(
                        "▪ Default: Ultimate Performance mode is hidden on consumer editions of Windows 10 and 11.\n▪ If enabled: Unlocks and activates the Workstation Ultimate Performance profile with zero core micro-parking.\n▪ If skipped: Leaves current power plan active without generating the Ultimate Performance scheme.",
                        "▪ Par défaut : Le plan Performances optimales est masqué sur les éditions grand public de Windows.\n▪ Si utilisé : Déverrouille et applique le plan station de travail avec suppression totale du micro-parking des cœurs.\n▪ Si non utilisé : Ne crée ni n'active ce plan d'alimentation spécifique.",
                    ),
                    l.tr("ℹ Useful for gaming and audio workstations (slightly higher idle power)", "ℹ Utile sur tour gaming/MAO (consomme un peu plus d'énergie au repos)"),
                    true,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.reduce_visual_fx,
                    l.tr("Reduce visual effects (VisualFXSetting = 2 'Best Performance')", "Réduire les effets visuels (VisualFXSetting = 2 'Meilleures performances')"),
                    l.tr("Visual Effects in 'Best Performance' Mode", "Effets visuels en mode « Meilleures performances »"),
                    l.tr(
                        "▪ Default: Windows renders all UI animations, sliding menus, window transitions, and drop shadows.\n▪ If enabled: Sets VisualFXSetting to Best Performance (2), disabling heavy animations for immediate UI response.\n▪ If skipped: Keeps current Windows visual effects and animations enabled.",
                        "▪ Par défaut : Windows active toutes les animations, transitions de fenêtres et ombres portées du bureau.\n▪ Si utilisé : Règle VisualFXSetting sur Meilleures performances (2), désactivant les effets lourds pour une réactivité maximale.\n▪ Si non utilisé : Conserve les animations graphiques et effets visuels actuels de Windows.",
                    ),
                    l.tr("✔ Makes UI snappier (reversible in 1 click in Restore tab)", "✔ Rend l'interface plus réactive (annulable en 1 clic dans Restauration)"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.enable_storage_sense,
                    l.tr("Enable Storage Sense (auto cleanup 30d)", "Activer Storage Sense (nettoyage automatique périodique 30j)"),
                    l.tr("Windows Storage Sense Auto Cleanup", "Assistant Stockage automatique de Windows (Storage Sense)"),
                    l.tr(
                        "▪ Default: Storage Sense auto-cleanup is often disabled or set to run only during critically low disk space.\n▪ If enabled: Configures Storage Sense to automatically purge old temporary files and Recycle Bin items every 30 days.\n▪ If skipped: Leaves Storage Sense in its current configuration without scheduling automated cleanup.",
                        "▪ Par défaut : L'Assistant Stockage est souvent désactivé ou ne se déclenche qu'en cas de saturation critique du disque.\n▪ Si utilisé : Active et configure Storage Sense pour purger automatiquement les fichiers temporaires tous les 30 jours.\n▪ Si non utilisé : Ne modifie pas la configuration actuelle de l'Assistant Stockage Windows.",
                    ),
                    l.tr("✔ 100% safe and keeps your PC clean over time", "✔ 100 % sans danger et maintient le PC propre dans la durée"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.disable_game_bar,
                    l.tr("Disable Xbox Game Bar overlay & GameDVR", "Désactiver l'overlay Xbox Game Bar & GameDVR"),
                    l.tr("Disable Xbox Game Bar & Background GameDVR Capture", "Désactivation de Xbox Game Bar & Capture GameDVR en arrière-plan"),
                    l.tr(
                        "▪ Default: Windows enables background GameDVR capture and the Xbox Game Bar overlay during games.\n▪ If enabled: Disables GameDVR recording and Game Bar popups, preventing micro-stutters and input lag in games.\n▪ If skipped: Xbox Game Bar overlay and continuous background capture stay active.",
                        "▪ Par défaut : Windows active la capture GameDVR continue en arrière-plan et l'overlay Xbox Game Bar en jeu.\n▪ Si utilisé : Désactive l'enregistrement GameDVR et l'overlay Game Bar pour éliminer les micro-saccades en jeu.\n▪ Si non utilisé : Conserve l'overlay Xbox Game Bar et la capture automatique en arrière-plan.",
                    ),
                    l.tr("✔ Prevents FPS drops and micro-stutters during gaming", "✔ Évite les pertes de FPS et micro-saccades en jeu"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.disable_transparency,
                    l.tr("Disable Windows transparency effects", "Désactiver les effets de transparence Windows"),
                    l.tr("Disable Acrylic / Mica Transparency Blur", "Désactivation du flou de transparence (Acrylique / Mica)"),
                    l.tr(
                        "▪ Default: Windows applies Mica and Acrylic transparency blur effects to the taskbar, Start Menu, and windows.\n▪ If enabled: Disables transparency effects (EnableTransparency = 0), reducing DWM GPU rendering overhead.\n▪ If skipped: Maintains transparency and acrylic blur effects on taskbar and applications.",
                        "▪ Par défaut : Windows applique des effets de transparence et de flou (Mica / Acrylique) sur l'interface.\n▪ Si utilisé : Désactive la transparence (EnableTransparency = 0) pour alléger le travail de rendu GPU du bureau.\n▪ Si non utilisé : Conserve les effets de transparence et de flou sur les fenêtres et la barre des tâches.",
                    ),
                    l.tr("✔ Reversible anytime in Restore tab", "✔ Réversible à tout moment dans Restauration"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.disable_sysmain,
                    l.tr("Disable SysMain / Superfetch (only if SSD + RAM >= 8 GB)", "Désactiver SysMain / Superfetch (uniquement si SSD + RAM >= 8 Go)"),
                    l.tr("Smart SysMain (Superfetch) Service Disabling", "Désactivation intelligente du service SysMain (Superfetch)"),
                    l.tr(
                        "▪ Default: SysMain (Superfetch) preloads application cache into RAM and performs periodic disk background indexing.\n▪ If enabled: Disables SysMain service safely (only if SSD + >= 8 GB RAM detected), reducing background disk writes.\n▪ If skipped: SysMain service continues running and prefetching applications.",
                        "▪ Par défaut : SysMain (Superfetch) précharge des applications en RAM et génère des accès disque en arrière-plan.\n▪ Si utilisé : Désactive SysMain en toute sécurité (uniquement si SSD + >= 8 Go RAM), réduisant l'usure disque.\n▪ Si non utilisé : Laisse le service SysMain actif pour le préchargement d'applications en mémoire.",
                    ),
                    l.tr("✔ Safe: WinCare checks for SSD and >= 8 GB RAM before disabling", "✔ Sécurisé : WinCare vérifie que vous avez un SSD et >= 8 Go de RAM avant de le désactiver"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.optimize_volume_trim,
                    l.tr("Run Optimize-Volume (TRIM on SSD / Defrag on HDD)", "Exécuter Optimize-Volume (TRIM sur SSD / Défragmentation sur HDD)"),
                    l.tr("System Drive Optimization (TRIM SSD / Defrag HDD)", "Optimisation du disque système (TRIM SSD / Defrag HDD)"),
                    l.tr(
                        "▪ Default: Windows schedules drive maintenance automatically, but TRIM frequency may lag behind heavy writes.\n▪ If enabled: Immediately triggers Optimize-Volume: executes TRIM on SSDs to maintain write speed or defrags HDDs.\n▪ If skipped: Relies on Windows background maintenance schedule to eventually optimize the system drive.",
                        "▪ Par défaut : Windows planifie la maintenance des disques mais l'intervalle peut retarder l'envoi des commandes TRIM.\n▪ Si utilisé : Déclenche immédiatement Optimize-Volume : exécute un TRIM sur SSD (performances d'écriture) ou défragmente un HDD.\n▪ Si non utilisé : Attend le passage différé de la tâche planifiée de maintenance automatique de Windows.",
                    ),
                    l.tr("✔ 100% safe and recommended by Microsoft", "✔ 100 % sans danger et recommandé par Microsoft"),
                    false,
                );
            });
        });

        ui.add_space(8.0);

        // Analyseur des gros dossiers %TEMP%
        ui.group(|ui| {
            let total_top_mb: f64 = self.temp_subfolders.iter().map(|f| f.size_mb).sum();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "• 3. {} ({:.1} {} / {:.2} {} {})",
                        l.tr("Largest folders in %TEMP%", "Analyseur des plus gros dossiers dans %TEMP%"),
                        total_top_mb,
                        l.tr("MB", "Mo"),
                        total_top_mb / 1024.0,
                        l.tr("GB", "Go"),
                        l.tr("listed", "listés"),
                    ))
                    .strong()
                    .color(Color32::from_rgb(255, 200, 85)),
                );
                if ui
                    .add_enabled(!self.is_busy, egui::Button::new(l.tr("▶ Scan %TEMP%", "▶ Scanner %TEMP%")))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Scan Top 15 folders in %TEMP%", "Scanner le Top 15 des dossiers dans %TEMP%"),
                            l.tr(
                                "Analyzes all subfolders in %TEMP% and C:\\Windows\\Temp, sorted by size descending to see what consumes disk space.",
                                "Analyse tous les sous-dossiers de %TEMP% et C:\\Windows\\Temp et les trie du plus lourd au plus léger pour voir exactement ce qui prend de la place.",
                            ),
                            l.tr("✔ 100% read-only", "✔ 100 % en lecture seule"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_temp_scan();
                }
                if !self.temp_subfolders.is_empty()
                    && ui
                        .add_enabled(
                            !self.is_busy,
                            egui::Button::new(
                                RichText::new(l.tr("✕ Purge all these large items", "✕ Purger tous ces gros éléments"))
                                    .color(Color32::from_rgb(255, 110, 110)),
                            ),
                        )
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Purge all listed large temporary items", "Purger tous les gros éléments temporaires listés"),
                                l.tr(
                                    "Deletes all temporary subfolders listed in this table at once (safely restricted to %TEMP%).",
                                    "Supprime d'un seul coup les sous-dossiers temporaires listés dans ce tableau (protégé pour n'agir qu'à l'intérieur de %TEMP%).",
                                ),
                                l.tr("✔ Immediately frees the corresponding gigabytes", "✔ Libère immédiatement les gigaoctets correspondants"),
                                false,
                            );
                        })
                        .clicked()
                {
                    let all_paths: Vec<String> =
                        self.temp_subfolders.iter().map(|f| f.path.clone()).collect();
                    self.spawn_delete_temp_paths(all_paths);
                }
            });

            if self.temp_subfolders.is_empty() {
                ui.label(
                    RichText::new(l.tr(
                        "Click 'Scan %TEMP%' to inspect which subfolders consume your temporary space.",
                        "Cliquez sur « Scanner %TEMP% » pour voir quels sous-dossiers occupent vos gigaoctets temporaires.",
                    ))
                    .weak(),
                );
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("temp_subfolders_scroll")
                    .max_height(155.0)
                    .show(ui, |ui| {
                        egui::Grid::new("temp_subfolders_grid")
                            .striped(true)
                            .num_columns(5)
                            .show(ui, |ui| {
                                ui.strong(l.tr("Action", "Action"));
                                ui.strong(l.tr("Name", "Nom"));
                                ui.strong(l.tr("Size", "Taille"));
                                ui.strong(l.tr("Files", "Fichiers"));
                                ui.strong(l.tr("Path", "Chemin"));
                                ui.end_row();

                                let items_snapshot = self.temp_subfolders.clone();
                                for item in items_snapshot {
                                    ui.push_id(&item.path, |ui| {
                                        if ui
                                            .add_enabled(
                                                !self.is_busy,
                                                egui::Button::new(l.tr("✕ Delete", "✕ Supprimer")),
                                            )
                                            .on_hover_text(format!(
                                                "{}:\n{}",
                                                l.tr("Delete only this temporary folder/file", "Supprimer uniquement le dossier/fichier temporaire"),
                                                item.path
                                            ))
                                            .clicked()
                                        {
                                            self.spawn_delete_temp_paths(vec![item.path.clone()]);
                                        }
                                    });
                                    ui.label(&item.name).on_hover_text(&item.path);
                                    let size_txt = if item.size_mb >= 1024.0 {
                                        format!("{:.2} {}", item.size_mb / 1024.0, l.tr("GB", "Go"))
                                    } else {
                                        format!("{:.1} {}", item.size_mb, l.tr("MB", "Mo"))
                                    };
                                    ui.label(
                                        RichText::new(size_txt)
                                            .strong()
                                            .color(Color32::from_rgb(255, 195, 80)),
                                    );
                                    ui.label(item.file_count.to_string());
                                    ui.label(RichText::new(&item.path).weak());
                                    ui.end_row();
                                }
                            });
                    });
            }
        });

        ui.add_space(8.0);

        // Programmes au démarrage (StartupApproved)
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "• 4. {} ({} {} — {})",
                        l.tr("Startup Programs", "Programmes au démarrage"),
                        self.startup_items.len(),
                        l.tr("detected", "détectés"),
                        l.tr("Task Manager compatible", "compatible Gestionnaire des tâches")
                    ))
                    .strong(),
                );
                if ui
                    .small_button(l.tr("↻ Refresh list", "↻ Actualiser la liste"))
                    .on_hover_text(l.tr(
                        "Rereads current startup applications state from Windows Registry.",
                        "Relit l'état actuel des applications au démarrage dans le Registre Windows.",
                    ))
                    .clicked()
                {
                    self.startup_items = crate::boost_pc::list_startup_items();
                }
            });

            egui::ScrollArea::vertical()
                .id_salt("startup_scroll")
                .max_height(145.0)
                .show(ui, |ui| {
                    egui::Grid::new("startup_grid")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            ui.strong(l.tr("State", "État"));
                            ui.strong(l.tr("Name", "Nom"));
                            ui.strong(l.tr("Location", "Emplacement"));
                            ui.strong(l.tr("Command", "Commande"));
                            ui.end_row();

                            let items_snapshot = self.startup_items.clone();
                            for item in items_snapshot {
                                let btn_text = if item.enabled {
                                    RichText::new(" [ON] ")
                                        .color(Color32::from_rgb(80, 220, 120))
                                        .strong()
                                } else {
                                    RichText::new("[OFF]").color(Color32::from_rgb(160, 160, 160))
                                };
                                ui.push_id((&item.source, &item.name), |ui| {
                                    if ui
                                        .button(btn_text)
                                        .on_hover_ui(|ui| {
                                            show_rich_tooltip(
                                                ui,
                                                &format!("{}: {}", l.tr("Startup Program", "Programme au démarrage"), item.name),
                                                &match l {
                                                    Lang::En => format!(
                                                        "Command: {}\nSource: {}\n\n▪ Default: Application is registered to run automatically on Windows startup.\n▪ If enabled: Program launches automatically with Windows (ON).\n▪ If skipped: Program will not launch at boot (OFF), saving RAM and boot time.",
                                                        item.command, item.source
                                                    ),
                                                    Lang::Fr => format!(
                                                        "Commande : {}\nSource : {}\n\n▪ Par défaut : L'application est configurée pour démarrer automatiquement avec Windows.\n▪ Si utilisé : Le programme se lance au démarrage de votre session (ON).\n▪ Si non utilisé : Le programme est désactivé au démarrage (OFF), accélérant le boot et économisant la RAM.",
                                                        item.command, item.source
                                                    ),
                                                },
                                                l.tr(
                                                    "✔ Uses official Task Manager StartupApproved mechanism (no entry is deleted)",
                                                    "✔ Utilise le mécanisme officiel StartupApproved du Gestionnaire des tâches (aucune entrée n'est supprimée)"
                                                ),
                                                false,
                                            );
                                        })
                                        .clicked()
                                    {
                                        let runner = self.make_runner();
                                        if crate::boost_pc::toggle_startup_item(
                                            &runner,
                                            &item,
                                            !item.enabled,
                                        ) {
                                            if !self.dry_run {
                                                self.startup_items =
                                                    crate::boost_pc::list_startup_items();
                                            } else if let Some(existing) = self
                                                .startup_items
                                                .iter_mut()
                                                .find(|i| {
                                                    i.name == item.name && i.source == item.source
                                                })
                                            {
                                                existing.enabled = !existing.enabled;
                                            }
                                        }
                                    }
                                });
                                ui.label(&item.name).on_hover_text(&item.command);
                                ui.label(RichText::new(&item.source).weak());
                                ui.label(RichText::new(&item.command).weak());
                                ui.end_row();
                            }
                        });
                });
        });

        // Résultats du diagnostic & comparatif Avant/Après
        if !self.pc_diffs.is_empty() {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.label(RichText::new(l.tr("• Before / After Comparison", "• Comparaison Avant / Après")).strong());
                render_diff_table(ui, "pc_diff_grid", &self.pc_diffs, l);
            });
        }

        if let Some(diag) = &self.pc_diag {
            ui.add_space(6.0);
            ui.columns(2, |cols| {
                cols[0].group(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "• {} ({}: {} {})",
                            l.tr("Detected Caches & Residues", "Caches & Débris détectés"),
                            l.tr("Total", "Total"),
                            diag.cache_total_mb,
                            l.tr("MB", "Mo")
                        ))
                        .strong(),
                    );
                    egui::Grid::new("caches_grid").striped(true).show(ui, |ui| {
                        ui.strong(l.tr("Cache", "Cache"));
                        ui.strong(l.tr("Size", "Taille"));
                        ui.end_row();
                        for c in &diag.caches {
                            ui.label(&c.label).on_hover_text(&c.path);
                            ui.label(format!("{} {}", c.size_mb, l.tr("MB", "Mo")));
                            ui.end_row();
                        }
                    });
                });

                cols[1].group(|ui| {
                    ui.label(RichText::new(l.tr("• Top Processes by RAM", "• Top Processus par RAM")).strong());
                    egui::Grid::new("top_proc_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("PID");
                            ui.strong(l.tr("Process", "Processus"));
                            ui.strong("RAM");
                            ui.end_row();
                            for p in &diag.top_processes {
                                ui.label(p.pid.to_string());
                                ui.label(&p.name);
                                ui.label(format!("{} {}", p.ram_mb, l.tr("MB", "Mo")));
                                ui.end_row();
                            }
                        });
                });
            });
        }
    }

    fn render_boost_net_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.horizontal(|ui| {
            ui.heading(l.tr(
                "Boost Internet — Network Diagnosis, DNS Benchmark & MTU",
                "Boost Internet — Diagnostic Réseau, Benchmark DNS & MTU",
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(
                            RichText::new(l.tr(
                                "▶ Apply Network Optimizations",
                                "▶ Appliquer les Optimisations Réseau",
                            ))
                            .strong(),
                        ),
                    )
                    .on_hover_text(l.tr(
                        "Applies checked network optimizations below and displays Before / After ping comparison.",
                        "Applique les optimisations réseau cochées ci-dessous et affiche le comparatif de ping Avant / Après.",
                    ))
                    .clicked()
                {
                    self.spawn_net_boost();
                }
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("▶ Diagnose Connection", "▶ Diagnostiquer la connexion")),
                    )
                    .on_hover_text(l.tr(
                        "Read-only test of network route, ping, fastest DNS servers, and optimal MTU without fragmentation.",
                        "Teste en lecture seule votre route réseau, le ping, les serveurs DNS les plus rapides et le MTU optimal sans fragmentation.",
                    ))
                    .clicked()
                {
                    self.spawn_net_diagnose();
                }
                ui.checkbox(&mut self.include_speedtest, l.tr("Ookla Speedtest", "Speedtest Ookla"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Ookla Speedtest CLI", "Speedtest Ookla CLI"),
                            l.tr(
                                "▪ Default: Bandwidth speed testing is skipped during network diagnosis to save time.\n▪ If enabled: Measures real download/upload speed and latency before and after optimization via Ookla Speedtest.\n▪ If skipped: Only ping, routing, DNS benchmark, and MTU will be evaluated.",
                                "▪ Par défaut : Le test de bande passante est ignoré lors du diagnostic réseau pour un résultat instantané.\n▪ Si utilisé : Mesure le débit descendant/montant réel et la latence avant et après optimisation via Ookla Speedtest.\n▪ Si non utilisé : Seuls le ping, la route, le benchmark DNS et le MTU sont évalués.",
                            ),
                            l.tr("ℹ Requires official speedtest.exe utility", "ℹ Nécessite l'exécutable officiel speedtest.exe"),
                            false,
                        );
                    });
            });
        });
        ui.add_space(6.0);

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(
                    RichText::new(l.tr(
                        "⚙ Available Network Optimizations (hover [?] for details)",
                        "⚙ Optimisations Réseau disponibles (survolez [?] pour les détails)",
                    ))
                    .strong(),
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.apply_best_dns,
                    l.tr(
                        "Apply fastest DNS resolver (preserves NextDNS if active)",
                        "Appliquer le résolveur DNS le plus rapide (préserve NextDNS si actif)",
                    ),
                    l.tr("Automatic Best DNS Server Configuration", "Configuration automatique du meilleur serveur DNS"),
                    l.tr(
                        "▪ Default: Windows uses your router or ISP default DNS, which can have slow lookup latency or content filtering.\n▪ If enabled: Tests Cloudflare, Google, Quad9, and OpenDNS in real-time, applying the lowest latency resolver.\n▪ If skipped: Retains your current DNS configuration (NextDNS is always preserved automatically).",
                        "▪ Par défaut : Windows utilise les DNS fournis par votre box/FAI, souvent plus lents et filtrés.\n▪ Si utilisé : Teste Cloudflare, Google, Quad9 et OpenDNS en direct et configure le résolveur le plus rapide.\n▪ Si non utilisé : Conserve vos serveurs DNS actuels (NextDNS est toujours détecté et préservé).",
                    ),
                    l.tr(
                        "✔ If NextDNS is detected, your DNS is NEVER modified to preserve ad-filtering",
                        "✔ Si NextDNS est détecté sur votre PC, vos DNS ne sont JAMAIS modifiés pour préserver votre filtrage anti-pub",
                    ),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.apply_optimal_mtu,
                    l.tr("Set optimal unfragmented MTU detected", "Régler le MTU optimal détecté sans fragmentation"),
                    l.tr("Optimal MTU (Maximum Transmission Unit) Tuning", "Calibrage du MTU (Maximum Transmission Unit)"),
                    l.tr(
                        "▪ Default: Windows defaults to MTU 1500, which causes packet fragmentation if your ISP uses PPPoE or encapsulation.\n▪ If enabled: Discovers the maximum non-fragmented packet size via ICMP probes and configures your active adapter.\n▪ If skipped: Leaves MTU at its current system value, risking fragmented packets on certain network types.",
                        "▪ Par défaut : Windows applique un MTU standard de 1500, causant des fragmentations si votre FAI utilise le PPPoE.\n▪ Si utilisé : Détecte par sondes ICMP la taille maximale de paquet sans fragmentation et l'applique à votre carte.\n▪ Si non utilisé : Conserve la valeur actuelle du MTU, avec un risque de découpage des paquets en jeu ou téléchargement.",
                    ),
                    l.tr(
                        "✔ Prevents packet loss and gaming latency (reversible in Restore tab)",
                        "✔ Évite les pertes de paquets et la latence en jeu (annulable dans Restauration)",
                    ),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.flush_dns,
                    l.tr("Flush DNS cache (ipconfig /flushdns)", "Vider le cache DNS (ipconfig /flushdns)"),
                    l.tr("DNS Resolver Cache Purge", "Purge du cache de résolution DNS"),
                    l.tr(
                        "▪ Default: Windows caches DNS host-to-IP mappings locally, which can become outdated or poisoned.\n▪ If enabled: Purges the local DNS resolver cache (ipconfig /flushdns) to resolve hostnames fresh.\n▪ If skipped: Keeps current cached DNS resolutions until their natural TTL expires.",
                        "▪ Par défaut : Windows stocke en mémoire les résolutions de domaines, pouvant garder des adresses obsolètes.\n▪ Si utilisé : Vide intégralement le cache du résolveur DNS local (ipconfig /flushdns) pour réinterroger les serveurs.\n▪ Si non utilisé : Conserve les enregistrements DNS en mémoire jusqu'à expiration de leur délai de validité (TTL).",
                    ),
                    l.tr("✔ 100% safe", "✔ 100 % sans danger"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.disable_wifi_power_saving,
                    l.tr("Wi-Fi: Disable wireless adapter power saving", "Wi-Fi : Désactiver l'économie d'énergie de la carte sans-fil"),
                    l.tr("Maximum Performance Mode for Wi-Fi Adapter", "Mode Performances Maximales pour la carte Wi-Fi"),
                    l.tr(
                        "▪ Default: Windows allows Wi-Fi network cards to enter power-saving micro-sleep modes when idle.\n▪ If enabled: Sets Wi-Fi adapter power management to Maximum Performance (0), stopping jitter and ping spikes.\n▪ If skipped: Preserves default Wi-Fi power saving mode, which may save slight battery on laptops.",
                        "▪ Par défaut : Windows autorise la carte Wi-Fi à passer en micro-veille énergétique lors des temps d'inactivité.\n▪ Si utilisé : Règle l'alimentation de la carte Wi-Fi sur Performances maximales (0), éliminant le jitter et les pics de ping.\n▪ Si non utilisé : Laisse le mode d'économie d'énergie Wi-Fi actif, ce qui économise une infime quantité de batterie.",
                    ),
                    l.tr("✔ Reduces ping spikes (jitter) on Wi-Fi", "✔ Réduit les pics de ping (jitter) en Wi-Fi"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.disable_wu_p2p,
                    l.tr("Limit Windows Update P2P sharing (DODownloadMode = 0)", "Limiter le partage P2P de Windows Update (DODownloadMode = 0)"),
                    l.tr("Disable Windows Update Delivery Optimization P2P", "Désactivation du partage P2P Windows Update (Delivery Optimization)"),
                    l.tr(
                        "▪ Default: Windows Delivery Optimization uploads update files to other PCs on local network and the Internet.\n▪ If enabled: Disables P2P sharing (DODownloadMode = 0), preventing Windows from saturating your upload bandwidth.\n▪ If skipped: Windows continues sharing update files in the background with external machines.",
                        "▪ Par défaut : Windows Delivery Optimization partage vos mises à jour avec d'autres PC sur Internet en P2P.\n▪ Si utilisé : Désactive le partage P2P (DODownloadMode = 0), protégeant votre débit montant (Upload) et votre ping.\n▪ Si non utilisé : Windows continue d'utiliser votre bande passante montante pour distribuer les mises à jour.",
                    ),
                    l.tr("✔ Frees upload bandwidth and stabilizes ping", "✔ Libère la bande passante Upload et stabilise le ping"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.disable_smb1,
                    l.tr("Disable legacy SMB1 protocol (security + performance)", "Désactiver le protocole vétuste SMB1 (sécurité + performance)"),
                    l.tr("SMBv1 Protocol Disabling", "Désactivation du protocole SMBv1"),
                    l.tr(
                        "▪ Default: Deprecated and insecure SMBv1 network sharing protocol may remain active on older Windows installations.\n▪ If enabled: Disables the SMB1Protocol optional feature, eliminating critical security vulnerabilities (e.g. WannaCry).\n▪ If skipped: Leaves SMBv1 active if already enabled, maintaining legacy compatibility with obsolete NAS devices.",
                        "▪ Par défaut : Le protocole SMBv1 obsolète et vulnérable peut rester actif sur certaines installations Windows.\n▪ Si utilisé : Désactive la fonctionnalité SMB1Protocol, bloquant des failles réseau majeures (comme WannaCry).\n▪ Si non utilisé : Laisse le protocole SMB1 actif s'il est présent, maintenant la compatibilité avec de très vieux NAS.",
                    ),
                    l.tr("✔ Recommended by Microsoft (network security)", "✔ Recommandé par Microsoft (sécurité réseau)"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.reset_winsock_tcp,
                    l.tr("Complete Winsock + TCP/IP stack reset (requires reboot)", "Reset complet des piles Winsock + TCP/IP (nécessite un redémarrage)"),
                    l.tr("Winsock & TCP/IP Stack Reset", "Réinitialisation Winsock & Pile TCP/IP"),
                    l.tr(
                        "▪ Default: Network socket catalogs and TCP/IP routing tables can accumulate corrupted winsock LSPs.\n▪ If enabled: Executes netsh winsock reset and netsh int ip reset to rebuild fresh networking stacks.\n▪ If skipped: Retains existing winsock LSP providers and TCP/IP stack configuration without requiring a reboot.",
                        "▪ Par défaut : La pile TCP/IP et le catalogue Winsock peuvent accumuler des corruptions ou règles orphelines.\n▪ Si utilisé : Exécute netsh winsock reset et netsh int ip reset pour réinitialiser les composants réseau à neuf.\n▪ Si non utilisé : Conserve la pile réseau actuelle sans nécessiter de redémarrage de la machine.",
                    ),
                    l.tr(
                        "⚠ Unchecked by default: requires a PC reboot after execution",
                        "⚠ Décoché par défaut : nécessite un redémarrage du PC après application",
                    ),
                    true,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.run_speedtest_after,
                    l.tr("Rerun Speedtest after optimization to compare bandwidth", "Relancer Speedtest après optimisation pour comparer le débit"),
                    l.tr("Post-Optimization Bandwidth Benchmark", "Second test de débit après optimisation"),
                    l.tr(
                        "▪ Default: Only initial connection metrics are displayed without measuring post-optimization throughput.\n▪ If enabled: Runs a secondary Ookla Speedtest after applying tweaks to display before/after bandwidth improvements.\n▪ If skipped: Finishes network optimization immediately without waiting for a second bandwidth test.",
                        "▪ Par défaut : Seules les métriques réseau de départ sont mesurées sans second test de débit.\n▪ Si utilisé : Relance un test Ookla Speedtest après application pour quantifier précisément le gain de débit.\n▪ Si non utilisé : Termine les optimisations immédiatement sans ajouter les ~25 secondes du second test.",
                    ),
                    l.tr("ℹ Adds ~25 seconds at the end of optimization", "ℹ Ajoute ~25 secondes à la fin de l'optimisation"),
                    false,
                );
            });

            cols[1].group(|ui| {
                ui.label(RichText::new(l.tr("• Interface State & Latency", "• État de l'interface & Latence")).strong());
                if let Some(d) = &self.net_diag {
                    ui.label(format!("{}: {} ({})", l.tr("Adapter", "Adaptateur"), d.adapter_name, d.adapter_desc));
                    ui.label(format!("{}: {} | {}: {}", l.tr("Type", "Type"), d.media_type, l.tr("Speed", "Vitesse"), d.link_speed));
                    ui.label(format!(
                        "{}: {} (Ping: {:?} ms)",
                        l.tr("Gateway", "Passerelle"), d.gateway, d.ping_gw_ms
                    ));
                    ui.label(format!(
                        "Ping 1.1.1.1 : {:?} ms | {}: {}",
                        d.ping_net_ms, l.tr("Losses", "Pertes"), d.ping_loss_count
                    ));
                    ui.label(format!(
                        "{}: {} | {}: {:?}",
                        l.tr("Current MTU", "MTU actuel"), d.current_mtu,
                        l.tr("Optimal MTU without fragmentation", "MTU optimal sans fragmentation"), d.optimal_mtu
                    ));
                    ui.label(format!(
                        "{}: ↓ {:.2} Mbps | ↑ {:.2} Mbps",
                        l.tr("Instant throughput", "Débit instantané"), d.rx_mbps, d.tx_mbps
                    ));
                    if let (Some(dl), Some(ul)) = (d.speedtest_dl_mbps, d.speedtest_ul_mbps) {
                        ui.label(
                            RichText::new(format!(
                                "Speedtest : ↓ {:.1} Mbps | ↑ {:.1} Mbps | Ping {:?} ms",
                                dl, ul, d.speedtest_ping_ms
                            ))
                            .color(Color32::from_rgb(80, 220, 120)),
                        );
                    }
                    if d.uses_nextdns {
                        ui.label(
                            RichText::new(format!(
                                "✔ NextDNS {} ({}) — {}",
                                l.tr("detected", "détecté"), d.nextdns_mode, l.tr("filtering preserved", "Filtrage préservé")
                            ))
                            .color(Color32::from_rgb(220, 130, 255))
                            .strong(),
                        );
                    }
                } else {
                    ui.label(
                        RichText::new(l.tr(
                            "Click 'Diagnose Connection' to analyze your network.",
                            "Cliquez sur « Diagnostiquer la connexion » pour analyser votre réseau.",
                        ))
                        .weak(),
                    );
                }
            });
        });

        if !self.net_diffs.is_empty() {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.label(RichText::new(l.tr("• Network Before / After Comparison", "• Comparaison Réseau Avant / Après")).strong());
                render_diff_table(ui, "net_diff_grid", &self.net_diffs, l);
            });
        }

        if let Some(d) = &self.net_diag {
            ui.add_space(6.0);
            ui.columns(2, |cols| {
                cols[0].group(|ui| {
                    ui.label(RichText::new(l.tr("⚡ DNS Resolver Benchmark", "⚡ Benchmark des résolveurs DNS")).strong());
                    egui::Grid::new("dns_bench_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong(l.tr("Resolver", "Résolveur"));
                            ui.strong("IP");
                            ui.strong(l.tr("Latency", "Latence"));
                            ui.strong(l.tr("Status", "Statut"));
                            ui.end_row();
                            for r in &d.dns_results {
                                ui.label(&r.resolver);
                                ui.label(&r.ip);
                                ui.label(
                                    r.latency_ms
                                        .map(|ms| format!("{ms} ms"))
                                        .unwrap_or_else(|| "-".to_string()),
                                );
                                if r.ok {
                                    ui.label(
                                        RichText::new(l.tr("OK", "OK")).color(Color32::from_rgb(80, 220, 120)),
                                    );
                                } else {
                                    ui.label(
                                        RichText::new(l.tr("Failed", "Échec"))
                                            .color(Color32::from_rgb(240, 90, 90)),
                                    );
                                }
                                ui.end_row();
                            }
                        });
                });

                cols[1].group(|ui| {
                    ui.label(RichText::new(l.tr("• Processes with Active TCP Connections", "• Processus avec connexions TCP actives")).strong());
                    egui::Grid::new("net_proc_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("PID");
                            ui.strong(l.tr("Process", "Processus"));
                            ui.strong(l.tr("Connections", "Connexions"));
                            ui.strong("RAM");
                            ui.end_row();
                            for p in &d.top_net_processes {
                                ui.label(p.pid.to_string());
                                ui.label(&p.name);
                                ui.label(p.connections.to_string());
                                ui.label(format!("{} {}", p.ram_mb, l.tr("MB", "Mo")));
                                ui.end_row();
                            }
                        });
                });
            });
        }
    }

    fn render_debloat_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.horizontal(|ui| {
            ui.heading(l.tr(
                "Windows 11 Debloat & Tweaks (Based on Win11Debloat)",
                "Debloat & Tweaks Windows 11 (Basé sur Win11Debloat)",
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(l.tr("↻ Refresh state", "↻ Actualiser l'état"))
                    .on_hover_text(l.tr(
                        "Checks in Windows Registry which tweaks are currently active.",
                        "Vérifie dans le Registre Windows quels tweaks sont actuellement actifs.",
                    ))
                    .clicked()
                {
                    self.debloat_tweaks = crate::debloat::list_debloat_tweaks();
                }
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("↻ Reset all to default", "↻ Tout remettre par défaut")),
                    )
                    .on_hover_text(l.tr(
                        "Reverts all tweaks in this tab and restores original Windows 11 behavior.",
                        "Annule tous les tweaks de cet onglet et restaure le comportement d'origine de Windows 11.",
                    ))
                    .clicked()
                {
                    self.spawn_debloat_batch(false);
                }
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(
                            RichText::new(l.tr(
                                "★ Apply Recommended Tweaks",
                                "★ Appliquer les Tweaks Recommandés",
                            ))
                            .strong(),
                        ),
                    )
                    .on_hover_text(l.tr(
                        "Applies all recommended tweaks in one click.",
                        "Active d'un seul coup tous les tweaks marqués (Recommandé).",
                    ))
                    .clicked()
                {
                    self.spawn_debloat_batch(true);
                }
            });
        });
        ui.label(l.tr(
            "Click a toggle button to enable or revert a tweak instantly (hover [?] for details):",
            "Cliquez sur le bouton d'état d'un réglage pour l'activer ou l'annuler instantanément (survolez [?] pour les détails) :",
        ));
        ui.add_space(6.0);

        let categories = [
            DebloatCategory::AiAndCopilot,
            DebloatCategory::PrivacyAndTelemetry,
            DebloatCategory::ExplorerAndUi,
            DebloatCategory::SystemAndGaming,
        ];

        for cat in categories {
            ui.group(|ui| {
                ui.label(
                    RichText::new(debloat_category_title(l, cat))
                        .strong()
                        .color(Color32::from_rgb(130, 210, 255)),
                );
                ui.add_space(2.0);

                let snapshot: Vec<DebloatTweakStatus> = self
                    .debloat_tweaks
                    .iter()
                    .filter(|t| t.category == cat)
                    .cloned()
                    .collect();

                for tweak in snapshot {
                    ui.push_id(&tweak.id, |ui| {
                        let (t_title, t_desc, t_safety) = debloat_tweak_text(l, &tweak.id)
                            .unwrap_or((&tweak.title, &tweak.description, "✔ Safe Windows 11 registry setting"));

                        let short_desc = if let Some(enabled_line) = t_desc.lines().find(|line| {
                            line.starts_with("▪ If enabled:") || line.starts_with("▪ Si utilisé :") || line.starts_with("▪ Si utilisé:")
                        }) {
                            enabled_line
                                .trim_start_matches("▪ If enabled:")
                                .trim_start_matches("▪ Si utilisé :")
                                .trim_start_matches("▪ Si utilisé:")
                                .trim()
                        } else if let Some(first_line) = t_desc.lines().next() {
                            first_line
                                .trim_start_matches("▪ Default:")
                                .trim_start_matches("▪ Par défaut :")
                                .trim_start_matches("▪ Par défaut:")
                                .trim()
                        } else {
                            &tweak.description
                        };

                        ui.horizontal(|ui| {
                            let btn_text = if tweak.is_applied {
                                RichText::new(l.tr("✔ [ACTIVE]", "✔ [ACTIF]"))
                                    .color(Color32::from_rgb(80, 225, 130))
                                    .strong()
                            } else {
                                RichText::new(l.tr("  [DEFAULT]  ", "  [PAR DÉFAUT]  "))
                                    .color(Color32::from_rgb(165, 165, 165))
                            };

                            if ui
                                .add_enabled(!self.is_busy, egui::Button::new(btn_text))
                                .on_hover_ui(|ui| {
                                    show_rich_tooltip(
                                        ui,
                                        t_title,
                                        t_desc,
                                        t_safety,
                                        false,
                                    );
                                })
                                .clicked()
                            {
                                let runner = self.make_runner();
                                let target_state = !tweak.is_applied;
                                if crate::debloat::set_debloat_tweak(&runner, &tweak.id, target_state) {
                                    if !self.dry_run {
                                        self.debloat_tweaks = crate::debloat::list_debloat_tweaks();
                                    } else if let Some(item) = self
                                        .debloat_tweaks
                                        .iter_mut()
                                        .find(|i| i.id == tweak.id)
                                    {
                                        item.is_applied = target_state;
                                    }
                                }
                            }

                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(t_title).strong())
                                        .on_hover_ui(|ui| {
                                            show_rich_tooltip(
                                                ui,
                                                t_title,
                                                t_desc,
                                                t_safety,
                                                false,
                                            );
                                        });
                                    ui.label(
                                        RichText::new("[?]")
                                            .color(Color32::from_rgb(95, 195, 250))
                                            .small()
                                            .strong(),
                                    )
                                    .on_hover_ui(|ui| {
                                        show_rich_tooltip(
                                            ui,
                                            t_title,
                                            t_desc,
                                            t_safety,
                                            false,
                                        );
                                    });
                                    if tweak.recommended {
                                        ui.label(
                                            RichText::new(l.tr("(Recommended)", "(Recommandé)"))
                                                .color(Color32::from_rgb(100, 200, 255))
                                                .small(),
                                        );
                                    }
                                });
                                ui.label(RichText::new(short_desc).weak().small())
                                    .on_hover_ui(|ui| {
                                        show_rich_tooltip(
                                            ui,
                                            t_title,
                                            t_desc,
                                            t_safety,
                                            false,
                                        );
                                    });
                            });
                        });
                        ui.add_space(2.0);
                    });
                }
            });
            ui.add_space(4.0);
        }
    }

    fn render_apps_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.heading(l.tr(
            "Application Management & Bloatware Removal",
            "Gestion des Applications & Nettoyage Bloatwares",
        ));
        ui.label(l.tr(
            "Update installed desktop applications via WinGet and uninstall superfluous Windows 10/11 AppX bloatware.",
            "Mettez à jour vos applications installées via WinGet et désinstallez les bloatwares AppX superflus de Windows 10/11.",
        ));
        ui.add_space(8.0);

        // Section 1: WinGet Updates
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new(l.tr("📦 Software Upgrades (WinGet)", "📦 Mises à jour de logiciels (WinGet)"))
                        .color(Color32::from_rgb(100, 210, 255)),
                );
            });
            ui.add_space(2.0);
            ui.label(l.tr(
                "Detect and upgrade outdated software packages using official Windows Package Manager (winget).",
                "Détectez et mettez à jour vos logiciels obsolètes via le gestionnaire officiel Windows Package Manager (winget).",
            ));
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("🔍 Scan for Updates (WinGet)", "🔍 Scanner les mises à jour (WinGet)")),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Scan WinGet Updates", "Scanner les mises à jour WinGet"),
                            l.tr(
                                "▪ Default: No software upgrade scan is conducted until requested.\n▪ If enabled: Interrogates winget repository for available updates across all installed applications.\n▪ If skipped: Installed applications remain at their current versions.",
                                "▪ Par défaut : Aucun scan de mise à jour des logiciels n'est effectué tant qu'il n'est pas demandé.\n▪ Si utilisé : Interroge le dépôt officiel WinGet pour répertorier les nouvelles versions disponibles de vos applications.\n▪ Si non utilisé : Vos logiciels restent dans leurs versions actuelles.",
                            ),
                            l.tr("✔ 100% read-only scan", "✔ Scan 100 % en lecture seule"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_scan_winget();
                }

                let sel_count = self.selected_winget_ids.len();
                let upgrade_sel_label = match l {
                    Lang::En => format!("⬆ Upgrade Selected ({sel_count})"),
                    Lang::Fr => format!("⬆ Mettre à jour la sélection ({sel_count})"),
                };
                if ui
                    .add_enabled(
                        !self.is_busy && sel_count > 0,
                        egui::Button::new(upgrade_sel_label),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Upgrade Selected Packages", "Mettre à jour les paquets sélectionnés"),
                            l.tr(
                                "▪ Default: Selected packages are not upgraded.\n▪ If enabled: Runs silent installer upgrade for each checked package via winget.\n▪ If skipped: Checked packages remain unchanged.",
                                "▪ Par défaut : Les paquets sélectionnés ne sont pas mis à jour.\n▪ Si utilisé : Lance l'installation silencieuse de la dernière version pour chaque paquet coché via winget.\n▪ Si non utilisé : Les logiciels cochés restent inchangés.",
                            ),
                            l.tr("✔ Installs official verified packages", "✔ Installe les paquets officiels vérifiés"),
                            false,
                        );
                    })
                    .clicked()
                {
                    let ids: Vec<String> = self.selected_winget_ids.iter().cloned().collect();
                    self.spawn_upgrade_winget(ids);
                }

                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("🚀 Upgrade All (--silent)", "🚀 Tout mettre à jour (Upgrade All --silent)")),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Upgrade All Packages", "Mise à jour globale de tous les logiciels"),
                            l.tr(
                                "▪ Default: Software packages are only updated on demand.\n▪ If enabled: Executes winget upgrade --all --silent to update every outdated program on the machine.\n▪ If skipped: Outdated software remains at current versions.",
                                "▪ Par défaut : Les logiciels sont mis à jour au cas par cas.\n▪ Si utilisé : Lance winget upgrade --all --silent pour actualiser l'intégralité des applications obsolètes du système.\n▪ Si non utilisé : Les logiciels obsolètes ne sont pas mis à jour en masse.",
                            ),
                            l.tr("✔ Automated unattended upgrades", "✔ Mises à jour automatisées et sans invite"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_upgrade_all_winget();
                }



                if self.is_busy {
                    if ui
                        .button(
                            RichText::new(l.tr("⏹ Stop Upgrade", "⏹ Annuler MAJ"))
                                .color(Color32::from_rgb(255, 110, 110))
                                .strong(),
                        )
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Stop WinGet Upgrade", "Annuler les mises à jour WinGet"),
                                l.tr(
                                    "Aborts the ongoing WinGet upgrade immediately.\n▪ Default: Upgrades all selected packages sequentially.\n▪ If enabled: Kills the running winget.exe process and stops the upgrade queue.\n▪ If skipped: Continues upgrading remaining packages.",
                                    "Interrompt immédiatement les mises à jour WinGet en cours.\n▪ Par défaut : Met à jour tous les paquets sélectionnés à la suite.\n▪ Si utilisé : Tue le processus winget.exe en cours et stoppe la file d'attente.\n▪ Si non utilisé : Poursuit la mise à jour des paquets restants.",
                                ),
                                l.tr("✔ Stops winget.exe immediately", "✔ Arrête winget.exe immédiatement"),
                                true,
                            );
                        })
                        .clicked()
                    {
                        self.cancel_current_task();
                    }
                }

                if !self.winget_items.is_empty() {
                    ui.separator();
                    if ui
                        .small_button(l.tr("Select all", "Tout cocher"))
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Select All Upgrades", "Tout cocher (Mises à jour)"),
                                l.tr(
                                    "Marks all outdated applications for upgrade.\n▪ Default: No software pre-selected.\n▪ If enabled: Selects all detected outdated applications to update them in one click.\n▪ If skipped: Only manually selected applications will be upgraded.",
                                    "Sélectionne l'intégralité des applications obsolètes détectées.\n▪ Par défaut : Aucune application pré-sélectionnée.\n▪ Si utilisé : Coche tous les logiciels à mettre à jour en un clic.\n▪ Si non utilisé : Seules les applications cochées individuellement seront mises à jour.",
                                ),
                                l.tr("✔ Only updates detected outdated packages", "✔ Met à jour uniquement les paquets obsolètes"),
                                false,
                            );
                        })
                        .clicked()
                    {
                        for item in &self.winget_items {
                            if item.state != WingetPkgState::Updated {
                                self.selected_winget_ids.insert(item.id.clone());
                            }
                        }
                    }
                    if ui
                        .small_button(l.tr("Deselect all", "Tout décocher"))
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Deselect All Upgrades", "Tout décocher (Mises à jour)"),
                                l.tr(
                                    "Clears the upgrade selection.\n▪ Default: Cleared list.\n▪ If enabled: Unchecks every application in the upgrade list.\n▪ If skipped: Keeps current selection.",
                                    "Efface la sélection de mise à jour.\n▪ Par défaut : Aucun paquet coché.\n▪ Si utilisé : Décoche tous les logiciels de la liste.\n▪ Si non utilisé : Conserve la sélection actuelle.",
                                ),
                                l.tr("✔ UI selection only", "✔ Modification d'affichage uniquement"),
                                false,
                            );
                        })
                        .clicked()
                    {
                        self.selected_winget_ids.clear();
                    }
                }
            });

            ui.add_space(6.0);

            // Bandeau de résumé clair avec compteurs colorés si la liste n'est pas vide
            if !self.winget_items.is_empty() {
                let total_count = self.winget_items.len();
                let sel_count = self.selected_winget_ids.len();
                let in_progress_count = self
                    .winget_items
                    .iter()
                    .filter(|it| matches!(it.state, WingetPkgState::Upgrading(_) | WingetPkgState::Queued))
                    .count();
                let updated_count = self
                    .winget_items
                    .iter()
                    .filter(|it| matches!(it.state, WingetPkgState::Updated))
                    .count();
                let failed_count = self
                    .winget_items
                    .iter()
                    .filter(|it| matches!(it.state, WingetPkgState::Failed(_)))
                    .count();

                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("📦 {} : {}", l.tr("Total", "Total"), total_count))
                            .color(Color32::from_rgb(200, 205, 215))
                            .strong(),
                    );
                    ui.label(RichText::new("|").color(Color32::DARK_GRAY));
                    ui.label(
                        RichText::new(format!(
                            "☑ {} : {}",
                            l.tr("Selected", "Sélectionnés"),
                            sel_count
                        ))
                        .color(Color32::from_rgb(100, 185, 255))
                        .strong(),
                    );
                    ui.label(RichText::new("|").color(Color32::DARK_GRAY));
                    ui.label(
                        RichText::new(format!(
                            "🔄 {} : {}",
                            l.tr("In progress", "En cours"),
                            in_progress_count
                        ))
                        .color(Color32::from_rgb(245, 195, 65))
                        .strong(),
                    );
                    ui.label(RichText::new("|").color(Color32::DARK_GRAY));
                    ui.label(
                        RichText::new(format!(
                            "✔ {} : {}",
                            l.tr("Updated", "Mis à jour"),
                            updated_count
                        ))
                        .color(Color32::from_rgb(80, 230, 120))
                        .strong(),
                    );
                    ui.label(RichText::new("|").color(Color32::DARK_GRAY));
                    ui.label(
                        RichText::new(format!(
                            "⚠ {} : {}",
                            l.tr("Failed", "Échecs"),
                            failed_count
                        ))
                        .color(Color32::from_rgb(255, 110, 110))
                        .strong(),
                    );
                });
                ui.add_space(4.0);
            }

            // Encadré bleu/cyan bien visible juste au-dessus du tableau si un paquet est en cours d'installation
            if let Some(upg_item) = self
                .winget_items
                .iter()
                .find(|it| matches!(it.state, WingetPkgState::Upgrading(_)))
            {
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgba_unmultiplied(15, 35, 60, 230))
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 175, 255)))
                    .rounding(6.0_f32)
                    .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new(match l {
                                    Lang::En => format!(
                                        "🔄 Installing: {} ({} → {}) — Downloading & running official installer in background...",
                                        upg_item.name, upg_item.current_version, upg_item.available_version
                                    ),
                                    Lang::Fr => format!(
                                        "🔄 En cours d'installation : {} ({} → {}) — Téléchargement et exécution de l'installateur officiel en tâche de fond...",
                                        upg_item.name, upg_item.current_version, upg_item.available_version
                                    ),
                                })
                                .color(Color32::from_rgb(110, 220, 255))
                                .strong(),
                            );
                        });
                    });
                ui.add_space(4.0);
            }

            if self.winget_items.is_empty() {
                if self.winget_has_scanned {
                    ui.label(
                        RichText::new(l.tr(
                            "✔ All software is up to date! (0 outdated packages detected via WinGet)",
                            "✔ Tous vos logiciels sont à jour ! (0 mise à jour détectée via WinGet)",
                        ))
                        .color(Color32::from_rgb(80, 225, 130))
                        .strong(),
                    );
                } else {
                    ui.label(
                        RichText::new(l.tr(
                            "No updates detected or scan not performed yet. Click 'Scan for Updates (WinGet)' to check.",
                            "Aucune mise à jour détectée ou scan non effectué. Cliquez sur 'Scanner les mises à jour (WinGet)'.",
                        ))
                        .weak(),
                    );
                }
            } else {
                let mut single_upgrade_id: Option<String> = None;

                egui::ScrollArea::vertical()
                    .id_salt("winget_list_scroll")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        egui::Grid::new("winget_table_grid")
                            .striped(true)
                            .min_col_width(55.0)
                            .show(ui, |ui| {
                                ui.strong("");
                                ui.strong(l.tr("Application", "Application"));
                                ui.strong(l.tr("ID", "Identifiant"));
                                ui.strong(l.tr("Current", "Version Actuelle"));
                                ui.strong("");
                                ui.strong(l.tr("Available", "Disponible"));
                                ui.strong(l.tr("Status / Action", "État / Action"));
                                ui.end_row();

                                for item in &self.winget_items {
                                    let mut is_selected =
                                        self.selected_winget_ids.contains(&item.id);
                                    let item_name = item.name.clone();
                                    let item_id = item.id.clone();
                                    let cur_ver = item.current_version.clone();
                                    let avail_ver = item.available_version.clone();
                                    let is_updated = matches!(item.state, WingetPkgState::Updated);

                                    ui.push_id(&item.id, |ui| {
                                        if ui
                                            .add_enabled(
                                                !is_updated,
                                                egui::Checkbox::new(&mut is_selected, ""),
                                            )
                                            .on_hover_ui(|ui| {
                                                show_rich_tooltip(
                                                    ui,
                                                    &item_name,
                                                    &match l {
                                                        Lang::En => format!(
                                                            "ID: {}\n\n▪ Default: Keeps current version ({})\n▪ If enabled: Upgrades application to latest release ({})\n▪ If skipped: Retains current installed build without updating",
                                                            item_id, cur_ver, avail_ver
                                                        ),
                                                        Lang::Fr => format!(
                                                            "Identifiant : {}\n\n▪ Par défaut : Conserve la version actuelle ({})\n▪ Si utilisé : Met à jour l'application vers la dernière version ({})\n▪ Si non utilisé : L'application n'est pas mise à jour et reste dans sa version actuelle",
                                                            item_id, cur_ver, avail_ver
                                                        ),
                                                    },
                                                    l.tr(
                                                        "✔ Official WinGet package upgrade",
                                                        "✔ Mise à jour officielle via le dépôt WinGet",
                                                    ),
                                                    false,
                                                );
                                            })
                                            .changed()
                                        {
                                            if is_selected {
                                                self.selected_winget_ids.insert(item.id.clone());
                                            } else {
                                                self.selected_winget_ids.remove(&item.id);
                                            }
                                        }
                                    });
                                    ui.label(RichText::new(&item.name).strong());
                                    ui.label(RichText::new(&item.id).weak());
                                    ui.label(&item.current_version);
                                    ui.label("→");
                                    ui.label(
                                        RichText::new(&item.available_version)
                                            .color(Color32::from_rgb(80, 225, 130))
                                            .strong(),
                                    );

                                    // Colonne État / Action
                                    ui.horizontal(|ui| match &item.state {
                                        WingetPkgState::Available => {
                                            ui.label(RichText::new(l.tr("Ready", "Prêt")).weak());
                                            if ui
                                                .add_enabled(
                                                    !self.is_busy,
                                                    egui::Button::new(
                                                        RichText::new(
                                                            l.tr("⬆ Update", "⬆ Mettre à jour"),
                                                        )
                                                        .size(11.0),
                                                    ),
                                                )
                                                .on_hover_text(l.tr(
                                                    "Update only this application immediately",
                                                    "Mettre à jour uniquement cette application immédiatement",
                                                ))
                                                .clicked()
                                            {
                                                single_upgrade_id = Some(item.id.clone());
                                            }
                                        }
                                        WingetPkgState::Queued => {
                                            ui.label(
                                                RichText::new(
                                                    l.tr("⏳ Queued", "⏳ En file d'attente"),
                                                )
                                                .color(Color32::from_rgb(240, 200, 80))
                                                .strong(),
                                            );
                                        }
                                        WingetPkgState::Upgrading(stage) => {
                                            ui.spinner();
                                            let stage_text = if stage.is_empty() {
                                                l.tr("🔄 Installing...", "🔄 Installation en cours...")
                                            } else {
                                                stage.as_str()
                                            };
                                            ui.label(
                                                RichText::new(stage_text)
                                                    .color(Color32::from_rgb(70, 215, 255))
                                                    .strong(),
                                            );
                                        }
                                        WingetPkgState::Updated => {
                                            ui.label(
                                                RichText::new(
                                                    l.tr("✔ Updated!", "✔ Mis à jour !"),
                                                )
                                                .color(Color32::from_rgb(80, 230, 120))
                                                .strong(),
                                            );
                                        }
                                        WingetPkgState::Failed(err) => {
                                            let badge = ui.label(
                                                RichText::new(l.tr("⚠ Failed", "⚠ Échec"))
                                                    .color(Color32::from_rgb(255, 110, 110))
                                                    .strong(),
                                            );
                                            if !err.is_empty() {
                                                badge.on_hover_text(err.as_str());
                                            }
                                        }
                                        WingetPkgState::Cancelled => {
                                            ui.label(
                                                RichText::new(l.tr("⏹ Cancelled", "⏹ Annulé"))
                                                    .color(Color32::from_rgb(210, 150, 110)),
                                            );
                                        }
                                    });

                                    ui.end_row();
                                }
                            });
                    });

                if let Some(id) = single_upgrade_id {
                    self.selected_winget_ids.insert(id.clone());
                    self.spawn_upgrade_winget(vec![id]);
                }
            }
        });

        ui.add_space(10.0);

        // Section 2: AppX Bloatware
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new(l.tr("🗑 Pre-installed Bloatware Uninstaller (AppX)", "🗑 Désinstallateur de Bloatwares Windows préinstallés (AppX)"))
                        .color(Color32::from_rgb(255, 140, 140)),
                );
            });
            ui.add_space(2.0);
            ui.label(l.tr(
                "Safely removes non-essential pre-installed UWP/AppX packages and prevents automatic provisioning for new users.",
                "Supprime en toute sécurité les paquets UWP/AppX préinstallés superflus et bloque leur réinstallation pour les nouveaux profils.",
            ));
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr("🔍 Check Installed Bloatware", "🔍 Vérifier les Bloatwares installés")),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Check Installed Bloatware", "Vérifier les Bloatwares installés"),
                            l.tr(
                                "▪ Default: Bloatware presence is evaluated from default catalog.\n▪ If enabled: Scans Get-AppxPackage to detect which of the 14 known superfluous apps are actually installed.\n▪ If skipped: AppX installation status is not queried.",
                                "▪ Par défaut : La présence des bloatwares est affichée selon le catalogue par défaut.\n▪ Si utilisé : Interroge Get-AppxPackage pour détecter quelles applications superflues parmi les 14 sont réellement installées.\n▪ Si non utilisé : L'état d'installation AppX n'est pas interrogé.",
                            ),
                            l.tr("✔ 100% read-only scan", "✔ Scan 100 % en lecture seule"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_scan_appx();
                }

                let sel_appx_count = self.selected_appx_patterns.len();
                let uninst_label = match l {
                    Lang::En => format!("🗑 Uninstall Checked Bloatware ({sel_appx_count})"),
                    Lang::Fr => format!("🗑 Désinstaller les Bloatwares cochés ({sel_appx_count})"),
                };
                if ui
                    .add_enabled(
                        !self.is_busy && sel_appx_count > 0,
                        egui::Button::new(uninst_label),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Uninstall Selected Bloatware", "Désinstaller les Bloatwares cochés"),
                            l.tr(
                                "▪ Default: Pre-installed AppX apps remain present on current and future user accounts.\n▪ If enabled: Removes chosen AppX packages and provisioned templates for current and new user accounts.\n▪ If skipped: No apps will be uninstalled.",
                                "▪ Par défaut : Les applications AppX préinstallées restent présentes sur les comptes utilisateurs.\n▪ Si utilisé : Supprime les paquets AppX et leurs modèles provisionnés pour l'utilisateur actuel et les futurs comptes.\n▪ Si non utilisé : Aucune application n'est désinstallée.",
                            ),
                            l.tr("✔ Safe removal without affecting core Windows features", "✔ Suppression propre sans impacter les composants essentiels de Windows"),
                            false,
                        );
                    })
                    .clicked()
                {
                    let patterns: Vec<String> = self.selected_appx_patterns.iter().cloned().collect();
                    self.spawn_remove_appx(patterns);
                }

                ui.separator();
                if ui
                    .small_button(l.tr("Select installed", "Cocher installés"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Select Installed Bloatware", "Cocher les bloatwares installés"),
                            l.tr(
                                "Selects only the pre-installed bloatware packages currently detected on the system.\n▪ Default: Unchecked until detected.\n▪ If enabled: Checks all packages marked as [Installed] for removal.\n▪ If skipped: Selection remains unchanged.",
                                "Sélectionne uniquement les bloatwares actuellement détectés comme installés sur la machine.\n▪ Par défaut : Non coché tant que non détecté.\n▪ Si utilisé : Coche tous les paquets marqués comme [Installé] pour suppression.\n▪ Si non utilisé : La sélection reste inchangée.",
                            ),
                            l.tr(
                                "✔ Targets only installed bloatware",
                                "✔ Cible uniquement les applications réellement présentes",
                            ),
                            false,
                        );
                    })
                    .clicked()
                {
                    for item in &self.appx_items {
                        if item.installed {
                            self.selected_appx_patterns.insert(item.package_pattern.clone());
                        }
                    }
                }
                if ui
                    .small_button(l.tr("Select all", "Tout cocher"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Select All Bloatware", "Tout cocher (Bloatwares)"),
                            l.tr(
                                "Selects all 14 catalog bloatware packages for removal and de-provisioning.\n▪ Default: Nothing selected.\n▪ If enabled: All catalog packages are selected.\n▪ If skipped: Selection remains unchanged.",
                                "Sélectionne la totalité des 14 bloatwares du catalogue pour suppression et dé-provisionnement.\n▪ Par défaut : Rien n'est sélectionné.\n▪ Si utilisé : Tous les bloatwares du catalogue sont sélectionnés.\n▪ Si non utilisé : La sélection reste inchangée.",
                            ),
                            l.tr(
                                "✔ Safe Windows 11 bloatware catalog",
                                "✔ Catalogue de bloatwares Windows 11 sans risque",
                            ),
                            false,
                        );
                    })
                    .clicked()
                {
                    for item in &self.appx_items {
                        self.selected_appx_patterns.insert(item.package_pattern.clone());
                    }
                }
                if ui
                    .small_button(l.tr("Deselect all", "Tout décocher"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Deselect All Bloatware", "Tout décocher (Bloatwares)"),
                            l.tr(
                                "Clears all bloatware checkboxes.\n▪ Default: Empty selection.\n▪ If enabled: Unchecks all packages.\n▪ If skipped: Keeps current selection.",
                                "Décoche tous les bloatwares de la liste.\n▪ Par défaut : Sélection vide.\n▪ Si utilisé : Décoche tous les paquets.\n▪ Si non utilisé : Conserve la sélection actuelle.",
                            ),
                            l.tr("✔ UI selection only", "✔ Modification d'affichage uniquement"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.selected_appx_patterns.clear();
                }
            });

            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("appx_list_scroll")
                .max_height(350.0)
                .show(ui, |ui| {
                    egui::Grid::new("appx_table_grid")
                        .striped(true)
                        .min_col_width(50.0)
                        .show(ui, |ui| {
                            ui.strong("");
                            ui.strong(l.tr("Application", "Application"));
                            ui.strong(l.tr("Status", "État"));
                            ui.strong(l.tr("Description", "Description"));
                            ui.strong("");
                            ui.end_row();

                            for item in &self.appx_items {
                                let mut is_selected = self.selected_appx_patterns.contains(&item.package_pattern);
                                let item_title = &item.display_name;
                                let details = match l {
                                    Lang::En => format!(
                                        "▪ Default: Pre-installed with Windows 10/11.\n▪ If enabled: Completely removes {} from the system and disables automated provisioning.\n▪ If skipped: Retains the application on the system.",
                                        item.display_name
                                    ),
                                    Lang::Fr => format!(
                                        "▪ Par défaut : Préinstallé avec Windows 10/11.\n▪ Si utilisé : Supprime complètement {} du système et empêche son approvisionnement automatique.\n▪ Si non utilisé : Conserve l'application sur le système.",
                                        item.display_name
                                    ),
                                };
                                let safety = l.tr("✔ Can be reinstalled from Microsoft Store if ever needed", "✔ Réinstallable depuis le Microsoft Store si nécessaire");

                                ui.push_id(&item.package_pattern, |ui| {
                                    if ui
                                        .checkbox(&mut is_selected, "")
                                        .on_hover_ui(|ui| {
                                            show_rich_tooltip(ui, item_title, &details, safety, false);
                                        })
                                        .changed()
                                    {
                                        if is_selected {
                                            self.selected_appx_patterns.insert(item.package_pattern.clone());
                                        } else {
                                            self.selected_appx_patterns.remove(&item.package_pattern);
                                        }
                                    }
                                });

                                ui.label(RichText::new(&item.display_name).strong());

                                if item.installed {
                                    ui.label(RichText::new(l.tr("[Installed]", "[Installé]")).color(Color32::from_rgb(255, 120, 120)).strong());
                                } else {
                                    ui.label(RichText::new(l.tr("[Not installed]", "[Non installé]")).color(Color32::from_rgb(100, 210, 120)));
                                }

                                let desc = if l == Lang::Fr {
                                    &item.description_fr
                                } else {
                                    &item.description_en
                                };
                                ui.label(desc);

                                ui.push_id(&item.package_pattern, |ui| {
                                    ui.label(RichText::new("[?]").color(Color32::from_rgb(95, 195, 250)).small().strong())
                                        .on_hover_ui(|ui| {
                                            show_rich_tooltip(ui, item_title, &details, safety, false);
                                        });
                                });

                                ui.end_row();
                            }
                        });
                });
        });
    }

    fn render_gaming_hardware_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.heading(l.tr(
            "Gaming Optimization, Latency & Hardware Health",
            "Optimisation Gaming, Latence & Matériel",
        ));
        ui.label(l.tr(
            "GPU shader caches purging, low latency OS settings, battery capacity analysis, and storage SMART counters.",
            "Purge des caches shaders GPU, réglages OS basse latence, santé batterie et compteurs SMART des disques.",
        ));
        ui.add_space(8.0);

        // Section 1: Gaming Optimizations & Shader Caches
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new(l.tr(
                        "🎮 Gaming Optimizations, Latency & GPU Shader Caches",
                        "🎮 Optimisations Gaming, Latence & Caches Shaders GPU",
                    ))
                    .color(Color32::from_rgb(255, 195, 90)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !self.is_busy,
                            egui::Button::new(l.tr(
                                "🔄 Refresh Gaming Status & Shader Caches",
                                "🔄 Actualiser l'état Gaming & Caches Shaders",
                            )),
                        )
                        .on_hover_text(l.tr(
                            "Queries registry and measures current DirectX/NVIDIA/AMD shader caches sizes.",
                            "Interroge le registre et mesure la taille actuelle des caches shaders DirectX/NVIDIA/AMD.",
                        ))
                        .clicked()
                    {
                        self.spawn_refresh_gaming_status();
                    }
                });
            });
            ui.add_space(4.0);

            ui.columns(2, |cols| {
                // Card: GPU Shader Cache
                cols[0].group(|ui| {
                    ui.label(RichText::new(l.tr("• GPU Shader Cache Cleaner", "• Nettoyeur de Cache Shaders GPU")).strong());
                    let sz = self.gaming_status.as_ref().map(|s| s.shader_cache_size_mb).unwrap_or(0);
                    ui.label(format!("{}: {} MB", l.tr("Shader cache size", "Taille du cache de shaders"), sz));
                    ui.add_space(2.0);
                    ui.label(l.tr(
                        "Covers NVIDIA (DXCache, GLCache), AMD (DxCache, OglCache), and DirectX (D3DSCache).",
                        "Nettoie NVIDIA (DXCache, GLCache), AMD (DxCache, OglCache) et DirectX (D3DSCache).",
                    ));
                    ui.add_space(4.0);

                    if ui
                        .add_enabled(
                            !self.is_busy,
                            egui::Button::new(l.tr(
                                "🧹 Purge GPU Shader Caches",
                                "🧹 Purger les Caches de Shaders GPU",
                            )),
                        )
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Purge GPU Shader Caches", "Purger les Caches de Shaders GPU"),
                                l.tr(
                                    "▪ Default: GPU shader cache files accumulate over time in DirectX, NVIDIA, and AMD driver folders.\n▪ If enabled: Safely deletes compiled shader caches to resolve stuttering, corrupted textures, and reclaim disk space (rebuilds automatically on next game launch).\n▪ If skipped: Existing compiled shader caches are kept.",
                                    "▪ Par défaut : Les fichiers de shaders compilés s'accumulent indéfiniment dans les répertoires DirectX, NVIDIA et AMD.\n▪ Si utilisé : Supprime les caches obsolètes pour corriger les micro-saccades (stuttering), textures corrompues et libérer de l'espace (recompilés automatiquement au prochain lancement de jeu).\n▪ Si non utilisé : Conserve les caches de shaders existants.",
                                ),
                                l.tr("✔ 100% safe: graphics drivers recreate shaders cleanly upon game start", "✔ 100 % sans danger : les pilotes graphiques recompilent les shaders proprement au démarrage d'un jeu"),
                                false,
                            );
                        })
                        .clicked()
                    {
                        self.spawn_clean_shader_caches();
                    }
                });

                // Card: Ultimate Performance Power Plan
                cols[1].group(|ui| {
                    ui.label(RichText::new(l.tr("• Ultimate Performance Power Plan", "• Plan d'alimentation Performances Ultimes")).strong());
                    let is_active = self.gaming_status.as_ref().map(|s| s.ultimate_perf_active).unwrap_or(false);
                    let (status_str, status_color) = if is_active {
                        (l.tr("Active", "Actif"), Color32::from_rgb(80, 225, 130))
                    } else {
                        (l.tr("Inactive", "Inactif"), Color32::from_rgb(180, 180, 180))
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{}: ", l.tr("Status", "État")));
                        ui.label(RichText::new(status_str).color(status_color).strong());
                    });
                    ui.add_space(2.0);
                    ui.label(l.tr(
                        "Eliminates micro-latencies by disabling core parking and power throttling.",
                        "Élimine les micro-latences en désactivant le parcage des cœurs et le bridage énergétique.",
                    ));
                    ui.add_space(4.0);

                    if ui
                        .add_enabled(
                            !self.is_busy,
                            egui::Button::new(l.tr(
                                "⚡ Unlock & Enable Ultimate Performance",
                                "⚡ Débloquer & Activer Performances Ultimes",
                            )),
                        )
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                l.tr("Unlock & Enable Ultimate Performance Power Plan", "Débloquer & Activer Performances Ultimes"),
                                l.tr(
                                    "▪ Default: Windows uses Balanced or High Performance plan with CPU frequency scaling and micro-sleep states.\n▪ If enabled: Unlocks the hidden OEM Ultimate Performance power scheme (GUID e9a42b02-d5df-448d-aa00-03f14749eb61) to eliminate CPU throttling and reduce latency.\n▪ If skipped: Retains currently active power plan.",
                                    "▪ Par défaut : Windows utilise le mode Équilibré ou Performances Élevées avec bridage d'horloge et micro-sommeil du CPU.\n▪ Si utilisé : Débloque le plan caché constructeur Performances Ultimes (GUID e9a42b02-d5df-448d-aa00-03f14749eb61) pour éliminer le throttling et réduire la latence.\n▪ Si non utilisé : Conserve le plan d'alimentation actuellement actif.",
                                ),
                                l.tr("✔ Reversible in Restore tab (sets back to Balanced)", "✔ Réversible dans l'onglet Restauration (remet le mode Équilibré)"),
                                false,
                            );
                        })
                        .clicked()
                    {
                        self.spawn_enable_ultimate_plan();
                    }
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(2.0);
            ui.label(RichText::new(l.tr("Low-Latency OS Gaming Switches", "Interrupteurs Gaming Basse Latence Windows")).strong());
            ui.add_space(4.0);

            // HAGS
            let hags_on = self.gaming_status.as_ref().map(|s| s.hags_enabled).unwrap_or(false);
            ui.horizontal(|ui| {
                let hags_btn_text = if hags_on {
                    RichText::new(l.tr("HAGS (GPU Scheduling) [ON]", "HAGS (Planification GPU) [ON]"))
                        .color(Color32::from_rgb(80, 225, 130)).strong()
                } else {
                    RichText::new(l.tr("HAGS (GPU Scheduling) [OFF]", "HAGS (Planification GPU) [OFF]"))
                        .color(Color32::from_rgb(180, 180, 180))
                };
                if ui
                    .add_enabled(!self.is_busy, egui::Button::new(hags_btn_text))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "HAGS (Hardware-Accelerated GPU Scheduling)",
                            l.tr(
                                "▪ Default: CPU coordinates and schedules GPU rendering tasks, introducing slight scheduling overhead.\n▪ If enabled: Offloads frame scheduling directly to a dedicated GPU scheduling processor, reducing frame-time latency and enabling DLSS 3 Frame Generation.\n▪ If skipped: Retains standard CPU-based GPU scheduling.",
                                "▪ Par défaut : Le processeur CPU planifie et distribue les tâches de rendu GPU, induisant une légère latence de synchronisation.\n▪ Si utilisé : Confie l'ordonnancement directement au processeur dédié du GPU, réduisant la latence d'affichage et activant la Génération d'images DLSS 3.\n▪ Si non utilisé : Conserve la planification GPU classique par le CPU.",
                            ),
                            l.tr("⚠ Requires PC reboot to take effect", "⚠ Nécessite un redémarrage du PC pour prendre effet"),
                            true,
                        );
                    })
                    .clicked()
                {
                    self.spawn_set_hags(!hags_on);
                }

                ui.label(RichText::new("[?]").color(Color32::from_rgb(95, 195, 250)).small().strong())
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "HAGS (Hardware-Accelerated GPU Scheduling)",
                            l.tr(
                                "▪ Default: CPU coordinates and schedules GPU rendering tasks, introducing slight scheduling overhead.\n▪ If enabled: Offloads frame scheduling directly to a dedicated GPU scheduling processor, reducing frame-time latency and enabling DLSS 3 Frame Generation.\n▪ If skipped: Retains standard CPU-based GPU scheduling.",
                                "▪ Par défaut : Le processeur CPU planifie et distribue les tâches de rendu GPU, induisant une légère latence de synchronisation.\n▪ Si utilisé : Confie l'ordonnancement directement au processeur dédié du GPU, réduisant la latence d'affichage et activant la Génération d'images DLSS 3.\n▪ Si non utilisé : Conserve la planification GPU classique par le CPU.",
                            ),
                            l.tr("⚠ Requires PC reboot to take effect", "⚠ Nécessite un redémarrage du PC pour prendre effet"),
                            true,
                        );
                    });

                ui.add_space(16.0);

                // VBS
                let vbs_on = self.gaming_status.as_ref().map(|s| s.vbs_enabled).unwrap_or(false);
                let vbs_btn_text = if vbs_on {
                    RichText::new(l.tr("VBS / Core Isolation [ON]", "VBS / Isolation Noyau [ON]"))
                        .color(Color32::from_rgb(80, 225, 130)).strong()
                } else {
                    RichText::new(l.tr("VBS / Core Isolation [OFF]", "VBS / Isolation Noyau [OFF]"))
                        .color(Color32::from_rgb(255, 120, 120)).strong()
                };
                if ui
                    .add_enabled(!self.is_busy, egui::Button::new(vbs_btn_text))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("VBS / Core Isolation (Virtualization-Based Security)", "VBS / Isolation du noyau (Virtualization-Based Security)"),
                            l.tr(
                                "▪ Default: Windows 11 enables Virtualization-Based Security and Hypervisor-Enforced Code Integrity (HVCI) by default.\n▪ If enabled: Disabling VBS frees hypervisor CPU overhead, providing a 5% to 15% FPS boost in CPU-heavy games, but lowers kernel exploit mitigation defenses.\n▪ If skipped: Keeps Microsoft virtualization security active for maximum enterprise security.",
                                "▪ Par défaut : Windows 11 active par défaut la sécurité basée sur la virtualisation et l'intégrité de la mémoire (HVCI).\n▪ Si utilisé : La désactivation de VBS libère la surcharge de l'hyperviseur CPU, offrant un gain de 5 % à 15 % de FPS dans les jeux gourmands en processeur, mais réduit l'isolation de sécurité du noyau.\n▪ Si non utilisé : Conserve la sécurité renforcée Microsoft contre les exploits noyau avancés.",
                            ),
                            l.tr("⚠ Major FPS impact in gaming vs enterprise security trade-off. Requires reboot.", "⚠ Compromis FPS en jeu vs protection entreprise. Nécessite un redémarrage."),
                            true,
                        );
                    })
                    .clicked()
                {
                    self.spawn_set_vbs(!vbs_on);
                }

                ui.label(RichText::new("[?]").color(Color32::from_rgb(95, 195, 250)).small().strong())
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("VBS / Core Isolation (Virtualization-Based Security)", "VBS / Isolation du noyau (Virtualization-Based Security)"),
                            l.tr(
                                "▪ Default: Windows 11 enables Virtualization-Based Security and Hypervisor-Enforced Code Integrity (HVCI) by default.\n▪ If enabled: Disabling VBS frees hypervisor CPU overhead, providing a 5% to 15% FPS boost in CPU-heavy games, but lowers kernel exploit mitigation defenses.\n▪ If skipped: Keeps Microsoft virtualization security active for maximum enterprise security.",
                                "▪ Par défaut : Windows 11 active par défaut la sécurité basée sur la virtualisation et l'intégrité de la mémoire (HVCI).\n▪ Si utilisé : La désactivation de VBS libère la surcharge de l'hyperviseur CPU, offrant un gain de 5 % à 15 % de FPS dans les jeux gourmands en processeur, mais réduit l'isolation de sécurité du noyau.\n▪ Si non utilisé : Conserve la sécurité renforcée Microsoft contre les exploits noyau avancés.",
                            ),
                            l.tr("⚠ Major FPS impact in gaming vs enterprise security trade-off. Requires reboot.", "⚠ Compromis FPS en jeu vs protection entreprise. Nécessite un redémarrage."),
                            true,
                        );
                    });

                ui.add_space(16.0);

                // Game Mode
                let gm_on = self.gaming_status.as_ref().map(|s| s.game_mode_enabled).unwrap_or(true);
                let gm_btn_text = if gm_on {
                    RichText::new(l.tr("Windows Game Mode [ON]", "Mode Jeu Windows [ON]"))
                        .color(Color32::from_rgb(80, 225, 130)).strong()
                } else {
                    RichText::new(l.tr("Windows Game Mode [OFF]", "Mode Jeu Windows [OFF]"))
                        .color(Color32::from_rgb(180, 180, 180))
                };
                if ui
                    .add_enabled(!self.is_busy, egui::Button::new(gm_btn_text))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Windows Game Mode", "Mode Jeu Windows"),
                            l.tr(
                                "▪ Default: Windows Game Mode is enabled by default in Windows 10/11.\n▪ If enabled: Prioritizes CPU and GPU resources for foreground games and prevents background updates or driver restarts while gaming.\n▪ If skipped: Treats games with normal background task priority.",
                                "▪ Par défaut : Le Mode Jeu Windows est activé par défaut dans Windows 10/11.\n▪ Si utilisé : Priorise l'allocation des cœurs CPU et du GPU pour les jeux au premier plan et suspend les tâches/mises à jour en arrière-plan pendant la partie.\n▪ Si non utilisé : Traite les jeux avec la priorité système standard.",
                            ),
                            l.tr("✔ Recommended ON for gaming stability", "✔ Recommandé ACTIF pour la stabilité en jeu"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_set_game_mode(!gm_on);
                }

                ui.label(RichText::new("[?]").color(Color32::from_rgb(95, 195, 250)).small().strong())
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Windows Game Mode", "Mode Jeu Windows"),
                            l.tr(
                                "▪ Default: Windows Game Mode is enabled by default in Windows 10/11.\n▪ If enabled: Prioritizes CPU and GPU resources for foreground games and prevents background updates or driver restarts while gaming.\n▪ If skipped: Treats games with normal background task priority.",
                                "▪ Par défaut : Le Mode Jeu Windows est activé par défaut dans Windows 10/11.\n▪ Si utilisé : Priorise l'allocation des cœurs CPU et du GPU pour les jeux au premier plan et suspend les tâches/mises à jour en arrière-plan pendant la partie.\n▪ Si non utilisé : Traite les jeux avec la priorité système standard.",
                            ),
                            l.tr("✔ Recommended ON for gaming stability", "✔ Recommandé ACTIF pour la stabilité en jeu"),
                            false,
                        );
                    });
            });
        });

        ui.add_space(10.0);

        // Section 2: Battery & SMART Health
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new(l.tr(
                        "🔋 Battery Health & Storage S.M.A.R.T.",
                        "🔋 Santé Batterie & S.M.A.R.T. SSD/NVMe",
                    ))
                    .color(Color32::from_rgb(110, 230, 160)),
                );
            });
            ui.add_space(2.0);
            ui.label(l.tr(
                "Inspect battery wear level, design vs actual capacity, NVMe/SSD wear percentage, temperatures, and hardware read/write error logs.",
                "Inspectez le niveau d'usure de la batterie, la capacité d'usine vs actuelle, l'usure des NVMe/SSD, les températures et les compteurs d'erreurs matérielles.",
            ));
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr(
                            "🔍 Scan Battery & S.M.A.R.T. Disks",
                            "🔍 Analyser Batterie & S.M.A.R.T. Disques",
                        )),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Scan Battery & S.M.A.R.T. Disks", "Analyser Batterie & S.M.A.R.T. Disques"),
                            l.tr(
                                "▪ Default: Hardware wear and battery telemetry are queried on demand.\n▪ If enabled: Reads battery capacity (design vs full charge) and disk health, wear %, temperature, and I/O error counters.\n▪ If skipped: Hardware telemetry is not queried.",
                                "▪ Par défaut : L'usure matérielle et la télémétrie de batterie sont interrogées sur demande.\n▪ Si utilisé : Interroge la capacité de batterie (d'usine vs pleine charge) et la santé des disques, usure %, température et erreurs E/S.\n▪ Si non utilisé : La télémétrie matérielle n'est pas interrogée.",
                            ),
                            l.tr("✔ 100% read-only diagnostic", "✔ Diagnostic 100 % en lecture seule"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_scan_battery_smart();
                }

                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(l.tr(
                            "📄 Generate Battery HTML Report (logs/)",
                            "📄 Générer le rapport HTML Batterie (logs/)",
                        )),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Generate Windows Battery Report", "Générer le rapport de batterie Windows"),
                            l.tr(
                                "▪ Default: No battery report file is generated.\n▪ If enabled: Executes powercfg /batteryreport to generate a complete official HTML diagnostic on your Desktop and automatically opens it in your default browser.\n▪ If skipped: No battery report is saved.",
                                "▪ Par défaut : Aucun rapport de batterie n'est généré sur le disque.\n▪ Si utilisé : Exécute powercfg /batteryreport pour produire un diagnostic HTML officiel complet sur votre Bureau et l'ouvre dans votre navigateur.\n▪ Si non utilisé : Aucun rapport de batterie n'est créé.",
                            ),
                            l.tr("✔ Official Microsoft diagnostic tool", "✔ Outil officiel de diagnostic Microsoft"),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.spawn_generate_battery_report();
                }
            });

            ui.add_space(6.0);

            if let Some(info) = &self.battery_smart {
                // Battery details
                ui.group(|ui| {
                    ui.label(RichText::new(l.tr("• Battery Health Status", "• État de Santé de la Batterie")).strong());
                    if info.has_battery {
                        ui.label(format!("{}: {}", l.tr("Model / ID", "Modèle / Identifiant"), info.battery_name));
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{}: {} mWh  |  {}: {} mWh  |  ",
                                l.tr("Design Capacity", "Capacité d'usine"),
                                info.design_capacity_mwh,
                                l.tr("Full Charge", "Pleine charge"),
                                info.full_charge_capacity_mwh
                            ));

                            let h_color = if info.battery_health_pct >= 80.0 {
                                Color32::from_rgb(80, 225, 120)
                            } else if info.battery_health_pct >= 50.0 {
                                Color32::from_rgb(255, 200, 70)
                            } else {
                                Color32::from_rgb(255, 100, 100)
                            };
                            ui.label(format!("{}: ", l.tr("Real Health", "Santé réelle")));
                            ui.label(RichText::new(format!("{:.1}%", info.battery_health_pct)).color(h_color).strong());
                        });
                    } else {
                        ui.label(RichText::new(l.tr(
                            "Aucune batterie détectée (PC fixe)",
                            "Aucune batterie détectée (PC fixe)",
                        )).color(Color32::LIGHT_GRAY));
                    }
                });

                ui.add_space(4.0);

                // SMART Disks details
                ui.group(|ui| {
                    ui.label(RichText::new(l.tr("• Physical Disks S.M.A.R.T. Counters", "• Compteurs S.M.A.R.T. des Disques Physiques")).strong());
                    if info.smart_disks.is_empty() {
                        ui.label(RichText::new(l.tr(
                            "No physical disks returned by storage telemetry.",
                            "Aucun disque physique détecté via la télémétrie de stockage.",
                        )).weak());
                    } else {
                        egui::Grid::new("smart_disks_grid")
                            .striped(true)
                            .min_col_width(70.0)
                            .show(ui, |ui| {
                                ui.strong(l.tr("Model", "Modèle"));
                                ui.strong(l.tr("Media Type", "Type"));
                                ui.strong(l.tr("Health", "Santé"));
                                ui.strong(l.tr("Temperature", "Température"));
                                ui.strong(l.tr("Wear %", "Usure %"));
                                ui.strong(l.tr("Read / Write Errors", "Erreurs Lecture / Écriture"));
                                ui.end_row();

                                for disk in &info.smart_disks {
                                    ui.label(RichText::new(&disk.friendly_name).strong());
                                    ui.label(&disk.media_type);

                                    let is_healthy = disk.health_status.to_lowercase().contains("healthy")
                                        || disk.health_status.to_lowercase().contains("sain");
                                    let h_color = if is_healthy {
                                        Color32::from_rgb(80, 225, 120)
                                    } else {
                                        Color32::from_rgb(255, 100, 100)
                                    };
                                    ui.label(RichText::new(&disk.health_status).color(h_color).strong());

                                    let temp_str = match disk.temperature_c {
                                        Some(t) => format!("{t} °C"),
                                        None => "N/A".to_string(),
                                    };
                                    ui.label(temp_str);

                                    let wear_str = match disk.wear_pct {
                                        Some(w) => format!("{w} %"),
                                        None => "N/A".to_string(),
                                    };
                                    ui.label(wear_str);

                                    ui.label(format!("{}/{}", disk.read_errors, disk.write_errors));
                                    ui.end_row();
                                }
                            });
                    }
                });
            } else {
                ui.label(RichText::new(l.tr(
                    "Click 'Scan Battery & S.M.A.R.T. Disks' to retrieve hardware health telemetry.",
                    "Cliquez sur 'Analyser Batterie & S.M.A.R.T. Disques' pour récupérer la télémétrie de santé matérielle.",
                )).weak());
            }
        });
    }

    fn render_restore_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.heading(l.tr(
            "Restore Default Settings (.RESTORE)",
            "Restauration des paramètres par défaut (.RESTORE)",
        ));
        ui.label(l.tr(
            "Revert at any time optimizations applied by Boost-PC or Boost-Internet (hover [?] for details).",
            "Permet d'annuler à tout moment les optimisations appliquées par Boost-PC ou Boost-Internet (survolez [?] pour les détails).",
        ));
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(RichText::new(l.tr("• System Settings (Boost-PC)", "• Paramètres Système (Boost-PC)")).strong());
                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_power_balanced,
                    l.tr(
                        "Restore Balanced power plan (SCHEME_BALANCED)",
                        "Restaurer le plan d'alimentation Équilibré (SCHEME_BALANCED)",
                    ),
                    l.tr(
                        "Restore Balanced Power Plan",
                        "Restauration du plan d'alimentation Équilibré",
                    ),
                    l.tr(
                        "▪ Default: System is running on High Performance or Ultimate Performance power scheme.\n▪ If enabled: Activates the official Windows Balanced power scheme (SCHEME_BALANCED).\n▪ If skipped: Keeps current High or Ultimate Performance power plan active.",
                        "▪ Par défaut : Le PC utilise un profil Performances élevées ou Ultimate Performance.\n▪ Si utilisé : Réactive le mode d'alimentation Équilibré officiel de Windows (SCHEME_BALANCED).\n▪ Si non utilisé : Conserve le plan d'alimentation haute performance actuellement actif.",
                    ),
                    l.tr("✔ Original Windows default", "✔ Valeur d'origine Windows"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_visual_fx,
                    l.tr(
                        "Restore automatic Windows visual effects (VisualFXSetting = 0)",
                        "Restaurer les effets visuels automatiques de Windows (VisualFXSetting = 0)",
                    ),
                    l.tr(
                        "Restore Default Visual Effects",
                        "Restauration des effets visuels par défaut",
                    ),
                    l.tr(
                        "▪ Default: Visual effects are stripped down to Best Performance (animations disabled).\n▪ If enabled: Restores default Windows visual effects, animations, and drop shadows (VisualFXSetting = 0).\n▪ If skipped: Keeps fast minimal visual effects without animations.",
                        "▪ Par défaut : Les effets visuels sont réduits au mode Meilleures performances (animations coupées).\n▪ Si utilisé : Restaure les animations et ombres portées par défaut de Windows (VisualFXSetting = 0).\n▪ Si non utilisé : Conserve l'interface réactive et allégée sans animations.",
                    ),
                    l.tr("✔ Original Windows default", "✔ Valeur d'origine Windows"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_transparency,
                    l.tr(
                        "Re-enable Windows transparency (EnableTransparency = 1)",
                        "Réactiver la transparence Windows (EnableTransparency = 1)",
                    ),
                    l.tr(
                        "Re-enable Transparency",
                        "Réactivation de la transparence",
                    ),
                    l.tr(
                        "▪ Default: Transparency blur effects are disabled to save GPU resources.\n▪ If enabled: Re-enables Mica and Acrylic transparency on taskbar and windows (EnableTransparency = 1).\n▪ If skipped: Leaves transparency disabled for lighter graphics rendering.",
                        "▪ Par défaut : Les effets de transparence sont désactivés pour alléger le rendu graphique.\n▪ Si utilisé : Réactive la transparence Mica et Acrylique sur la barre des tâches et fenêtres (EnableTransparency = 1).\n▪ Si non utilisé : Maintient la transparence désactivée pour économiser les ressources GPU.",
                    ),
                    l.tr("✔ Original Windows default", "✔ Valeur d'origine Windows"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_sysmain,
                    l.tr(
                        "Re-enable and start SysMain service (Superfetch)",
                        "Réactiver et démarrer le service SysMain (Superfetch)",
                    ),
                    l.tr(
                        "Re-enable SysMain Service",
                        "Réactivation du service SysMain",
                    ),
                    l.tr(
                        "▪ Default: SysMain (Superfetch) service is stopped and disabled in Windows Services.\n▪ If enabled: Sets SysMain startup type back to Automatic and starts the service immediately.\n▪ If skipped: Leaves SysMain service disabled, preventing background caching.",
                        "▪ Par défaut : Le service SysMain (Superfetch) est arrêté et désactivé dans les services Windows.\n▪ Si utilisé : Remet le service SysMain en démarrage Automatique et le démarre immédiatement.\n▪ Si non utilisé : Laisse le service SysMain désactivé sans préchargement d'applications.",
                    ),
                    l.tr("✔ Original Windows default", "✔ Valeur d'origine Windows"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_storage_sense,
                    l.tr(
                        "Disable automatic Storage Sense",
                        "Désactiver Storage Sense automatique",
                    ),
                    l.tr(
                        "Disable Storage Sense",
                        "Désactivation de Storage Sense",
                    ),
                    l.tr(
                        "▪ Default: Storage Sense automated cleanup is active and scheduled every 30 days.\n▪ If enabled: Disables automated Storage Sense cleanup policy in Windows settings.\n▪ If skipped: Keeps Storage Sense enabled to purge temporary files periodically.",
                        "▪ Par défaut : L'Assistant Stockage automatique est configuré pour purger les fichiers tous les 30 jours.\n▪ Si utilisé : Désactive le nettoyage automatique périodique de l'Assistant Stockage.\n▪ Si non utilisé : Maintient le nettoyage périodique automatique actif.",
                    ),
                    l.tr("✔ Safe", "✔ Sans danger"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_game_bar,
                    l.tr(
                        "Re-enable Xbox Game Bar & GameDVR",
                        "Réactiver Xbox Game Bar & GameDVR",
                    ),
                    l.tr(
                        "Re-enable Xbox Game Bar & GameDVR",
                        "Réactivation de Xbox Game Bar & GameDVR",
                    ),
                    l.tr(
                        "▪ Default: Xbox GameDVR background recording and Game Bar popups are turned off.\n▪ If enabled: Restores default Windows Xbox Game Bar overlay and GameDVR capture features.\n▪ If skipped: Leaves GameDVR and Xbox Game Bar background capture disabled.",
                        "▪ Par défaut : L'enregistrement GameDVR et l'overlay Xbox Game Bar sont désactivés.\n▪ Si utilisé : Réactive l'overlay Xbox Game Bar et la capture automatique en arrière-plan.\n▪ Si non utilisé : Maintient GameDVR et la Game Bar désactivés pour préserver les performances en jeu.",
                    ),
                    l.tr("✔ Original Windows default", "✔ Valeur d'origine Windows"),
                    false,
                );
            });

            cols[1].group(|ui| {
                ui.label(RichText::new(l.tr("• Network Settings (Boost-Internet)", "• Paramètres Réseau (Boost-Internet)")).strong());
                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_dns_dhcp,
                    l.tr(
                        "Restore automatic DNS servers (DHCP)",
                        "Restaurer les serveurs DNS automatiques (DHCP)",
                    ),
                    l.tr(
                        "Restore Automatic ISP / Router DNS (DHCP)",
                        "Restauration des DNS automatiques de votre Box / FAI",
                    ),
                    l.tr(
                        "▪ Default: Network adapter uses static public DNS resolvers (Cloudflare, Google, Quad9).\n▪ If enabled: Resets DNS configuration to automatic DHCP assignment from your router / ISP.\n▪ If skipped: Preserves current static fast DNS server addresses on your adapter.",
                        "▪ Par défaut : La carte réseau utilise des serveurs DNS publics statiques (Cloudflare, Google...).\n▪ Si utilisé : Rétablit l'attribution automatique des serveurs DNS par DHCP (votre Box / FAI).\n▪ Si non utilisé : Conserve les serveurs DNS rapides actuels configurés sur votre carte.",
                    ),
                    l.tr(
                        "⚠ Uncheck if you use NextDNS configured via static IP address",
                        "⚠ Décochez si vous avez configuré NextDNS manuellement par adresse IP statique",
                    ),
                    true,
                );
                ui.label(
                    RichText::new(l.tr(
                        "  ℹ Uncheck if you use NextDNS configured via static IP.",
                        "  ℹ Décochez si vous utilisez NextDNS configuré par IP statique.",
                    ))
                    .weak(),
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_mtu_1500,
                    l.tr(
                        "Restore default MTU (1500)",
                        "Restaurer le MTU par défaut (1500)",
                    ),
                    l.tr(
                        "Restore MTU = 1500",
                        "Restauration du MTU = 1500",
                    ),
                    l.tr(
                        "▪ Default: MTU size is tuned to a custom unfragmented value (e.g. 1472, 1492).\n▪ If enabled: Resets standard Maximum Transmission Unit MTU = 1500 on your active network adapter.\n▪ If skipped: Retains the tuned custom MTU packet size.",
                        "▪ Par défaut : Le MTU est calibré sur une valeur personnalisée sans fragmentation (ex: 1472, 1492).\n▪ Si utilisé : Rétablit la taille de paquet standard MTU = 1500 sur votre carte réseau active.\n▪ Si non utilisé : Conserve la valeur optimisée actuelle du MTU.",
                    ),
                    l.tr("✔ Original Windows default", "✔ Valeur d'origine Windows"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_wu_p2p,
                    l.tr(
                        "Restore default Delivery Optimization behavior (Windows Update)",
                        "Restaurer le comportement par défaut de Delivery Optimization (Windows Update)",
                    ),
                    l.tr(
                        "Restore Delivery Optimization",
                        "Restauration de Delivery Optimization",
                    ),
                    l.tr(
                        "▪ Default: Delivery Optimization P2P sharing is locked to 0 to save upload bandwidth.\n▪ If enabled: Restores original Windows Update Delivery Optimization cloud sharing policy.\n▪ If skipped: Keeps Windows Update P2P upload sharing disabled.",
                        "▪ Par défaut : Le partage P2P de Delivery Optimization est bridé à 0 pour protéger l'Upload.\n▪ Si utilisé : Restaure le comportement par défaut de partage de mises à jour de Windows Update.\n▪ Si non utilisé : Maintient le blocage du partage P2P de mises à jour vers l'extérieur.",
                    ),
                    l.tr("✔ Original Windows default", "✔ Valeur d'origine Windows"),
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_wifi_power,
                    l.tr(
                        "Restore default Wi-Fi power saving",
                        "Restaurer l'économie d'énergie Wi-Fi par défaut",
                    ),
                    l.tr(
                        "Restore Wi-Fi Power Saving",
                        "Restauration de l'économie d'énergie Wi-Fi",
                    ),
                    l.tr(
                        "▪ Default: Wireless network adapter is configured for Maximum Performance (power saving 0).\n▪ If enabled: Restores default Wi-Fi power saving mode index (3) to optimize battery life.\n▪ If skipped: Keeps Wi-Fi adapter in continuous Maximum Performance mode for low latency.",
                        "▪ Par défaut : La carte Wi-Fi est forcée en Performances maximales (indice d'économie 0).\n▪ Si utilisé : Rétablit l'indice d'économie d'énergie Wi-Fi standard (3) pour préserver la batterie.\n▪ Si non utilisé : Maintient la carte Wi-Fi en performance maximale sans micro-sommeil.",
                    ),
                    l.tr("✔ Original Windows default", "✔ Valeur d'origine Windows"),
                    false,
                );
            });
        });

        ui.add_space(10.0);
        if ui
            .add_enabled(
                !self.is_busy,
                egui::Button::new(RichText::new(l.tr("↻ Restore Checked Settings", "↻ Restaurer les paramètres cochés")).strong()),
            )
            .on_hover_text(l.tr(
                "Immediately applies checked restorations above.",
                "Applique immédiatement les restaurations sélectionnées ci-dessus.",
            ))
            .clicked()
        {
            self.spawn_restore();
        }
    }

    fn render_log_panel(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.horizontal(|ui| {
            ui.label(RichText::new(l.tr(
                "• Execution Console & Real-Time Log",
                "• Console d'exécution & Journal Temps Réel",
            )).strong());
            ui.label(RichText::new(format!("({})", self.log_file_path.display())).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(l.tr("📂 Open Folder", "📂 Dossier logs"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Open Logs Directory", "Ouvrir le dossier des journaux"),
                            l.tr(
                                "Opens the dedicated logs folder in Windows File Explorer.\n▪ Default: Session logs and HTML reports are stored inside the dedicated logs/ folder.\n▪ If enabled: Opens File Explorer directly in the logs/ directory.\n▪ If skipped: Does nothing.",
                                "Ouvre le dossier dédié aux fichiers journaux dans l'Explorateur Windows.\n▪ Par défaut : Tous les logs et rapports HTML sont rangés proprement dans le sous-dossier logs/.\n▪ Si utilisé : Ouvre l'Explorateur directement dans le dossier logs/.\n▪ Si non utilisé : N'ouvre aucune fenêtre.",
                            ),
                            l.tr("✔ Safe read-only action", "✔ Action de consultation sans danger"),
                            false,
                        );
                    })
                    .clicked()
                {
                    let dir = self
                        .log_file_path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(crate::logger::resolve_logs_dir);
                    let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
                }
                if ui
                    .small_button(l.tr("Clear", "Effacer"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Clear Console", "Effacer la console"),
                            l.tr(
                                "Clears all displayed log messages from the UI view.\n▪ Default: Log history is retained in memory.\n▪ If enabled: Clears current console display (the file on disk is preserved).\n▪ If skipped: Keeps existing log history displayed.",
                                "Efface tous les messages du journal affichés dans la fenêtre.\n▪ Par défaut : L'historique des logs reste visible.\n▪ Si utilisé : Vide l'affichage de la console (le fichier journal sur disque reste intact).\n▪ Si non utilisé : Conserve l'affichage actuel du journal.",
                            ),
                            l.tr(
                                "✔ Console display only, does not delete log file",
                                "✔ Affichage console uniquement, le fichier journal n'est pas supprimé",
                            ),
                            false,
                        );
                    })
                    .clicked()
                {
                    self.logs.clear();
                }
                ui.checkbox(&mut self.auto_scroll_logs, l.tr("Auto-scroll", "Défilement auto"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            l.tr("Auto-scroll Console", "Défilement automatique"),
                            l.tr(
                                "Automatically scrolls down to latest received log messages.\n▪ Default: Enabled for live command tracking.\n▪ If enabled: Console automatically follows newest log output.\n▪ If skipped: Scroll position remains fixed to let you read earlier lines.",
                                "Fait défiler automatiquement la vue vers les derniers messages reçus.\n▪ Par défaut : Activé pour suivre l'exécution en direct.\n▪ Si utilisé : La console suit automatiquement les nouvelles lignes émises.\n▪ Si non utilisé : La position de lecture reste fixe pour vous permettre de lire les messages précédents.",
                            ),
                            l.tr(
                                "✔ Real-time console view setting",
                                "✔ Réglage d'affichage de la console",
                            ),
                            false,
                        );
                    });
            });
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("logs_scroll_area")
            .auto_shrink([false, false])
            .stick_to_bottom(self.auto_scroll_logs)
            .show(ui, |ui| {
                for entry in &self.logs {
                    let color = match entry.level {
                        LogLevel::Info => Color32::from_rgb(200, 215, 230),
                        LogLevel::Success => Color32::from_rgb(90, 230, 130),
                        LogLevel::Warning => Color32::from_rgb(255, 200, 75),
                        LogLevel::Error => Color32::from_rgb(255, 95, 95),
                        LogLevel::Section => Color32::from_rgb(90, 200, 255),
                    };
                    let line = format!("[{}] {}", entry.timestamp, entry.message);
                    ui.label(RichText::new(line).monospace().color(color));
                }
            });
    }
}

fn render_diff_table(ui: &mut egui::Ui, id: &str, diffs: &[MetricDiff], lang: Lang) {
    egui::Grid::new(id)
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            ui.strong(lang.tr("Metric", "Métrique"));
            ui.strong(lang.tr("Before", "Avant"));
            ui.strong(lang.tr("After", "Après"));
            ui.strong(lang.tr("Delta", "Delta"));
            ui.end_row();

            for d in diffs {
                ui.label(&d.metric);
                ui.label(&d.before);
                ui.label(&d.after);
                let color = match d.improved {
                    Some(true) => Color32::from_rgb(80, 220, 120),
                    Some(false) => Color32::from_rgb(255, 180, 70),
                    None => Color32::LIGHT_GRAY,
                };
                ui.label(RichText::new(&d.delta).color(color).strong());
                ui.end_row();
            }
        });
}

impl eframe::App for WinCareApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();

        if self.is_busy {
            ctx.request_repaint_after(Duration::from_millis(80));
        }

        egui::TopBottomPanel::top("top_header_panel").show(ctx, |ui| {
            self.render_top_bar(ui);
        });

        egui::TopBottomPanel::bottom("bottom_log_panel")
            .resizable(true)
            .default_height(190.0)
            .min_height(110.0)
            .show(ctx, |ui| {
                self.render_log_panel(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("main_tab_scroll", self.active_tab))
                .show(ui, |ui| match self.active_tab {
                    ActiveTab::Overview => self.render_overview_tab(ui),
                    ActiveTab::Repair => self.render_repair_tab(ui),
                    ActiveTab::BoostPc => self.render_boost_pc_tab(ui),
                    ActiveTab::BoostNet => self.render_boost_net_tab(ui),
                    ActiveTab::Debloat => self.render_debloat_tab(ui),
                    ActiveTab::Apps => self.render_apps_tab(ui),
                    ActiveTab::GamingHardware => self.render_gaming_hardware_tab(ui),
                    ActiveTab::Restore => self.render_restore_tab(ui),
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_tab_labels_bilingual() {
        assert_eq!(ActiveTab::Overview.label(Lang::En), "🏠 Overview");
        assert_eq!(ActiveTab::Overview.label(Lang::Fr), "🏠 Vue d'ensemble");

        assert_eq!(ActiveTab::Repair.label(Lang::En), "🛠 Windows Repair");
        assert_eq!(ActiveTab::Repair.label(Lang::Fr), "🛠 Réparation Windows");

        assert_eq!(ActiveTab::BoostPc.label(Lang::En), "⚡ Boost PC");
        assert_eq!(ActiveTab::BoostPc.label(Lang::Fr), "⚡ Boost PC");

        assert_eq!(ActiveTab::BoostNet.label(Lang::En), "🌐 Boost Internet");
        assert_eq!(ActiveTab::BoostNet.label(Lang::Fr), "🌐 Boost Internet");

        assert_eq!(ActiveTab::Debloat.label(Lang::En), "🧹 Debloat & Tweaks");
        assert_eq!(ActiveTab::Debloat.label(Lang::Fr), "🧹 Debloat & Tweaks");

        assert_eq!(ActiveTab::Apps.label(Lang::En), "📦 Apps & Bloatware");
        assert_eq!(ActiveTab::Apps.label(Lang::Fr), "📦 Logiciels & Bloatwares");

        assert_eq!(ActiveTab::GamingHardware.label(Lang::En), "🎮 Gaming & Hardware");
        assert_eq!(ActiveTab::GamingHardware.label(Lang::Fr), "🎮 Gaming & Matériel");

        assert_eq!(ActiveTab::Restore.label(Lang::En), "↩ Restore (.RESTORE)");
        assert_eq!(ActiveTab::Restore.label(Lang::Fr), "↩ Restauration (.RESTORE)");
    }

    #[test]
    fn test_parse_tooltip_line() {
        let def_en = parse_tooltip_line("▪ Default: sample text");
        assert!(def_en.is_some());
        let (p, c, r) = def_en.unwrap();
        assert_eq!(p, "▪ Default:");
        assert_eq!(c, Color32::from_rgb(150, 200, 255));
        assert_eq!(r, " sample text");

        let enab_fr = parse_tooltip_line("▪ Si utilisé : sample text");
        assert!(enab_fr.is_some());
        let (p2, c2, _) = enab_fr.unwrap();
        assert_eq!(p2, "▪ Si utilisé :");
        assert_eq!(c2, Color32::from_rgb(100, 235, 160));

        let skip_en = parse_tooltip_line("▪ If skipped: sample text");
        assert!(skip_en.is_some());
        let (p3, c3, _) = skip_en.unwrap();
        assert_eq!(p3, "▪ If skipped:");
        assert_eq!(c3, Color32::from_rgb(235, 190, 110));

        assert!(parse_tooltip_line("Regular line").is_none());
    }

    #[test]
    fn test_active_tab_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ActiveTab::Overview);
        set.insert(ActiveTab::Repair);
        assert!(set.contains(&ActiveTab::Overview));
        assert!(!set.contains(&ActiveTab::GamingHardware));
    }

    #[test]
    fn test_task_finished_guard_drop() {
        let (tx, rx) = mpsc::channel();
        let logger = Logger::new(Some(tx), false);
        let runner = Runner::new(logger, false);

        {
            let _guard = TaskFinishedGuard {
                runner,
                title: "Test Task".to_string(),
                need_reboot: true,
            };
        }

        let event = rx.try_recv().expect("Event should have been sent on guard drop");
        match event {
            WorkerEvent::TaskFinished { title, need_reboot } => {
                assert_eq!(title, "Test Task");
                assert!(need_reboot);
            }
            _ => panic!("Expected TaskFinished event"),
        }
    }
}
