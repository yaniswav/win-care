use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use crate::runner::Runner;

pub const TASK_NAME: &str = "WinCare_Weekly_Maintenance";
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn is_weekly_task_enabled() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn set_weekly_task(runner: &Runner, enabled: bool) -> bool {
    if enabled {
        let Ok(exe) = std::env::current_exe() else {
            runner
                .logger
                .error("Impossible de déterminer le chemin de l'exécutable courant pour la tâche planifiée.");
            return false;
        };
        let tr_arg = format!("\"{}\" --boost-pc --no-elevate", exe.display());
        let res = runner.run_action(
            "schtasks",
            &[
                "/Create",
                "/TN",
                TASK_NAME,
                "/TR",
                &tr_arg,
                "/SC",
                "WEEKLY",
                "/D",
                "SUN",
                "/ST",
                "12:00",
                "/RL",
                "HIGHEST",
                "/F",
            ],
            true,
        );
        res.success()
    } else {
        let res = runner.run_action("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"], true);
        res.success()
    }
}
