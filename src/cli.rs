#[derive(Debug, Clone, Default)]
pub struct CliArgs {
    pub auto_repair: bool,
    pub auto_boost_pc: bool,
    pub auto_boost_net: bool,
    pub self_update: bool,
    pub diagnose_only: bool,
    pub dry_run: bool,
    pub no_elevate: bool,
    pub cli_mode: bool,
    pub show_help: bool,
}

impl CliArgs {
    pub fn parse() -> Self {
        let mut args = Self::default();
        for arg in std::env::args().skip(1) {
            match arg.as_str() {
                "--auto" | "--auto-repair" => {
                    args.auto_repair = true;
                    args.cli_mode = true;
                }
                "--boost-pc" => {
                    args.auto_boost_pc = true;
                    args.cli_mode = true;
                }
                "--boost-net" => {
                    args.auto_boost_net = true;
                    args.cli_mode = true;
                }
                "--all-auto" => {
                    args.auto_repair = true;
                    args.auto_boost_pc = true;
                    args.auto_boost_net = true;
                    args.cli_mode = true;
                }
                "--update" => {
                    args.self_update = true;
                    args.cli_mode = true;
                }
                "--diagnose" | "-DiagnoseOnly" => {
                    args.diagnose_only = true;
                    args.cli_mode = true;
                }
                "--dry-run" => {
                    args.dry_run = true;
                }
                "--cli" => {
                    args.cli_mode = true;
                }
                "--no-elevate" => {
                    args.no_elevate = true;
                }
                "--help" | "-h" | "/?" => {
                    args.show_help = true;
                    args.cli_mode = true;
                }
                _ => {}
            }
        }
        args
    }

    pub fn print_help() {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = writeln!(out, "WinCare v0.1.0 — Outil tout-en-un léger de réparation et d'optimisation Windows");
        let _ = writeln!(out);
        let _ = writeln!(out, "UTILISATION :");
        let _ = writeln!(out, "  win-care.exe                 Lance l'interface graphique native (GUI)");
        let _ = writeln!(out, "  win-care.exe [OPTIONS]       Exécution en ligne de commande ou options de test");
        let _ = writeln!(out);
        let _ = writeln!(out, "OPTIONS :");
        let _ = writeln!(out, "  --auto, --auto-repair        Lance toutes les réparations Windows (équivalent Repair-Windows-Auto.ps1)");
        let _ = writeln!(out, "  --boost-pc                   Lance le nettoyage et les optimisations safe de Boost-PC");
        let _ = writeln!(out, "  --boost-net                  Lance les optimisations safe de Boost-Internet");
        let _ = writeln!(out, "  --all-auto                   Exécute Repair + Boost-PC + Boost-Internet en mode automatique");
        let _ = writeln!(out, "  --update                     Vérifie et installe automatiquement la dernière mise à jour depuis GitHub");
        let _ = writeln!(out, "  --diagnose                   Exécute uniquement le diagnostic complet (PC + Internet) sans rien modifier");
        let _ = writeln!(out, "  --dry-run                    Mode simulation : affiche et logge les commandes sans modifier le système");
        let _ = writeln!(out, "  --no-elevate                 Ne relance pas automatiquement via UAC si non-administrateur");
        let _ = writeln!(out, "  --help, -h                   Affiche cette aide");
    }
}
