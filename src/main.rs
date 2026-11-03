#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app;
pub mod boost_net;
pub mod boost_pc;
pub mod cli;
pub mod debloat;
pub mod logger;
pub mod repair;
pub mod restore;
pub mod runner;
pub mod types;
pub mod updater;

use cli::CliArgs;
use logger::Logger;
use runner::Runner;
use std::sync::Arc;
use types::{NetOptimOptions, PcCleanOptions, PcOptimOptions, RepairTask};

pub fn is_elevated() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

pub fn attach_parent_console() {
    unsafe {
        let attached = windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
        if attached == 0 {
            let _ = windows_sys::Win32::System::Console::AllocConsole();
        }
    }
}

pub fn relaunch_as_admin() -> bool {
    use std::os::windows::ffi::OsStrExt;
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let args_vec: Vec<String> = std::env::args()
        .skip(1)
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a
            }
        })
        .collect();
    let params = args_vec.join(" ");

    let verb_w: Vec<u16> = std::ffi::OsStr::new("runas")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let file_w: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let params_w: Vec<u16> = std::ffi::OsStr::new(&params)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let res = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb_w.as_ptr(),
            file_w.as_ptr(),
            params_w.as_ptr(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL as i32,
        )
    };
    (res as isize) > 32
}

/// Génère une icône bouclier 32x32 en RGBA pour la fenêtre et la barre des tâches Windows.
fn build_shield_icon() -> Arc<eframe::egui::IconData> {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = vec![0u8; (width * height * 4) as usize];

    for y in 0..height {
        for x in 0..width {
            let fx = x as f32 - 15.5;
            let fy = y as f32;
            let half_w = if fy < 4.0 {
                0.0
            } else if fy < 16.0 {
                12.5
            } else if fy < 29.0 {
                12.5 * (1.0 - ((fy - 16.0) / 13.5).powf(1.35))
            } else {
                0.0
            };

            let idx = ((y * width + x) * 4) as usize;
            if half_w > 0.0 && fx.abs() <= half_w {
                let border = (half_w - fx.abs()) < 1.6 || (fy - 4.0) < 1.6;
                if border {
                    rgba[idx] = 110;
                    rgba[idx + 1] = 215;
                    rgba[idx + 2] = 255;
                    rgba[idx + 3] = 255;
                } else if fx < 0.0 {
                    rgba[idx] = 24;
                    rgba[idx + 1] = 128;
                    rgba[idx + 2] = 210;
                    rgba[idx + 3] = 255;
                } else {
                    rgba[idx] = 16;
                    rgba[idx + 1] = 96;
                    rgba[idx + 2] = 175;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }

    Arc::new(eframe::egui::IconData {
        rgba,
        width,
        height,
    })
}

fn main() {
    updater::cleanup_old_binary();
    let args = CliArgs::parse();

    if args.cli_mode || args.dry_run {
        attach_parent_console();
    }

    if args.show_help {
        CliArgs::print_help();
        return;
    }

    let admin = is_elevated();

    // Auto-élévation UAC au lancement (sauf si --dry-run, --diagnose ou --no-elevate)
    if !admin && !args.dry_run && !args.diagnose_only && !args.no_elevate {
        if relaunch_as_admin() {
            return;
        }
    }

    if args.cli_mode {
        run_cli_mode(&args, admin);
        return;
    }

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("WinCare v0.2 — Réparation, Optimisation & Debloat Windows")
            .with_inner_size([1120.0, 760.0])
            .with_min_inner_size([880.0, 600.0])
            .with_icon(build_shield_icon()),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "WinCare",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(app::WinCareApp::new(
                &cc.egui_ctx,
                admin,
                args.dry_run,
            )))
        }),
    );
}

fn run_cli_mode(args: &CliArgs, admin: bool) {
    let logger = Logger::new(None, true);
    logger.section("WINCARE v0.2 - MODE LIGNE DE COMMANDE");
    logger.info(format!(
        "Droits Administrateur : {} | Dry-Run : {}",
        if admin { "OUI" } else { "NON" },
        if args.dry_run { "OUI" } else { "NON" }
    ));
    logger.info(format!("Journal enregistré dans : {}", logger.log_path.display()));

    let runner = Runner::new(logger.clone(), args.dry_run);

    if args.self_update {
        logger.section("MISE A JOUR AUTOMATIQUE DEPUIS GITHUB");
        if let Some(info) = updater::check_for_update(&runner) {
            if info.update_available {
                updater::perform_self_update(&runner, &info);
            }
        }
        return;
    }

    if args.diagnose_only {
        logger.section("DIAGNOSTIC PC COMPLET");
        let _pc = boost_pc::run_pc_diagnosis(&runner);
        logger.section("DIAGNOSTIC INTERNET COMPLET");
        let _net = boost_net::run_net_diagnosis(&runner, false);
        logger.success("Diagnostic terminé (aucune modification appliquée).");
        return;
    }

    let mut need_reboot = false;

    if args.auto_repair {
        logger.section("REPARATION AUTOMATIQUE WINDOWS");
        if repair::execute_repair_tasks(&runner, &RepairTask::ALL, true) {
            need_reboot = true;
        }
    }

    if args.auto_boost_pc {
        logger.section("BOOST PC AUTOMATIQUE (MODE SAFE)");
        let before = boost_pc::run_pc_diagnosis(&runner);
        boost_pc::run_pc_cleaning(&runner, &PcCleanOptions::default());
        boost_pc::run_pc_optimizations(&runner, &PcOptimOptions::default(), &before);
        let _diffs = boost_pc::compute_pc_comparison(&runner, &before);
    }

    if args.auto_boost_net {
        logger.section("BOOST INTERNET AUTOMATIQUE (MODE SAFE)");
        let before = boost_net::run_net_diagnosis(&runner, false);
        if boost_net::run_net_optimizations(&runner, &NetOptimOptions::default(), &before) {
            need_reboot = true;
        }
        let _diffs = boost_net::compute_net_comparison(&runner, &before, false);
    }

    if need_reboot {
        logger.warn("IMPORTANT : Un redémarrage du PC est recommandé pour finaliser les réparations.");
    }
    logger.success("Opérations terminées.");
}
