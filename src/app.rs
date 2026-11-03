use crate::i18n::{debloat_category_title, debloat_tweak_text, repair_task_text, Lang};
use crate::logger::Logger;
use crate::runner::Runner;
use crate::types::{
    DebloatCategory, DebloatTweakStatus, LogEntry, LogLevel, MetricDiff, NetDiagnosis,
    NetOptimOptions, PcCleanOptions, PcDiagnosis, PcOptimOptions, RepairTask, RestoreOptions,
    StartupItem, TempSubfolderInfo, WorkerEvent,
};
use eframe::egui::{self, Color32, RichText};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Overview,
    Repair,
    BoostPc,
    BoostNet,
    Debloat,
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
    tx: Sender<WorkerEvent>,
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

    // Auto-update GitHub
    update_info: Option<crate::updater::UpdateInfo>,

    // Onglet Restore
    restore_opts: RestoreOptions,
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

        Self {
            is_admin,
            dry_run,
            lang,
            active_tab: ActiveTab::Overview,
            tx,
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
            update_info: None,
            restore_opts: RestoreOptions::default(),
        }
    }

    fn make_runner(&self) -> Runner {
        let logger = Logger::new(Some(self.tx.clone()), false);
        Runner::new(logger, self.dry_run)
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
        self.progress_label = "Vérification des mises à jour sur GitHub...".to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
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
            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Vérification MAJ GitHub".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_create_restore_point(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        self.progress_label = "Création d'un Point de Restauration Windows...".to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let desc = format!(
                "WinCare Backup ({})",
                chrono::Local::now().format("%Y-%m-%d %H:%M")
            );
            crate::boost_pc::create_system_restore_point(&runner, &desc);
            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Point de Restauration".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_temp_scan(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        self.progress_label = "Analyse des sous-dossiers les plus lourds dans %TEMP%...".to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let items = crate::boost_pc::scan_top_temp_folders(&runner);
            runner.logger.send_event(WorkerEvent::TempAnalysisReady(items));
            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Analyse %TEMP%".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_delete_temp_paths(&mut self, paths: Vec<String>) {
        if self.is_busy || paths.is_empty() {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = paths.len();
        self.progress_label = "Suppression des dossiers temporaires sélectionnés...".to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let total = paths.len();
            for (idx, p) in paths.iter().enumerate() {
                runner
                    .logger
                    .progress(idx + 1, total, format!("Suppression de {p}..."));
                crate::boost_pc::delete_temp_target(&runner, p);
            }
            let updated = crate::boost_pc::scan_top_temp_folders(&runner);
            runner
                .logger
                .send_event(WorkerEvent::TempAnalysisReady(updated));
            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Purge ciblée %TEMP%".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_full_diagnose(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = 3;
        self.progress_label = "Diagnostic complet en cours...".to_string();

        let runner = self.make_runner();
        let speedtest = self.include_speedtest;
        std::thread::spawn(move || {
            runner
                .logger
                .progress(1, 3, "Analyse du PC (Hardware, RAM, Disques, Caches, Startup)...");
            let pc = crate::boost_pc::run_pc_diagnosis(&runner);
            runner.logger.send_event(WorkerEvent::PcDiagnosisReady(pc));

            runner
                .logger
                .progress(2, 3, "Identification des plus gros dossiers dans %TEMP%...");
            let top_temp = crate::boost_pc::scan_top_temp_folders(&runner);
            runner
                .logger
                .send_event(WorkerEvent::TempAnalysisReady(top_temp));

            runner
                .logger
                .progress(3, 3, "Analyse de la connexion Internet (Route, Ping, DNS, MTU)...");
            let net = crate::boost_net::run_net_diagnosis(&runner, speedtest);
            runner.logger.send_event(WorkerEvent::NetDiagnosisReady(net));

            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Diagnostic complet".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_repair(&mut self, tasks: Vec<RepairTask>, schedule_chkdsk: bool) {
        if self.is_busy || tasks.is_empty() {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = tasks.len();
        self.progress_label = "Réparation Windows en cours...".to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            let need_reboot =
                crate::repair::execute_repair_tasks(&runner, &tasks, schedule_chkdsk);
            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Réparation Windows".to_string(),
                need_reboot,
            });
        });
    }

    fn spawn_pc_diagnose(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 2;
        self.progress_label = "Analyse du PC en cours...".to_string();

        let runner = self.make_runner();
        std::thread::spawn(move || {
            runner.logger.progress(1, 2, "Diagnostic matériel & système...");
            let diag = crate::boost_pc::run_pc_diagnosis(&runner);
            runner.logger.send_event(WorkerEvent::PcDiagnosisReady(diag));

            runner
                .logger
                .progress(2, 2, "Scan des plus gros sous-dossiers %TEMP%...");
            let top_temp = crate::boost_pc::scan_top_temp_folders(&runner);
            runner
                .logger
                .send_event(WorkerEvent::TempAnalysisReady(top_temp));

            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Diagnostic PC".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_pc_boost(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = 4;
        self.progress_label = "Boost PC en cours...".to_string();

        let runner = self.make_runner();
        let clean_opts = self.pc_clean_opts.clone();
        let optim_opts = self.pc_optim_opts.clone();
        let existing_diag = self.pc_diag.clone();

        std::thread::spawn(move || {
            runner.logger.progress(1, 4, "Mesure de l'état initial du PC...");
            let before =
                existing_diag.unwrap_or_else(|| crate::boost_pc::run_pc_diagnosis(&runner));
            runner
                .logger
                .send_event(WorkerEvent::PcDiagnosisReady(before.clone()));

            runner
                .logger
                .progress(2, 4, "Nettoyage des caches et fichiers temporaires...");
            crate::boost_pc::run_pc_cleaning(&runner, &clean_opts);

            runner
                .logger
                .progress(3, 4, "Application des optimisations système...");
            crate::boost_pc::run_pc_optimizations(&runner, &optim_opts, &before);

            runner
                .logger
                .progress(4, 4, "Calcul du comparatif Avant / Après...");
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

            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Boost PC".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_net_diagnose(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        self.progress_label = "Diagnostic réseau & DNS en cours...".to_string();

        let runner = self.make_runner();
        let speedtest = self.include_speedtest;
        std::thread::spawn(move || {
            let diag = crate::boost_net::run_net_diagnosis(&runner, speedtest);
            runner.logger.send_event(WorkerEvent::NetDiagnosisReady(diag));
            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Diagnostic Internet".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_net_boost(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 0;
        self.progress_total = 3;
        self.progress_label = "Optimisation Internet en cours...".to_string();

        let runner = self.make_runner();
        let optim_opts = self.net_optim_opts.clone();
        let existing_diag = self.net_diag.clone();
        let speedtest = self.include_speedtest;

        std::thread::spawn(move || {
            runner.logger.progress(1, 3, "Diagnostic réseau initial...");
            let before = existing_diag
                .unwrap_or_else(|| crate::boost_net::run_net_diagnosis(&runner, speedtest));
            runner
                .logger
                .send_event(WorkerEvent::NetDiagnosisReady(before.clone()));

            runner
                .logger
                .progress(2, 3, "Application des optimisations réseau...");
            let need_reboot =
                crate::boost_net::run_net_optimizations(&runner, &optim_opts, &before);

            runner
                .logger
                .progress(3, 3, "Comparaison Avant / Après...");
            let diffs = crate::boost_net::compute_net_comparison(
                &runner,
                &before,
                optim_opts.run_speedtest_after,
            );
            runner
                .logger
                .send_event(WorkerEvent::NetComparisonReady(diffs));

            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Boost Internet".to_string(),
                need_reboot,
            });
        });
    }

    fn spawn_debloat_batch(&mut self, recommended_only: bool) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        self.progress_label = if recommended_only {
            "Application du Pack Debloat Windows 11 Recommandé...".to_string()
        } else {
            "Restauration des paramètres Windows 11 par défaut...".to_string()
        };

        let runner = self.make_runner();
        std::thread::spawn(move || {
            if recommended_only {
                crate::debloat::apply_recommended_debloat(&runner);
            } else {
                crate::debloat::restore_all_debloat(&runner);
            }
            let updated = crate::debloat::list_debloat_tweaks();
            runner
                .logger
                .send_event(WorkerEvent::DebloatListUpdated(updated));
            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Debloat Windows 11".to_string(),
                need_reboot: false,
            });
        });
    }

    fn spawn_restore(&mut self) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.progress_current = 1;
        self.progress_total = 1;
        self.progress_label = "Restauration des paramètres par défaut...".to_string();

        let runner = self.make_runner();
        let opts = self.restore_opts.clone();
        let if_index = self.net_diag.as_ref().map(|d| d.if_index);
        let adapter_name = self.net_diag.as_ref().map(|d| d.adapter_name.clone());

        std::thread::spawn(move || {
            crate::restore::execute_restore(&runner, &opts, if_index, adapter_name.as_deref());
            runner.logger.send_event(WorkerEvent::TaskFinished {
                title: "Restauration système".to_string(),
                need_reboot: false,
            });
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

        ui.horizontal(|ui| {
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
            ui.add(
                egui::ProgressBar::new(frac)
                    .text(&self.progress_label)
                    .animate(self.is_busy),
            );
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
                if ui.small_button(l.tr("Dismiss", "Masquer")).clicked() {
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
    }

    fn render_repair_tab(&mut self, ui: &mut egui::Ui) {
        let l = self.lang;
        ui.horizontal(|ui| {
            ui.heading(l.tr(
                "Windows Repair (Interactive & Auto — Real-Time Streaming)",
                "Réparation Windows (Interactif & Auto — Streaming Temps Réel)",
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(l.tr("Uncheck all", "Tout décocher")).clicked() {
                    self.selected_repairs.clear();
                }
                if ui.button(l.tr("Check all", "Tout cocher")).clicked() {
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
                            "Schedules a deep integrity check of drive C: filesystem and bad sectors upon next Windows startup.",
                            "Programme une vérification complète du système de fichiers et des secteurs physiques du disque C: lors du prochain démarrage de Windows.",
                        ),
                        l.tr(
                            "⚠ Next PC boot will take 15 to 45 minutes while scanning the drive",
                            "⚠ Le prochain démarrage du PC prendra 15 à 45 minutes pendant l'analyse du disque",
                        ),
                        true,
                    );
                }
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
                }
            });
            ui.add_space(4.0);
        }
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
                if ui.small_button(l.tr("Clear", "Effacer")).clicked() {
                    self.logs.clear();
                }
                ui.checkbox(&mut self.auto_scroll_logs, l.tr("Auto-scroll", "Défilement auto"));
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
                .id_salt("main_tab_scroll")
                .show(ui, |ui| match self.active_tab {
                    ActiveTab::Overview => self.render_overview_tab(ui),
                    ActiveTab::Repair => self.render_repair_tab(ui),
                    ActiveTab::BoostPc => self.render_boost_pc_tab(ui),
                    ActiveTab::BoostNet => self.render_boost_net_tab(ui),
                    ActiveTab::Debloat => self.render_debloat_tab(ui),
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
}
