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
    pub lang: Option<crate::i18n::Lang>,
}

impl CliArgs {
    pub fn parse() -> Self {
        let mut args = Self::default();
        let raw_args: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < raw_args.len() {
            let arg = &raw_args[i];
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
                "--lang" => {
                    if let Some(next) = raw_args.get(i + 1) {
                        let next_lower = next.to_lowercase();
                        if next_lower == "fr" {
                            args.lang = Some(crate::i18n::Lang::Fr);
                            i += 1;
                        } else if next_lower == "en" {
                            args.lang = Some(crate::i18n::Lang::En);
                            i += 1;
                        }
                    }
                }
                s if s.starts_with("--lang=") => {
                    let val = s.trim_start_matches("--lang=").to_lowercase();
                    if val == "fr" {
                        args.lang = Some(crate::i18n::Lang::Fr);
                    } else if val == "en" {
                        args.lang = Some(crate::i18n::Lang::En);
                    }
                }
                "--help" | "-h" | "/?" => {
                    args.show_help = true;
                    args.cli_mode = true;
                }
                _ => {}
            }
            i += 1;
        }
        args
    }

    pub fn print_help() {
        use std::io::Write;
        let lang = crate::i18n::Lang::detect();
        let mut out = std::io::stdout();
        match lang {
            crate::i18n::Lang::En => {
                let _ = writeln!(
                    out,
                    "WinCare v{} — Lightweight All-in-One Windows Repair and Optimization Tool",
                    env!("CARGO_PKG_VERSION")
                );
                let _ = writeln!(out);
                let _ = writeln!(out, "USAGE:");
                let _ = writeln!(
                    out,
                    "  win-care.exe                 Launch the native Graphical User Interface (GUI)"
                );
                let _ = writeln!(
                    out,
                    "  win-care.exe [OPTIONS]       Command-line execution or test options"
                );
                let _ = writeln!(out);
                let _ = writeln!(out, "OPTIONS:");
                let _ = writeln!(
                    out,
                    "  --lang <en|fr>               Select UI and console language (e.g. --lang en or --lang fr)"
                );
                let _ = writeln!(
                    out,
                    "  --auto, --auto-repair        Run all Windows repairs (equivalent to Repair-Windows-Auto.ps1)"
                );
                let _ = writeln!(
                    out,
                    "  --boost-pc                   Run safe PC cleaning and optimizations"
                );
                let _ = writeln!(
                    out,
                    "  --boost-net                  Run safe Internet optimizations"
                );
                let _ = writeln!(
                    out,
                    "  --all-auto                   Run Repair + Boost-PC + Boost-Internet automatically"
                );
                let _ = writeln!(
                    out,
                    "  --update                     Check and install the latest update from GitHub"
                );
                let _ = writeln!(
                    out,
                    "  --diagnose                   Run full diagnostic (PC + Internet) without applying changes"
                );
                let _ = writeln!(
                    out,
                    "  --dry-run                    Simulation mode: displays and logs commands without system changes"
                );
                let _ = writeln!(
                    out,
                    "  --no-elevate                 Do not automatically elevate via UAC if not administrator"
                );
                let _ = writeln!(
                    out,
                    "  --help, -h                   Display this help message"
                );
            }
            crate::i18n::Lang::Fr => {
                let _ = writeln!(
                    out,
                    "WinCare v{} — Outil tout-en-un léger de réparation et d'optimisation Windows",
                    env!("CARGO_PKG_VERSION")
                );
                let _ = writeln!(out);
                let _ = writeln!(out, "UTILISATION :");
                let _ = writeln!(
                    out,
                    "  win-care.exe                 Lance l'interface graphique native (GUI)"
                );
                let _ = writeln!(
                    out,
                    "  win-care.exe [OPTIONS]       Exécution en ligne de commande ou options de test"
                );
                let _ = writeln!(out);
                let _ = writeln!(out, "OPTIONS :");
                let _ = writeln!(
                    out,
                    "  --lang <en|fr>               Sélectionne la langue de l'interface ou de la console (ex: --lang fr)"
                );
                let _ = writeln!(
                    out,
                    "  --auto, --auto-repair        Lance toutes les réparations Windows (équivalent Repair-Windows-Auto.ps1)"
                );
                let _ = writeln!(
                    out,
                    "  --boost-pc                   Lance le nettoyage et les optimisations safe de Boost-PC"
                );
                let _ = writeln!(
                    out,
                    "  --boost-net                  Lance les optimisations safe de Boost-Internet"
                );
                let _ = writeln!(
                    out,
                    "  --all-auto                   Exécute Repair + Boost-PC + Boost-Internet en mode automatique"
                );
                let _ = writeln!(
                    out,
                    "  --update                     Vérifie et installe automatiquement la dernière mise à jour depuis GitHub"
                );
                let _ = writeln!(
                    out,
                    "  --diagnose                   Exécute uniquement le diagnostic complet (PC + Internet) sans rien modifier"
                );
                let _ = writeln!(
                    out,
                    "  --dry-run                    Mode simulation : affiche et logge les commandes sans modifier le système"
                );
                let _ = writeln!(
                    out,
                    "  --no-elevate                 Ne relance pas automatiquement via UAC si non-administrateur"
                );
                let _ = writeln!(
                    out,
                    "  --help, -h                   Affiche cette aide"
                );
            }
        }
    }
}
