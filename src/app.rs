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
/// pour afficher toutes les icônes Unicode sans aucun carré `□` manquant.
fn configure_windows_fonts(ctx: &egui::Context) {
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
        configure_windows_fonts(ctx);

        let (tx, rx) = mpsc::channel();
        let init_logger = Logger::new(Some(tx.clone()), false);
        let log_file_path = init_logger.log_path.clone();

        init_logger.info(
            "WinCare v0.2 prêt. Survolez n'importe quelle option avec la souris pour afficher son explication détaillée.",
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
                RichText::new("⚡ WinCare v0.2")
                    .strong()
                    .color(Color32::from_rgb(90, 200, 250)),
            );
            ui.label(RichText::new("— Repair, Boost & Win11 Debloat").weak());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut self.dry_run, "Mode Simulation (Dry-Run)")
                    .on_hover_text(
                        "Mode test : simule et affiche toutes les commandes dans le journal en bas sans modifier aucun fichier ni clé de registre.",
                    );

                ui.separator();

                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("★ Créer Point de Restauration"),
                    )
                    .on_hover_text(
                        "Crée un point de restauration système Windows officiel (Checkpoint-Computer) pour pouvoir revenir en arrière dans Windows en cas de besoin.",
                    )
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
                        .on_hover_text(
                            "Télécharge le nouveau WinCare.exe depuis GitHub Releases et remplace automatiquement votre exécutable actuel.",
                        )
                        .clicked()
                    {
                        self.spawn_check_update(true);
                    }
                } else if ui
                    .add_enabled(!self.is_busy, egui::Button::new("↻ Vérifier MAJ"))
                    .on_hover_text(
                        "Interroge le dépôt GitHub (yaniswav/win-care) pour vérifier si une version plus récente de WinCare.exe est disponible.",
                    )
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
        ui.label("Survolez n'importe quel bouton ou case à cocher pour voir en détail ce que fait chaque action.");
        ui.add_space(8.0);

        ui.columns(4, |cols| {
            cols[0].group(|ui| {
                ui.heading(
                    RichText::new("▶ Diagnostic Complet")
                        .color(Color32::from_rgb(100, 210, 255)),
                );
                ui.label("Analyse PC + Top dossiers %TEMP% + Connexion Internet (Route, Ping, DNS, MTU) en lecture seule.");
                ui.add_space(4.0);
                ui.checkbox(&mut self.include_speedtest, "Inclure Speedtest Ookla")
                    .on_hover_text("Si coché, exécute un test de débit complet Download/Upload via Ookla Speedtest CLI (~25 secondes).");
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("▶ Lancer le Diagnostic Complet"),
                    )
                    .on_hover_text("100 % en lecture seule : mesure votre RAM, CPU, disques, caches %TEMP%, programmes au démarrage, ping, DNS et MTU sans rien modifier.")
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
                    .on_hover_text("Lance d'une traite DISM + SFC + Réparation Windows Update + Reset Réseau + Nettoyage Temp + Réparation Store + Services + Planification CHKDSK au redémarrage.")
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
                        .on_hover_text("Exécute les nettoyages et optimisations cochés dans l'onglet Boost PC, puis affiche le gain de RAM et d'espace disque.")
                        .clicked()
                    {
                        self.active_tab = ActiveTab::BoostPc;
                        self.spawn_pc_boost();
                    }
                    if ui
                        .add_enabled(!self.is_busy, egui::Button::new("🌐 Boost Net"))
                        .on_hover_text("Exécute les optimisations réseau cochées dans l'onglet Boost Internet (DNS le plus rapide, MTU optimal, Flush DNS, Wi-Fi performance).")
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
                    .on_hover_text("Applique automatiquement les 12 tweaks Windows 11 recommandés (sans toucher aux préférences personnelles comme l'accélération souris ou le thème).")
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
        ui.label("Sélectionnez les outils de réparation système à exécuter dans l'ordre :");
        ui.add_space(6.0);

        for task in RepairTask::ALL {
            let mut checked = self.selected_repairs.contains(&task);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(
                            &mut checked,
                            RichText::new(format!("[{}] {}", task.code(), task.label())).strong(),
                        )
                        .on_hover_text(task.description())
                        .changed()
                    {
                        if checked {
                            self.selected_repairs.insert(task);
                        } else {
                            self.selected_repairs.remove(&task);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(!self.is_busy, egui::Button::new("Exécuter seul"))
                            .on_hover_text("Lance uniquement cette étape de réparation tout de suite.")
                            .clicked()
                        {
                            self.spawn_repair(vec![task], self.schedule_chkdsk_reboot);
                        }
                    });
                });
                ui.label(RichText::new(task.description()).weak());

                if task == RepairTask::Chkdsk {
                    ui.checkbox(
                        &mut self.schedule_chkdsk_reboot,
                        "Planifier également un CHKDSK complet (chkdsk C: /F /R /X) au prochain redémarrage",
                    )
                    .on_hover_text("Si coché, programme une vérification minutieuse des secteurs du disque C: lors du prochain redémarrage de Windows (peut durer 15 à 45 min au boot).");
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
                    .on_hover_text("Exécute uniquement les cases cochées dans « 1. Options de Nettoyage » et « 2. Optimisations Système », puis affiche le tableau comparatif Avant / Après.")
                    .clicked()
                {
                    self.spawn_pc_boost();
                }
                if ui
                    .add_enabled(
                        !self.is_busy,
                        egui::Button::new("▶ Analyser le PC & %TEMP%"),
                    )
                    .on_hover_text("Analyse en lecture seule l'état du PC et liste les sous-dossiers les plus lourds dans %TEMP%.")
                    .clicked()
                {
                    self.spawn_pc_diagnose();
                }
            });
        });
        ui.add_space(6.0);

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(RichText::new("• 1. Options de Nettoyage (survolez pour les détails)").strong());

                ui.checkbox(&mut self.pc_clean_opts.clean_user_temp, "Vider %TEMP% utilisateur")
                    .on_hover_text(
                        "Supprime les fichiers et dossiers temporaires de votre session (C:\\Users\\<Vous>\\AppData\\Local\\Temp).\n\
                        ✔ Sans danger : les fichiers actuellement utilisés par vos logiciels ouverts sont automatiquement ignorés.",
                    );

                ui.checkbox(
                    &mut self.pc_clean_opts.clean_sys_temp,
                    "Vider C:\\Windows\\Temp système",
                )
                .on_hover_text(
                    "Supprime les fichiers temporaires laissés par les services système et installateurs dans C:\\Windows\\Temp.\n\
                    ✔ Sans danger : libère de l'espace disque sans affecter vos logiciels.",
                );

                ui.checkbox(
                    &mut self.pc_clean_opts.clean_wu_download,
                    "Vider le cache Windows Update (SoftwareDistribution\\Download)",
                )
                .on_hover_text(
                    "Arrête momentanément le service Windows Update, supprime les anciens installateurs de mises à jour déjà téléchargés dans C:\\Windows\\SoftwareDistribution\\Download, puis relance le service.\n\
                    ✔ Récupère souvent plusieurs gigaoctets et débloque les mises à jour coincées.",
                );

                ui.checkbox(
                    &mut self.pc_clean_opts.clean_wer_dumps,
                    "Supprimer les rapports d'erreur (WER) et CrashDumps",
                )
                .on_hover_text(
                    "Supprime les archives de rapports de plantage Windows (Windows Error Reporting) et les fichiers mémoire .dmp dans %LOCALAPPDATA%\\CrashDumps.\n\
                    ✔ Utile pour récupérer de la place si des jeux ou applications ont généré de gros fichiers de crash.",
                );

                ui.checkbox(
                    &mut self.pc_clean_opts.empty_recycle_bin,
                    "Vider la corbeille de tous les disques",
                )
                .on_hover_text(
                    "Vide définitivement la Corbeille Windows sur tous vos disques.\n\
                    ⚠ Décochez cette case si vous gardez des fichiers dans votre Corbeille que vous souhaiteriez récupérer !",
                );

                ui.checkbox(
                    &mut self.pc_clean_opts.dism_component_cleanup,
                    "Nettoyer le WinSxS (DISM /StartComponentCleanup /ResetBase — lent)",
                )
                .on_hover_text(
                    "Purge les anciennes versions des composants système Windows remplacés par des mises à jour récentes (dossier C:\\Windows\\WinSxS).\n\
                    ℹ Décoché par défaut car l'opération dure 5 à 15 minutes. À cocher une fois de temps en temps pour gagner 2 à 8 Go sur C:.",
                );

                ui.checkbox(
                    &mut self.pc_clean_opts.run_cleanmgr,
                    "Lancer le nettoyage de disque Windows (cleanmgr silencieux)",
                )
                .on_hover_text(
                    "Configure et exécute l'outil officiel « Nettoyage de disque » de Microsoft (cleanmgr.exe) pour purger les caches système, miniatures et anciens journaux d'installation.",
                );
            });

            cols[1].group(|ui| {
                ui.label(RichText::new("⚡ 2. Optimisations Système (Mode SAFE — réversibles)").strong());

                ui.checkbox(
                    &mut self.pc_optim_opts.high_perf_power,
                    "Plan d'alimentation : Performances élevées (SCHEME_MIN)",
                )
                .on_hover_text(
                    "Active le profil d'alimentation « Performances élevées » de Windows pour éviter que le processeur ne réduise agressivement sa fréquence.\n\
                    ✔ Recommandé sur PC fixe (tour) ou PC portable branché sur secteur.",
                );

                ui.checkbox(
                    &mut self.pc_optim_opts.ultimate_perf_power,
                    "Activer le plan 'Ultimate Performance' (créé si absent)",
                )
                .on_hover_text(
                    "Débloque et active le profil caché « Performances optimales » de Windows Pro/Workstation, qui supprime les micro-latences de mise en veille des composants.\n\
                    ℹ Décoché par défaut (utile pour le gaming compétitif ou la MAO sur tour).",
                );

                ui.checkbox(
                    &mut self.pc_optim_opts.reduce_visual_fx,
                    "Réduire les effets visuels (VisualFXSetting = 2 'Meilleures performances')",
                )
                .on_hover_text(
                    "Règle les effets visuels de l'Explorateur Windows en mode « Ajuster afin d'obtenir les meilleures performances » (désactive les ombres et animations superflues des fenêtres).\n\
                    ✔ Rend l'ouverture des fenêtres instantanée (annulable en 1 clic dans l'onglet Restauration).",
                );

                ui.checkbox(
                    &mut self.pc_optim_opts.enable_storage_sense,
                    "Activer Storage Sense (nettoyage automatique périodique 30j)",
                )
                .on_hover_text(
                    "Active l'Assistant Stockage natif de Windows pour qu'il nettoie automatiquement les fichiers temporaires et les éléments de la corbeille de plus de 30 jours.",
                );

                ui.checkbox(
                    &mut self.pc_optim_opts.disable_game_bar,
                    "Désactiver l'overlay Xbox Game Bar & GameDVR",
                )
                .on_hover_text(
                    "Désactive l'enregistrement vidéo en arrière-plan de Xbox GameDVR et l'overlay Game Bar (sans désinstaller l'application Xbox).\n\
                    ✔ Évite les chutes de FPS et micro-saccades (stuttering) dans les jeux.",
                );

                ui.checkbox(
                    &mut self.pc_optim_opts.disable_transparency,
                    "Désactiver les effets de transparence Windows",
                )
                .on_hover_text(
                    "Désactive l'effet de flou Acrylique / Mica sur la barre des tâches et les fenêtres pour soulager le GPU.",
                );

                ui.checkbox(
                    &mut self.pc_optim_opts.disable_sysmain,
                    "Désactiver SysMain / Superfetch (uniquement si SSD + RAM >= 8 Go)",
                )
                .on_hover_text(
                    "Désactive le service SysMain (anciennement Superfetch) qui précharge des applications en RAM et sollicite le disque en arrière-plan.\n\
                    ✔ Sécurisé : WinCare ne l'applique que si vous avez un SSD et au moins 8 Go de RAM.",
                );

                ui.checkbox(
                    &mut self.pc_optim_opts.optimize_volume_trim,
                    "Exécuter Optimize-Volume (TRIM sur SSD / Défragmentation sur HDD)",
                )
                .on_hover_text(
                    "Lance la commande officielle Optimize-Volume sur le disque C: : envoie l'instruction TRIM si c'est un SSD (maintient la vitesse d'écriture) ou défragmente si c'est un disque dur mécanique.",
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
                    .on_hover_text("Scanne les sous-dossiers de %TEMP% et C:\\Windows\\Temp pour vous montrer exactement quels dossiers prennent le plus de gigaoctets.")
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
                        .on_hover_text("Supprime tous les gros sous-dossiers temporaires listés ci-dessous.")
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
                                        .on_hover_text("Supprime immédiatement ce dossier/fichier temporaire.")
                                        .clicked()
                                    {
                                        self.spawn_delete_temp_paths(vec![item.path.clone()]);
                                    }
                                    ui.label(&item.name);
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
                                    .on_hover_text("Cliquez pour activer [ON] ou désactiver [OFF] ce programme au démarrage de Windows (sans supprimer l'entrée, 100 % réversible).")
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
                                ui.label(&item.name);
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
                            ui.label(&c.label);
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
                ui.label(RichText::new("⚙ Optimisations Réseau disponibles (survolez pour les détails)").strong());

                ui.checkbox(
                    &mut self.net_optim_opts.apply_best_dns,
                    "Appliquer le résolveur DNS le plus rapide (préserve NextDNS si actif)",
                )
                .on_hover_text(
                    "Configure automatiquement sur votre carte réseau le serveur DNS ayant obtenu la plus faible latence en millisecondes (Cloudflare, Google, Quad9, OpenDNS).\n\
                    ✔ Protection intelligente : si NextDNS est détecté sur votre PC, vos DNS ne sont jamais remplacés pour préserver votre blocage de pubs.",
                );

                ui.checkbox(
                    &mut self.net_optim_opts.apply_optimal_mtu,
                    "Régler le MTU optimal détecté sans fragmentation",
                )
                .on_hover_text(
                    "Ajuste la taille maximale des paquets IPv4 (MTU) sur votre carte réseau selon le test ICMP 'Don't Fragment' (ex: 1500 ou 1492) pour éviter la fragmentation des paquets.",
                );

                ui.checkbox(
                    &mut self.net_optim_opts.flush_dns,
                    "Vider le cache DNS (ipconfig /flushdns)",
                )
                .on_hover_text(
                    "Purge le cache de résolution DNS local de Windows pour forcer l'actualisation des adresses IP des sites web.",
                );

                ui.checkbox(
                    &mut self.net_optim_opts.disable_wifi_power_saving,
                    "Wi-Fi : Désactiver l'économie d'énergie de la carte sans-fil",
                )
                .on_hover_text(
                    "Configure la carte Wi-Fi en mode « Performances maximales » dans le profil d'alimentation pour éviter les pics de ping liés à la mise en veille radio.",
                );

                ui.checkbox(
                    &mut self.net_optim_opts.disable_wu_p2p,
                    "Limiter le partage P2P de Windows Update (DODownloadMode = 0)",
                )
                .on_hover_text(
                    "Empêche Windows Update d'utiliser votre bande passante montante (Upload) pour envoyer des mises à jour à d'autres PC sur Internet (Delivery Optimization).",
                );

                ui.checkbox(
                    &mut self.net_optim_opts.disable_smb1,
                    "Désactiver le protocole vétuste SMB1 (sécurité + performance)",
                )
                .on_hover_text(
                    "Désactive l'ancien protocole de partage réseau SMBv1 s'il est encore actif (protocole obsolète et vulnérable).",
                );

                ui.checkbox(
                    &mut self.net_optim_opts.reset_winsock_tcp,
                    "Reset complet des piles Winsock + TCP/IP (nécessite un redémarrage)",
                )
                .on_hover_text(
                    "Réinitialise complètement les catalogues Winsock et la pile TCP/IP de Windows.\n\
                    ℹ Décoché par défaut : à utiliser si votre connexion a des coupures anormales après un VPN ou un malware (nécessite de redémarrer le PC).",
                );

                ui.checkbox(
                    &mut self.net_optim_opts.run_speedtest_after,
                    "Relancer Speedtest après optimisation pour comparer le débit",
                )
                .on_hover_text(
                    "Lance un second test de débit Ookla après l'application des réglages pour afficher le gain Download / Upload dans le tableau comparatif.",
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
        ui.label("Cliquez sur le bouton d'état d'un réglage pour l'activer ou l'annuler instantanément dans le Registre Windows :");
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
                            .on_hover_text(&tweak.description)
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
                            .on_hover_text(&tweak.description);
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
        ui.label("Permet d'annuler à tout moment les optimisations appliquées par Boost-PC ou Boost-Internet.");
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.label(RichText::new("• Paramètres Système (Boost-PC)").strong());
                ui.checkbox(
                    &mut self.restore_opts.restore_power_balanced,
                    "Restaurer le plan d'alimentation Équilibré (SCHEME_BALANCED)",
                )
                .on_hover_text("Remet le mode de gestion d'alimentation standard « Équilibré » de Windows.");

                ui.checkbox(
                    &mut self.restore_opts.restore_visual_fx,
                    "Restaurer les effets visuels automatiques de Windows (VisualFXSetting = 0)",
                )
                .on_hover_text("Réactive toutes les animations, ombres et effets esthétiques par défaut des fenêtres Windows.");

                ui.checkbox(
                    &mut self.restore_opts.restore_transparency,
                    "Réactiver la transparence Windows (EnableTransparency = 1)",
                )
                .on_hover_text("Réactive l'effet de transparence de la barre des tâches et du menu Démarrer.");

                ui.checkbox(
                    &mut self.restore_opts.restore_sysmain,
                    "Réactiver et démarrer le service SysMain (Superfetch)",
                )
                .on_hover_text("Remet le service SysMain en démarrage Automatique et le démarre immédiatement.");

                ui.checkbox(
                    &mut self.restore_opts.restore_storage_sense,
                    "Désactiver Storage Sense automatique",
                )
                .on_hover_text("Désactive le nettoyage périodique automatique de l'Assistant Stockage Windows.");

                ui.checkbox(
                    &mut self.restore_opts.restore_game_bar,
                    "Réactiver Xbox Game Bar & GameDVR",
                )
                .on_hover_text("Réactive l'overlay Xbox Game Bar (Win+G) et l'enregistrement GameDVR.");
            });

            cols[1].group(|ui| {
                ui.label(RichText::new("• Paramètres Réseau (Boost-Internet)").strong());
                ui.checkbox(
                    &mut self.restore_opts.restore_dns_dhcp,
                    "Restaurer les serveurs DNS automatiques (DHCP)",
                )
                .on_hover_text("Supprime les serveurs DNS personnalisés de la carte réseau pour réutiliser ceux fournis automatiquement par votre box Internet (DHCP).");
                ui.label(
                    RichText::new(
                        "  ℹ Décochez si vous utilisez NextDNS configuré par IP statique.",
                    )
                    .weak(),
                );

                ui.checkbox(
                    &mut self.restore_opts.restore_mtu_1500,
                    "Restaurer le MTU par défaut (1500)",
                )
                .on_hover_text("Remet la taille de paquet standard MTU = 1500 sur la carte réseau active.");

                ui.checkbox(
                    &mut self.restore_opts.restore_wu_p2p,
                    "Restaurer le comportement par défaut de Delivery Optimization (Windows Update)",
                )
                .on_hover_text("Supprime la règle de registre qui bloquait le partage P2P des mises à jour Windows.");

                ui.checkbox(
                    &mut self.restore_opts.restore_wifi_power,
                    "Restaurer l'économie d'énergie Wi-Fi par défaut",
                )
                .on_hover_text("Remet le niveau d'économie d'énergie standard (3) sur l'adaptateur Wi-Fi.");
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
