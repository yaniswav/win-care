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

/// Affiche une infobulle riche et structurée (Titre + Explication + Niveau de sécurité).
fn show_rich_tooltip(ui: &mut egui::Ui, title: &str, details: &str, safety: &str, is_warning: bool) {
    ui.set_max_width(420.0);
    ui.label(
        RichText::new(title)
            .strong()
            .color(Color32::from_rgb(100, 210, 255)),
    );
    ui.separator();
    ui.label(details);
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

        let (tx, rx) = mpsc::channel();
        let init_logger = Logger::new(Some(tx.clone()), false);
        let log_file_path = init_logger.log_path.clone();

        init_logger.info(
            "WinCare v0.2.1 prêt. Survolez n'importe quelle option ou badge [?] pour afficher son infobulle détaillée.",
        );
        if !is_admin {
            init_logger.warn(
                "Exécuté sans droits Administrateur : certaines réparations nécessitent une élévation UAC.",
            );
        }
        if dry_run {
            init_logger.info(
                "Mode Simulation (--dry-run) actif : aucune modification système ne sera appliquée.",
            );
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
                    self.progress_label = format!("Terminé : {title}");
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
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("⚡ WinCare v0.2.1")
                    .strong()
                    .color(Color32::from_rgb(90, 200, 250)),
            );
            ui.label(RichText::new("— Repair, Boost & Win11 Debloat").weak());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut self.dry_run, "Mode Simulation (Dry-Run)")
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Mode Simulation (--dry-run)",
                            "Quand cette case est cochée, WinCare n'effectue aucune modification sur votre PC : il affiche uniquement dans le journal en bas les commandes qui auraient été lancées.",
                            "✔ Idéal pour tester l'outil sans rien toucher au système",
                            false,
                        );
                    });

                ui.separator();

                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("★ Créer Point de Restauration"),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Point de Restauration Système Windows",
                            "Appelle Checkpoint-Computer pour créer un point de restauration officiel Windows avant vos modifications.",
                            "✔ Recommandé avant un gros nettoyage ou debloat",
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
                    if ui
                        .add_enabled(
                            !self.is_busy,
                            egui::Button::new(
                                RichText::new(format!("⬆ Installer MAJ v{ver}"))
                                    .color(Color32::from_rgb(90, 235, 135))
                                    .strong(),
                            ),
                        )
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                "Installer la mise à jour GitHub",
                                "Télécharge automatiquement la nouvelle version de WinCare.exe depuis GitHub Releases, vérifie sa signature exécutable et remplace le fichier actuel.",
                                "✔ Mise à jour officielle depuis yaniswav/win-care",
                                false,
                            );
                        })
                        .clicked()
                    {
                        self.spawn_check_update(true);
                    }
                } else if ui
                    .add_enabled(!self.is_busy, egui::Button::new("↻ Vérifier MAJ"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Vérifier les mises à jour (GitHub Releases)",
                            "Interroge https://github.com/yaniswav/win-care/releases/latest pour savoir si une version plus récente de WinCare.exe est disponible.",
                            "✔ Aucune donnée personnelle envoyée",
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
                        RichText::new("✔ Administrateur")
                            .color(Color32::from_rgb(80, 220, 120))
                            .strong(),
                    )
                    .on_hover_text("WinCare tourne avec les privilèges Administrateur complets.");
                } else if ui
                    .button(
                        RichText::new("⚠ Mode Standard — Relancer en Admin")
                            .color(Color32::from_rgb(255, 200, 70)),
                    )
                    .on_hover_text(
                        "Cliquez ici pour relancer WinCare avec l'invite Administrateur Windows (UAC), requise pour DISM, SFC et le nettoyage système.",
                    )
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
                "▶ Aperçu & Mode Auto",
            )
            .on_hover_text("Vue d'ensemble et lancement en 1 clic des diagnostics ou réparations complètes.");
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::Repair,
                "⚙ Réparation Windows",
            )
            .on_hover_text("Les 8 outils de réparation système Windows (DISM, SFC, Windows Update, Réseau, Store, Services, CHKDSK).");
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::BoostPc,
                "⚡ Boost PC & %TEMP%",
            )
            .on_hover_text("Nettoyage des caches, analyseur des plus gros dossiers %TEMP%, optimisations de performance et gestion des programmes au démarrage.");
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::BoostNet,
                "🌐 Boost Internet & DNS",
            )
            .on_hover_text("Benchmark de vitesse des serveurs DNS (avec protection NextDNS), calcul du MTU optimal et optimisation Wi-Fi / TCP.");
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::Debloat,
                "★ Debloat & Tweaks Win11",
            )
            .on_hover_text("Désactivation de Copilot, Windows Recall, Bing, Télémétrie, Widgets et activation du clic-droit classique Windows 10.");
            ui.selectable_value(&mut self.active_tab, ActiveTab::Restore, "↻ Restauration")
                .on_hover_text("Permet de remettre à l'état d'origine tous les réglages modifiés par Boost PC ou Boost Internet.");
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
                    RichText::new(
                        "⚠ Un redémarrage de Windows est recommandé pour finaliser les réparations.",
                    )
                    .color(Color32::from_rgb(255, 190, 60))
                    .strong(),
                );
                if ui.small_button("Masquer").clicked() {
                    self.need_reboot_banner = false;
                }
            });
        }
    }

    fn render_overview_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Tableau de bord & Actions rapides");
        ui.label("💡 Astuce : Survolez n'importe quelle option ou icône [?] pour afficher instantanément son explication détaillée.");
        ui.add_space(8.0);

        ui.columns(4, |cols| {
            cols[0].group(|ui| {
                ui.heading(
                    RichText::new("▶ Diagnostic Complet")
                        .color(Color32::from_rgb(100, 210, 255)),
                );
                ui.label("Analyse PC + Top dossiers %TEMP% + Connexion Internet (Route, Ping, DNS, MTU) en lecture seule.");
                ui.add_space(4.0);
                checkbox_with_help(
                    ui,
                    &mut self.include_speedtest,
                    "Inclure Speedtest Ookla",
                    "Test de débit Ookla Speedtest",
                    "Si coché, exécute speedtest.exe pour mesurer votre vitesse réelle de téléchargement (Download) et d'envoi (Upload).",
                    "ℹ Ajoute environ 25 secondes au diagnostic",
                    false,
                );
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("▶ Lancer le Diagnostic Complet"),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Diagnostic Complet (100 % Lecture Seule)",
                            "Analyse votre matériel, la RAM, le CPU, l'espace disque, les 9 dossiers de caches, les sous-dossiers les plus lourds de %TEMP%, les programmes au démarrage, le ping, les serveurs DNS et le MTU.",
                            "✔ 100 % sans danger : ne modifie aucun fichier ni réglage",
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
                ui.label("Exécute automatiquement les 8 réparations système (DISM, SFC, Windows Update, Réseau, Temp, Store, Services, CHKDSK).");
                ui.add_space(4.0);
                ui.label(RichText::new("Streaming en direct de la progression %.").weak());
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("▶ Réparation Auto Complète"),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Réparation Automatique Complète (Repair-Windows-Auto)",
                            "Enchaîne automatiquement : 1. DISM (Check/Scan/RestoreHealth + WinSxS), 2. SFC (/scannow), 3. Réparation Windows Update, 4. Reset Réseau, 5. Nettoyage Temp, 6. Réparation Windows Store, 7. Services critiques, 8. Planification CHKDSK au prochain redémarrage.",
                            "⚠ Dure 20 à 45 minutes (vous pouvez verrouiller le PC avec Win+L)",
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
                    RichText::new("⚡ Pack Boost Safe").color(Color32::from_rgb(255, 200, 90)),
                );
                ui.label("Nettoyage sécurisé (%TEMP%, caches) et optimisations recommandées PC + Internet avec diff Avant/Après.");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.is_busy, egui::Button::new("⚡ Boost PC"))
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                "Boost PC Safe",
                                "Vide les fichiers temporaires et caches sélectionnés, applique les optimisations de réactivité Windows et affiche le comparatif Avant / Après.",
                                "✔ Toutes les optimisations sont réversibles dans l'onglet Restauration",
                                false,
                            );
                        })
                        .clicked()
                    {
                        self.active_tab = ActiveTab::BoostPc;
                        self.spawn_pc_boost();
                    }
                    if ui
                        .add_enabled(!self.is_busy, egui::Button::new("🌐 Boost Net"))
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                "Boost Internet Safe",
                                "Teste les résolveurs DNS, applique le plus rapide (sauf si NextDNS est actif), règle le MTU optimal sans fragmentation, vide le cache DNS et optimise la carte Wi-Fi.",
                                "✔ Préserve automatiquement NextDNS s'il est détecté",
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
                    RichText::new("★ Win11 Debloat").color(Color32::from_rgb(210, 140, 255)),
                );
                let active_count = self.debloat_tweaks.iter().filter(|t| t.is_applied).count();
                ui.label(format!(
                    "Désactive Copilot, Recall, Télémétrie, Bing, Widgets et active le clic-droit classique ({active_count}/{} actifs).",
                    self.debloat_tweaks.len()
                ));
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("★ Appliquer Pack Debloat Recommandé"),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Pack Debloat Windows 11 Recommandé",
                            "Active les 13 tweaks recommandés : désactive Copilot, Recall, Click to Do, IA Edge/Paint/Notepad, Télémétrie, Bing dans Démarrer, Pubs Win11, Widgets, Touches rémanentes 5x Maj, et active le menu clic-droit classique Win10 + extensions de fichiers + Fin de tâche.",
                            "✔ Annulable à tout moment en 1 clic",
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
        ui.heading("Résumé de l'état détecté");

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(RichText::new("• Système, Matériel & Stockage Temporaire").strong());
                if let Some(d) = &self.pc_diag {
                    ui.label(format!("Modèle : {}", d.machine_model));
                    ui.label(format!("OS : {} (Uptime: {:.1} j)", d.os_caption, d.uptime_days));
                    ui.label(format!(
                        "CPU : {} ({} cœurs / {} threads) — Charge : {:.1}%",
                        d.cpu_name, d.cpu_cores, d.cpu_threads, d.cpu_usage_pct
                    ));
                    ui.label(format!(
                        "RAM : {} Mo libres / {} Mo ({:.1}% utilisée)",
                        d.ram_free_mb, d.ram_total_mb, d.ram_used_pct
                    ));
                    ui.label(format!(
                        "Disque système libre : {:.2} Go | SSD détecté : {}",
                        d.sys_free_gb,
                        if d.ssd_detected { "Oui" } else { "Non" }
                    ));
                    ui.label(
                        RichText::new(format!(
                            "Caches nettoyables détectés : {} Mo ({:.2} Go)",
                            d.cache_total_mb,
                            d.cache_total_mb as f64 / 1024.0
                        ))
                        .color(Color32::from_rgb(255, 200, 80)),
                    );
                } else {
                    ui.label(
                        RichText::new("Aucun diagnostic PC exécuté dans cette session.").weak(),
                    );
                }
            });

            cols[1].group(|ui| {
                ui.label(RichText::new("• Réseau & Connectivité").strong());
                if let Some(n) = &self.net_diag {
                    ui.label(format!(
                        "Carte active : {} ({})",
                        n.adapter_name, n.link_speed
                    ));
                    ui.label(format!(
                        "Passerelle : {} | MTU actuel : {} (optimal : {:?})",
                        n.gateway, n.current_mtu, n.optimal_mtu
                    ));
                    ui.label(format!("DNS actuels : {}", n.current_dns.join(", ")));
                    if n.uses_nextdns {
                        ui.label(
                            RichText::new(format!("✔ NextDNS actif ({})", n.nextdns_mode))
                                .color(Color32::from_rgb(220, 130, 255)),
                        );
                    }
                    if let Some(best) = &n.best_dns {
                        ui.label(format!(
                            "DNS le plus rapide : {} ({}) en {:?} ms",
                            best.resolver, best.ip, best.latency_ms
                        ));
                    }
                    ui.label(format!(
                        "Ping 1.1.1.1 : {:?} ms (pertes : {}/4)",
                        n.ping_net_ms, n.ping_loss_count
                    ));
                } else {
                    ui.label(
                        RichText::new("Aucun diagnostic Internet exécuté dans cette session.")
                            .weak(),
                    );
                }
            });
        });
    }

    fn render_repair_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Réparation Windows (Interactif & Auto — Streaming Temps Réel)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Tout décocher").clicked() {
                    self.selected_repairs.clear();
                }
                if ui.button("Tout cocher").clicked() {
                    for t in RepairTask::ALL {
                        self.selected_repairs.insert(t);
                    }
                }
            });
        });
        ui.label("Sélectionnez les outils de réparation système à exécuter dans l'ordre (survolez [?] pour les détails) :");
        ui.add_space(6.0);

        for task in RepairTask::ALL {
            let mut checked = self.selected_repairs.contains(&task);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if checkbox_with_help(
                        ui,
                        &mut checked,
                        &format!("[{}] {}", task.code(), task.label()),
                        task.label(),
                        task.description(),
                        "✔ Outil officiel de maintenance Windows",
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
                            .add_enabled(!self.is_busy, egui::Button::new("Exécuter seul"))
                            .on_hover_text("Lance uniquement cette réparation tout de suite.")
                            .clicked()
                        {
                            self.spawn_repair(vec![task], self.schedule_chkdsk_reboot);
                        }
                    });
                });
                ui.label(RichText::new(task.description()).weak());

                if task == RepairTask::Chkdsk {
                    checkbox_with_help(
                        ui,
                        &mut self.schedule_chkdsk_reboot,
                        "Planifier également un CHKDSK complet (chkdsk C: /F /R /X) au prochain redémarrage",
                        "Planification CHKDSK /F /R /X au redémarrage",
                        "Programme une vérification complète du système de fichiers et des secteurs physiques du disque C: lors du prochain démarrage de Windows.",
                        "⚠ Le prochain démarrage du PC prendra 15 à 45 minutes pendant l'analyse du disque",
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

            if ui
                .add_enabled(
                    !self.is_busy && !ordered.is_empty(),
                    egui::Button::new(
                        RichText::new(format!(
                            "▶ Lancer les {} réparations sélectionnées",
                            ordered.len()
                        ))
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
        ui.horizontal(|ui| {
            ui.heading("Boost PC — Analyse, Nettoyage %TEMP%, Optimisations & Startup");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(
                            RichText::new("▶ Appliquer Nettoyage & Optimisations").strong(),
                        ),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Appliquer Nettoyage & Optimisations",
                            "Exécute toutes les cases cochées dans les colonnes « 1. Options de Nettoyage » et « 2. Optimisations Système », puis calcule le gain de RAM et d'espace disque.",
                            "✔ Respecte scrupuleusement vos cases cochées",
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
                        egui::Button::new("▶ Analyser le PC & %TEMP%"),
                    )
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Diagnostic PC & Scan %TEMP% (Lecture seule)",
                            "Scanne l'état du processeur, de la mémoire RAM, des disques, des 9 dossiers de caches et identifie les 15 sous-dossiers les plus lourds dans %TEMP%.",
                            "✔ 100 % en lecture seule : ne supprime rien",
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
                    RichText::new("• 1. Options de Nettoyage (survolez [?] pour les détails)")
                        .strong(),
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.clean_user_temp,
                    "Vider %TEMP% utilisateur",
                    "Nettoyage du dossier %TEMP% utilisateur",
                    "Supprime les fichiers et dossiers temporaires de votre session (C:\\Users\\<Vous>\\AppData\\Local\\Temp). C'est ici que s'accumulent les résidus d'installateurs, de navigateurs et d'outils.",
                    "✔ 100 % sans danger : les fichiers utilisés par vos applications ouvertes sont automatiquement ignorés",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.clean_sys_temp,
                    "Vider C:\\Windows\\Temp système",
                    "Nettoyage du dossier C:\\Windows\\Temp",
                    "Supprime les fichiers temporaires créés par les services système Windows et les mises à jour de pilotes.",
                    "✔ 100 % sans danger",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.clean_wu_download,
                    "Vider le cache Windows Update (SoftwareDistribution\\Download)",
                    "Purge du cache de téléchargement Windows Update",
                    "Arrête momentanément le service Windows Update, supprime les anciens paquets de mises à jour déjà installés dans C:\\Windows\\SoftwareDistribution\\Download, puis redémarre le service.",
                    "✔ 100 % sans danger : libère souvent plusieurs Go et répare les mises à jour bloquées",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.clean_wer_dumps,
                    "Supprimer les rapports d'erreur (WER) et CrashDumps",
                    "Suppression des rapports de plantage (WER & CrashDumps)",
                    "Vide les dossiers Windows Error Reporting (ReportQueue / ReportArchive) ainsi que les fichiers mémoire .dmp dans %LOCALAPPDATA%\\CrashDumps créés lors du crash d'un jeu ou logiciel.",
                    "✔ 100 % sans danger",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.empty_recycle_bin,
                    "Vider la corbeille de tous les disques",
                    "Vidage de la Corbeille Windows",
                    "Vide définitivement la Corbeille sur tous vos disques internes et externes connectés.",
                    "⚠ Attention : décochez cette case si vous conservez des fichiers dans votre Corbeille !",
                    true,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.dism_component_cleanup,
                    "Nettoyer le WinSxS (DISM /StartComponentCleanup /ResetBase — lent)",
                    "Nettoyage approfondi du magasin de composants WinSxS",
                    "Exécute DISM /StartComponentCleanup /ResetBase pour supprimer les anciennes versions obsolètes des composants système Windows remplacées par les mises à jour récentes.",
                    "ℹ Décoché par défaut car l'opération dure 5 à 15 minutes (permet de gagner 2 à 8 Go)",
                    true,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_clean_opts.run_cleanmgr,
                    "Lancer le nettoyage de disque Windows (cleanmgr silencieux)",
                    "Nettoyage de disque officiel Microsoft (cleanmgr.exe)",
                    "Active les 20 règles de nettoyage sécurisées de l'utilitaire Windows Cleanmgr (miniatures, caches DirectX/Internet, anciens journaux de setup) et l'exécute automatiquement.",
                    "✔ 100 % sans danger",
                    false,
                );
            });

            cols[1].group(|ui| {
                ui.label(
                    RichText::new("⚡ 2. Optimisations Système (survolez [?] pour les détails)")
                        .strong(),
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.high_perf_power,
                    "Plan d'alimentation : Performances élevées (SCHEME_MIN)",
                    "Profil d'alimentation « Performances élevées »",
                    "Configure Windows sur le mode d'alimentation Performances élevées (powercfg /setactive SCHEME_MIN) pour éviter que le processeur ne bride sa fréquence au repos.",
                    "✔ Recommandé sur PC fixe ou PC portable branché (annulable dans Restauration)",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.ultimate_perf_power,
                    "Activer le plan 'Ultimate Performance' (créé si absent)",
                    "Profil caché « Performances optimales » (Ultimate Performance)",
                    "Débloque et active le plan d'alimentation réservé aux stations de travail (GUID e9a42b02...), qui désactive complètement le micro-parking des cœurs CPU.",
                    "ℹ Utile sur tour gaming/MAO (consomme un peu plus d'énergie au repos)",
                    true,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.reduce_visual_fx,
                    "Réduire les effets visuels (VisualFXSetting = 2 'Meilleures performances')",
                    "Effets visuels en mode « Meilleures performances »",
                    "Configure VisualFXSetting = 2 dans l'Explorateur pour désactiver les animations lentes et ombres portées des fenêtres.",
                    "✔ Rend l'interface plus réactive (annulable en 1 clic dans Restauration)",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.enable_storage_sense,
                    "Activer Storage Sense (nettoyage automatique périodique 30j)",
                    "Assistant Stockage automatique de Windows (Storage Sense)",
                    "Configure Windows pour purger automatiquement les fichiers temporaires inutilisés et les éléments de la corbeille datant de plus de 30 jours.",
                    "✔ 100 % sans danger et maintient le PC propre dans la durée",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.disable_game_bar,
                    "Désactiver l'overlay Xbox Game Bar & GameDVR",
                    "Désactivation de Xbox Game Bar & Capture GameDVR en arrière-plan",
                    "Désactive l'enregistrement vidéo continu GameDVR et l'overlay automatique Xbox Game Bar (sans désinstaller l'application Xbox).",
                    "✔ Évite les pertes de FPS et micro-saccades en jeu",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.disable_transparency,
                    "Désactiver les effets de transparence Windows",
                    "Désactivation du flou de transparence (Acrylique / Mica)",
                    "Met EnableTransparency = 0 dans les paramètres de personnalisation Windows pour alléger le rendu graphique du bureau.",
                    "✔ Réversible à tout moment dans Restauration",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.disable_sysmain,
                    "Désactiver SysMain / Superfetch (uniquement si SSD + RAM >= 8 Go)",
                    "Désactivation intelligente du service SysMain (Superfetch)",
                    "SysMain précharge des applications en mémoire mais génère parfois une activité disque/RAM inutile sur les PC modernes équipés d'un SSD rapide.",
                    "✔ Sécurisé : WinCare vérifie que vous avez un SSD et >= 8 Go de RAM avant de le désactiver",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.pc_optim_opts.optimize_volume_trim,
                    "Exécuter Optimize-Volume (TRIM sur SSD / Défragmentation sur HDD)",
                    "Optimisation du disque système (TRIM SSD / Defrag HDD)",
                    "Appelle la cmdlet officielle Optimize-Volume sur le lecteur C: : exécute un TRIM rapide si c'est un SSD (maintient les performances d'écriture) ou défragmente si c'est un HDD.",
                    "✔ 100 % sans danger et recommandé par Microsoft",
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
                        "• 3. Analyseur des plus gros dossiers dans %TEMP% ({:.1} Mo / {:.2} Go listés)",
                        total_top_mb,
                        total_top_mb / 1024.0
                    ))
                    .strong()
                    .color(Color32::from_rgb(255, 200, 85)),
                );
                if ui
                    .add_enabled(!self.is_busy, egui::Button::new("▶ Scanner %TEMP%"))
                    .on_hover_ui(|ui| {
                        show_rich_tooltip(
                            ui,
                            "Scanner le Top 15 des dossiers dans %TEMP%",
                            "Analyse tous les sous-dossiers de %TEMP% et C:\\Windows\\Temp et les trie du plus lourd au plus léger pour voir exactement ce qui prend de la place.",
                            "✔ 100 % en lecture seule",
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
                                RichText::new("✕ Purger tous ces gros éléments")
                                    .color(Color32::from_rgb(255, 110, 110)),
                            ),
                        )
                        .on_hover_ui(|ui| {
                            show_rich_tooltip(
                                ui,
                                "Purger tous les gros éléments temporaires listés",
                                "Supprime d'un seul coup les sous-dossiers temporaires listés dans ce tableau (protégé pour n'agir qu'à l'intérieur de %TEMP%).",
                                "✔ Libère immédiatement les gigaoctets correspondants",
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
                    RichText::new(
                        "Cliquez sur « Scanner %TEMP% » pour voir quels sous-dossiers occupent vos gigaoctets temporaires.",
                    )
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
                                ui.strong("Action");
                                ui.strong("Nom");
                                ui.strong("Taille");
                                ui.strong("Fichiers");
                                ui.strong("Chemin");
                                ui.end_row();

                                let items_snapshot = self.temp_subfolders.clone();
                                for item in items_snapshot {
                                    if ui
                                        .add_enabled(
                                            !self.is_busy,
                                            egui::Button::new("✕ Supprimer"),
                                        )
                                        .on_hover_text(format!(
                                            "Supprimer uniquement le dossier/fichier temporaire :\n{}",
                                            item.path
                                        ))
                                        .clicked()
                                    {
                                        self.spawn_delete_temp_paths(vec![item.path.clone()]);
                                    }
                                    ui.label(&item.name).on_hover_text(&item.path);
                                    let size_txt = if item.size_mb >= 1024.0 {
                                        format!("{:.2} Go", item.size_mb / 1024.0)
                                    } else {
                                        format!("{:.1} Mo", item.size_mb)
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
                        "• 4. Programmes au démarrage ({} détectés — compatible Gestionnaire des tâches)",
                        self.startup_items.len()
                    ))
                    .strong(),
                );
                if ui
                    .small_button("↻ Actualiser la liste")
                    .on_hover_text("Relit l'état actuel des applications au démarrage dans le Registre Windows.")
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
                            ui.strong("État");
                            ui.strong("Nom");
                            ui.strong("Emplacement");
                            ui.strong("Commande");
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
                                            &format!("Programme au démarrage : {}", item.name),
                                            &format!(
                                                "Commande : {}\nSource : {}\n\nCliquez pour basculer entre [ON] (lancé au démarrage de Windows) et [OFF] (désactivé au démarrage).",
                                                item.command, item.source
                                            ),
                                            "✔ Utilise le mécanisme officiel StartupApproved du Gestionnaire des tâches (aucune entrée n'est supprimée)",
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
                ui.label(RichText::new("• Comparaison Avant / Après").strong());
                render_diff_table(ui, "pc_diff_grid", &self.pc_diffs);
            });
        }

        if let Some(diag) = &self.pc_diag {
            ui.add_space(6.0);
            ui.columns(2, |cols| {
                cols[0].group(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "• Caches & Débris détectés (Total : {} Mo)",
                            diag.cache_total_mb
                        ))
                        .strong(),
                    );
                    egui::Grid::new("caches_grid").striped(true).show(ui, |ui| {
                        ui.strong("Cache");
                        ui.strong("Taille");
                        ui.end_row();
                        for c in &diag.caches {
                            ui.label(&c.label).on_hover_text(&c.path);
                            ui.label(format!("{} Mo", c.size_mb));
                            ui.end_row();
                        }
                    });
                });

                cols[1].group(|ui| {
                    ui.label(RichText::new("• Top Processus par RAM").strong());
                    egui::Grid::new("top_proc_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("PID");
                            ui.strong("Processus");
                            ui.strong("RAM");
                            ui.end_row();
                            for p in &diag.top_processes {
                                ui.label(p.pid.to_string());
                                ui.label(&p.name);
                                ui.label(format!("{} Mo", p.ram_mb));
                                ui.end_row();
                            }
                        });
                });
            });
        }
    }

    fn render_boost_net_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Boost Internet — Diagnostic Réseau, Benchmark DNS & MTU");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(
                            RichText::new("▶ Appliquer les Optimisations Réseau").strong(),
                        ),
                    )
                    .on_hover_text("Applique les optimisations réseau cochées ci-dessous et affiche le comparatif de ping Avant / Après.")
                    .clicked()
                {
                    self.spawn_net_boost();
                }
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("▶ Diagnostiquer la connexion"),
                    )
                    .on_hover_text("Teste en lecture seule votre route réseau, le ping, les serveurs DNS les plus rapides et le MTU optimal sans fragmentation.")
                    .clicked()
                {
                    self.spawn_net_diagnose();
                }
                ui.checkbox(&mut self.include_speedtest, "Speedtest Ookla")
                    .on_hover_text("Exécute également un test de bande passante Download / Upload via Ookla Speedtest CLI.");
            });
        });
        ui.add_space(6.0);

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(
                    RichText::new(
                        "⚙ Optimisations Réseau disponibles (survolez [?] pour les détails)",
                    )
                    .strong(),
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.apply_best_dns,
                    "Appliquer le résolveur DNS le plus rapide (préserve NextDNS si actif)",
                    "Configuration automatique du meilleur serveur DNS",
                    "Teste en temps réel la latence de Cloudflare (1.1.1.1), Google (8.8.8.8), Quad9 (9.9.9.9) et OpenDNS, puis applique le plus rapide sur votre carte active.",
                    "✔ Si NextDNS est détecté sur votre PC, vos DNS ne sont JAMAIS modifiés pour préserver votre filtrage anti-pub",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.apply_optimal_mtu,
                    "Régler le MTU optimal détecté sans fragmentation",
                    "Calibrage du MTU (Maximum Transmission Unit)",
                    "Envoie des paquets ICMP avec le flag 'Don't Fragment' (1500, 1492, 1472...) pour trouver la taille maximale qui passe sans être découpée par votre box/FAI.",
                    "✔ Évite les pertes de paquets et la latence en jeu (annulable dans Restauration)",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.flush_dns,
                    "Vider le cache DNS (ipconfig /flushdns)",
                    "Purge du cache de résolution DNS",
                    "Vide les entrées DNS mises en cache par Windows pour résoudre immédiatement les domaines avec les nouvelles adresses IP.",
                    "✔ 100 % sans danger",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.disable_wifi_power_saving,
                    "Wi-Fi : Désactiver l'économie d'énergie de la carte sans-fil",
                    "Mode Performances Maximales pour la carte Wi-Fi",
                    "Configure le profil d'alimentation de l'adaptateur sans-fil sur 0 (Performance maximale) pour empêcher la carte Wi-Fi de se mettre en micro-veille.",
                    "✔ Réduit les pics de ping (jitter) en Wi-Fi",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.disable_wu_p2p,
                    "Limiter le partage P2P de Windows Update (DODownloadMode = 0)",
                    "Désactivation du partage P2P Windows Update (Delivery Optimization)",
                    "Configure DODownloadMode = 0 pour empêcher Windows d'utiliser votre connexion montante (Upload) afin d'envoyer des mises à jour à d'autres PC sur Internet.",
                    "✔ Libère la bande passante Upload et stabilise le ping",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.disable_smb1,
                    "Désactiver le protocole vétuste SMB1 (sécurité + performance)",
                    "Désactivation du protocole SMBv1",
                    "Désactive la fonctionnalité Windows SMB1Protocol si elle est encore active sur le système.",
                    "✔ Recommandé par Microsoft (sécurité réseau)",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.reset_winsock_tcp,
                    "Reset complet des piles Winsock + TCP/IP (nécessite un redémarrage)",
                    "Réinitialisation Winsock & Pile TCP/IP",
                    "Exécute netsh winsock reset et netsh int ip reset pour réparer les sockets réseau corrompus.",
                    "⚠ Décoché par défaut : nécessite un redémarrage du PC après application",
                    true,
                );

                checkbox_with_help(
                    ui,
                    &mut self.net_optim_opts.run_speedtest_after,
                    "Relancer Speedtest après optimisation pour comparer le débit",
                    "Second test de débit après optimisation",
                    "Si speedtest.exe est disponible, relance un test Download/Upload après application des réglages pour mesurer le delta.",
                    "ℹ Ajoute ~25 secondes à la fin de l'optimisation",
                    false,
                );
            });

            cols[1].group(|ui| {
                ui.label(RichText::new("• État de l'interface & Latence").strong());
                if let Some(d) = &self.net_diag {
                    ui.label(format!("Adaptateur : {} ({})", d.adapter_name, d.adapter_desc));
                    ui.label(format!("Type : {} | Vitesse : {}", d.media_type, d.link_speed));
                    ui.label(format!(
                        "Passerelle : {} (Ping : {:?} ms)",
                        d.gateway, d.ping_gw_ms
                    ));
                    ui.label(format!(
                        "Ping 1.1.1.1 : {:?} ms | Pertes : {}",
                        d.ping_net_ms, d.ping_loss_count
                    ));
                    ui.label(format!(
                        "MTU actuel : {} | MTU optimal sans fragmentation : {:?}",
                        d.current_mtu, d.optimal_mtu
                    ));
                    ui.label(format!(
                        "Débit instantané : ↓ {:.2} Mbps | ↑ {:.2} Mbps",
                        d.rx_mbps, d.tx_mbps
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
                                "✔ NextDNS détecté ({}) — Filtrage préservé",
                                d.nextdns_mode
                            ))
                            .color(Color32::from_rgb(220, 130, 255))
                            .strong(),
                        );
                    }
                } else {
                    ui.label(
                        RichText::new(
                            "Cliquez sur « Diagnostiquer la connexion » pour analyser votre réseau.",
                        )
                        .weak(),
                    );
                }
            });
        });

        if !self.net_diffs.is_empty() {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.label(RichText::new("• Comparaison Réseau Avant / Après").strong());
                render_diff_table(ui, "net_diff_grid", &self.net_diffs);
            });
        }

        if let Some(d) = &self.net_diag {
            ui.add_space(6.0);
            ui.columns(2, |cols| {
                cols[0].group(|ui| {
                    ui.label(RichText::new("⚡ Benchmark des résolveurs DNS").strong());
                    egui::Grid::new("dns_bench_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Résolveur");
                            ui.strong("IP");
                            ui.strong("Latence");
                            ui.strong("Statut");
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
                                        RichText::new("OK").color(Color32::from_rgb(80, 220, 120)),
                                    );
                                } else {
                                    ui.label(
                                        RichText::new("Échec")
                                            .color(Color32::from_rgb(240, 90, 90)),
                                    );
                                }
                                ui.end_row();
                            }
                        });
                });

                cols[1].group(|ui| {
                    ui.label(RichText::new("• Processus avec connexions TCP actives").strong());
                    egui::Grid::new("net_proc_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("PID");
                            ui.strong("Processus");
                            ui.strong("Connexions");
                            ui.strong("RAM");
                            ui.end_row();
                            for p in &d.top_net_processes {
                                ui.label(p.pid.to_string());
                                ui.label(&p.name);
                                ui.label(p.connections.to_string());
                                ui.label(format!("{} Mo", p.ram_mb));
                                ui.end_row();
                            }
                        });
                });
            });
        }
    }

    fn render_debloat_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Debloat & Tweaks Windows 11 (Basé sur Win11Debloat)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("↻ Actualiser l'état")
                    .on_hover_text("Vérifie dans le Registre Windows quels tweaks sont actuellement actifs.")
                    .clicked()
                {
                    self.debloat_tweaks = crate::debloat::list_debloat_tweaks();
                }
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("↻ Tout remettre par défaut"),
                    )
                    .on_hover_text("Annule tous les tweaks de cet onglet et restaure le comportement d'origine de Windows 11.")
                    .clicked()
                {
                    self.spawn_debloat_batch(false);
                }
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new(
                            RichText::new("★ Appliquer les Tweaks Recommandés").strong(),
                        ),
                    )
                    .on_hover_text("Active d'un seul coup tous les tweaks marqués (Recommandé).")
                    .clicked()
                {
                    self.spawn_debloat_batch(true);
                }
            });
        });
        ui.label("Cliquez sur le bouton d'état d'un réglage pour l'activer ou l'annuler instantanément (survolez [?] pour les détails) :");
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
                    RichText::new(cat.label())
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
                    ui.horizontal(|ui| {
                        let btn_text = if tweak.is_applied {
                            RichText::new("✔ [ACTIF]")
                                .color(Color32::from_rgb(80, 225, 130))
                                .strong()
                        } else {
                            RichText::new("  [PAR DÉFAUT]  ")
                                .color(Color32::from_rgb(165, 165, 165))
                        };

                        if ui
                            .add_enabled(!self.is_busy, egui::Button::new(btn_text))
                            .on_hover_ui(|ui| {
                                show_rich_tooltip(
                                    ui,
                                    &tweak.title,
                                    &tweak.description,
                                    "✔ Cliquez pour basculer entre [ACTIF] et [PAR DÉFAUT] (100 % réversible)",
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

                        ui.label(RichText::new(&tweak.title).strong())
                            .on_hover_ui(|ui| {
                                show_rich_tooltip(
                                    ui,
                                    &tweak.title,
                                    &tweak.description,
                                    "✔ Réglage Registre Windows 11 réversible",
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
                                &tweak.title,
                                &tweak.description,
                                "✔ Réglage Registre Windows 11 réversible",
                                false,
                            );
                        });
                        if tweak.recommended {
                            ui.label(
                                RichText::new("(Recommandé)")
                                    .color(Color32::from_rgb(100, 200, 255))
                                    .small(),
                            );
                        }
                        ui.label(RichText::new(format!("— {}", tweak.description)).weak());
                    });
                }
            });
            ui.add_space(4.0);
        }
    }

    fn render_restore_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Restauration des paramètres par défaut (.RESTORE)");
        ui.label("Permet d'annuler à tout moment les optimisations appliquées par Boost-PC ou Boost-Internet (survolez [?] pour les détails).");
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(RichText::new("• Paramètres Système (Boost-PC)").strong());
                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_power_balanced,
                    "Restaurer le plan d'alimentation Équilibré (SCHEME_BALANCED)",
                    "Restauration du plan d'alimentation Équilibré",
                    "Exécute powercfg /setactive SCHEME_BALANCED pour revenir au mode d'alimentation standard de Windows.",
                    "✔ Valeur d'origine Windows",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_visual_fx,
                    "Restaurer les effets visuels automatiques de Windows (VisualFXSetting = 0)",
                    "Restauration des effets visuels par défaut",
                    "Remet VisualFXSetting = 0 pour laisser Windows activer toutes les animations et ombres des fenêtres.",
                    "✔ Valeur d'origine Windows",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_transparency,
                    "Réactiver la transparence Windows (EnableTransparency = 1)",
                    "Réactivation de la transparence",
                    "Remet EnableTransparency = 1 pour réactiver les effets de flou sur la barre des tâches et les fenêtres.",
                    "✔ Valeur d'origine Windows",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_sysmain,
                    "Réactiver et démarrer le service SysMain (Superfetch)",
                    "Réactivation du service SysMain",
                    "Configure le service SysMain en démarrage Automatique et le démarre immédiatement.",
                    "✔ Valeur d'origine Windows",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_storage_sense,
                    "Désactiver Storage Sense automatique",
                    "Désactivation de Storage Sense",
                    "Remet la clé StoragePolicy '01' à 0 pour désactiver le nettoyage automatique périodique.",
                    "✔ Sans danger",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_game_bar,
                    "Réactiver Xbox Game Bar & GameDVR",
                    "Réactivation de Xbox Game Bar & GameDVR",
                    "Remet AutoGameModeEnabled = 1, AllowAutoGameMode = 1 et GameDVR_Enabled = 1.",
                    "✔ Valeur d'origine Windows",
                    false,
                );
            });

            cols[1].group(|ui| {
                ui.label(RichText::new("• Paramètres Réseau (Boost-Internet)").strong());
                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_dns_dhcp,
                    "Restaurer les serveurs DNS automatiques (DHCP)",
                    "Restauration des DNS automatiques de votre Box / FAI",
                    "Exécute Set-DnsClientServerAddress -ResetServerAddresses sur votre carte réseau active.",
                    "⚠ Décochez si vous avez configuré NextDNS manuellement par adresse IP statique",
                    true,
                );
                ui.label(
                    RichText::new(
                        "  ℹ Décochez si vous utilisez NextDNS configuré par IP statique.",
                    )
                    .weak(),
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_mtu_1500,
                    "Restaurer le MTU par défaut (1500)",
                    "Restauration du MTU = 1500",
                    "Remet la taille de paquet standard MTU = 1500 sur la carte réseau active via netsh.",
                    "✔ Valeur d'origine Windows",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_wu_p2p,
                    "Restaurer le comportement par défaut de Delivery Optimization (Windows Update)",
                    "Restauration de Delivery Optimization",
                    "Supprime la valeur DODownloadMode dans HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\DeliveryOptimization.",
                    "✔ Valeur d'origine Windows",
                    false,
                );

                checkbox_with_help(
                    ui,
                    &mut self.restore_opts.restore_wifi_power,
                    "Restaurer l'économie d'énergie Wi-Fi par défaut",
                    "Restauration de l'économie d'énergie Wi-Fi",
                    "Remet l'indice d'économie d'énergie de l'adaptateur sans-fil à sa valeur par défaut (3).",
                    "✔ Valeur d'origine Windows",
                    false,
                );
            });
        });

        ui.add_space(10.0);
        if ui
            .add_enabled(
                !self.is_busy,
                egui::Button::new(RichText::new("↻ Restaurer les paramètres cochés").strong()),
            )
            .on_hover_text("Applique immédiatement les restaurations sélectionnées ci-dessus.")
            .clicked()
        {
            self.spawn_restore();
        }
    }

    fn render_log_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("• Console d'exécution & Journal Temps Réel").strong());
            ui.label(RichText::new(format!("({})", self.log_file_path.display())).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Effacer").clicked() {
                    self.logs.clear();
                }
                ui.checkbox(&mut self.auto_scroll_logs, "Défilement auto");
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

fn render_diff_table(ui: &mut egui::Ui, id: &str, diffs: &[MetricDiff]) {
    egui::Grid::new(id)
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            ui.strong("Métrique");
            ui.strong("Avant");
            ui.strong("Après");
            ui.strong("Delta");
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
