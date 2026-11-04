use crate::logger::Logger;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

pub const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

#[derive(Clone)]
pub struct Runner {
    pub logger: Logger,
    pub dry_run: bool,
    pub cancelled: Arc<AtomicBool>,
    pub active_child_pid: Arc<AtomicU32>,
}

impl Runner {
    pub fn new(logger: Logger, dry_run: bool) -> Self {
        Self {
            logger,
            dry_run,
            cancelled: Arc::new(AtomicBool::new(false)),
            active_child_pid: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Indique si l'utilisateur a demandé l'annulation de la tâche en cours.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Annule immédiatement la tâche en cours et tue l'arbre du sous-processus actif (ex: `winget.exe`).
    pub fn cancel(&self) {
        let was_already_cancelled = self.cancelled.swap(true, Ordering::SeqCst);
        let pid = self.active_child_pid.swap(0, Ordering::SeqCst);
        if pid != 0 {
            let pid_str = pid.to_string();
            let _ = Command::new("taskkill.exe")
                .args(["/PID", &pid_str, "/T", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if !was_already_cancelled {
            self.logger
                .warn("⏹ Opération annulée par l'utilisateur (arrêt du processus en cours).");
        }
    }

    /// Décode intelligemment la sortie brute d'un outil console Windows :
    /// - Détecte l'UTF-16LE (utilisé par `sfc.exe` sous Windows)
    /// - Essaie l'UTF-8 valide
    /// - Repli sur le décodage OEM CP850 / Windows-1252 (accents français dans `ipconfig`, `netsh`, `dism`, `ping`)
    pub fn decode_windows_bytes(bytes: &[u8]) -> String {
        if bytes.is_empty() {
            return String::new();
        }

        // Détection BOM UTF-16LE ou alternance d'octets nuls typique de sfc.exe
        if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            let u16s: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            return String::from_utf16_lossy(&u16s);
        }

        let null_odd_count = bytes
            .iter()
            .skip(1)
            .step_by(2)
            .filter(|&&b| b == 0)
            .count();
        if bytes.len() >= 8 && null_odd_count * 3 >= (bytes.len() / 2) * 2 {
            let u16s: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            return String::from_utf16_lossy(&u16s);
        }

        if let Ok(utf8) = std::str::from_utf8(bytes) {
            return utf8.to_string();
        }

        // Table de correspondance CP850 (OEM français Windows) pour les octets >= 0x80
        bytes.iter().map(|&b| cp850_to_char(b)).collect()
    }

    /// Exécute une commande de modification système (bloquée et simulée si `dry_run` est actif).
    pub fn run_action(&self, program: &str, args: &[&str], stream_to_log: bool) -> CommandOutput {
        if self.is_cancelled() {
            return CommandOutput {
                exit_code: -2,
                stdout: String::new(),
                stderr: String::from("Cancelled"),
            };
        }
        let cmd_str = format!("{} {}", program, args.join(" "));
        if self.dry_run {
            self.logger
                .info(format!("[DRY-RUN] Commande simulée : {cmd_str}"));
            return CommandOutput {
                exit_code: 0,
                stdout: String::from("[DRY-RUN]"),
                stderr: String::new(),
            };
        }
        self.exec_internal(program, args, None, stream_to_log)
    }

    /// Exécute une commande de modification système avec une entrée stdin (ex: `echo Y | chkdsk`).
    pub fn run_action_with_stdin(
        &self,
        program: &str,
        args: &[&str],
        stdin_input: &str,
        stream_to_log: bool,
    ) -> CommandOutput {
        if self.is_cancelled() {
            return CommandOutput {
                exit_code: -2,
                stdout: String::new(),
                stderr: String::from("Cancelled"),
            };
        }
        let cmd_str = format!(
            "echo {:?} | {} {}",
            stdin_input.trim(),
            program,
            args.join(" ")
        );
        if self.dry_run {
            self.logger
                .info(format!("[DRY-RUN] Commande simulée : {cmd_str}"));
            return CommandOutput {
                exit_code: 0,
                stdout: String::from("[DRY-RUN]"),
                stderr: String::new(),
            };
        }
        self.exec_internal(program, args, Some(stdin_input), stream_to_log)
    }

    /// Exécute une commande de lecture/diagnostic (toujours exécutée même en `dry_run`).
    pub fn run_query(&self, program: &str, args: &[&str]) -> CommandOutput {
        self.exec_internal(program, args, None, false)
    }

    /// Exécute une expression PowerShell en lecture seule (encodée en UTF-8).
    pub fn run_ps_query(&self, script: &str) -> CommandOutput {
        let wrapped = format!(
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $ErrorActionPreference = 'SilentlyContinue'; {script}"
        );
        self.run_query(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &wrapped,
            ],
        )
    }

    /// Exécute une action PowerShell modifiante (simulée si `dry_run` est actif).
    pub fn run_ps_action(
        &self,
        description: &str,
        script: &str,
        stream_to_log: bool,
    ) -> CommandOutput {
        if self.is_cancelled() {
            return CommandOutput {
                exit_code: -2,
                stdout: String::new(),
                stderr: String::from("Cancelled"),
            };
        }
        if self.dry_run {
            self.logger
                .info(format!("[DRY-RUN] Action PowerShell simulée : {description}"));
            return CommandOutput {
                exit_code: 0,
                stdout: String::from("[DRY-RUN]"),
                stderr: String::new(),
            };
        }
        let wrapped = format!(
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $ErrorActionPreference = 'SilentlyContinue'; {script}"
        );
        self.exec_internal(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &wrapped,
            ],
            None,
            stream_to_log,
        )
    }

    fn exec_internal(
        &self,
        program: &str,
        args: &[&str],
        stdin_data: Option<&str>,
        stream_to_log: bool,
    ) -> CommandOutput {
        if self.is_cancelled() {
            return CommandOutput {
                exit_code: -2,
                stdout: String::new(),
                stderr: String::from("Cancelled"),
            };
        }

        let mut cmd = Command::new(program);
        cmd.args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if stdin_data.is_some() {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Impossible de lancer {program}: {e}");
                if stream_to_log {
                    self.logger.warn(&err_msg);
                }
                return CommandOutput {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: err_msg,
                };
            }
        };

        let child_pid = child.id();
        self.active_child_pid.store(child_pid, Ordering::SeqCst);
        if self.is_cancelled() {
            self.cancel();
        }

        if let Some(input) = stdin_data {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(input.as_bytes());
            }
        }

        let out_pipe = child.stdout.take();
        let err_pipe = child.stderr.take();

        // Lecture concurrente temps réel sur stdout et stderr (zéro risque de deadlock IPC)
        let (stdout, stderr) = std::thread::scope(|s| {
            let logger_out = self.logger.clone();
            let out_handle = s.spawn(move || {
                if let Some(pipe) = out_pipe {
                    read_stream_realtime(pipe, stream_to_log, false, &logger_out)
                } else {
                    String::new()
                }
            });

            let logger_err = self.logger.clone();
            let err_handle = s.spawn(move || {
                if let Some(pipe) = err_pipe {
                    read_stream_realtime(pipe, stream_to_log, true, &logger_err)
                } else {
                    String::new()
                }
            });

            (
                out_handle.join().unwrap_or_default(),
                err_handle.join().unwrap_or_default(),
            )
        });

        let exit_code = child
            .wait()
            .ok()
            .and_then(|st| st.code())
            .unwrap_or(-1);

        self.active_child_pid.store(0, Ordering::SeqCst);

        CommandOutput {
            exit_code,
            stdout,
            stderr,
        }
    }
}

/// Lit un flux (stdout ou stderr) en temps réel par blocs et émet chaque ligne ou progression (\n ou \r)
/// immédiatement vers le logger si `stream_to_log` est actif.
fn read_stream_realtime<R: Read>(
    mut reader: R,
    stream_to_log: bool,
    is_stderr: bool,
    logger: &Logger,
) -> String {
    let mut all_bytes = Vec::new();
    let mut line_buf = Vec::new();
    let mut chunk = [0u8; 512];

    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let slice = &chunk[..n];
                all_bytes.extend_from_slice(slice);

                if stream_to_log {
                    for &b in slice {
                        if b == b'\n' || b == b'\r' {
                            if !line_buf.is_empty() {
                                emit_decoded_line(&line_buf, is_stderr, logger);
                                line_buf.clear();
                            }
                        } else {
                            line_buf.push(b);
                        }
                    }
                }
            }
            Err(_) => break,
        }
    }

    if stream_to_log && !line_buf.is_empty() {
        emit_decoded_line(&line_buf, is_stderr, logger);
    }

    Runner::decode_windows_bytes(&all_bytes)
}

fn emit_decoded_line(raw_line: &[u8], is_stderr: bool, logger: &Logger) {
    let decoded = Runner::decode_windows_bytes(raw_line);
    let cleaned: String = decoded.chars().filter(|&c| c != '\0').collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return;
    }
    if is_stderr {
        logger.warn(format!("  {trimmed}"));
    } else {
        logger.info(format!("  {trimmed}"));
    }
}

fn cp850_to_char(b: u8) -> char {
    if b < 0x80 {
        return b as char;
    }
    const CP850_TABLE: [char; 128] = [
        'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å',
        'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', 'ø', '£', 'Ø', '×', 'ƒ',
        'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '®', '¬', '½', '¼', '¡', '«', '»',
        '░', '▒', '▓', '│', '┤', 'Á', 'Â', 'À', '©', '╣', '║', '╗', '╝', '¢', '¥', '┐',
        '└', '┴', '┬', '├', '─', '┼', 'ã', 'Ã', '╚', '╔', '╩', '╦', '╠', '═', '╬', '¤',
        'ð', 'Ð', 'Ê', 'Ë', 'È', 'ı', 'Í', 'Î', 'Ï', '┘', '┌', '█', '▄', '¦', 'Ì', '▀',
        'Ó', 'ß', 'Ô', 'Ò', 'õ', 'Õ', 'µ', 'þ', 'Þ', 'Ú', 'Û', 'Ù', 'ý', 'Ý', '¯', '´',
        '\u{00AD}', '±', '‗', '¾', '¶', '§', '÷', '¸', '°', '¨', '·', '¹', '³', '²', '■', '\u{00A0}',
    ];
    CP850_TABLE[(b - 0x80) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_utf8() {
        let s = "Réparation terminée avec succès";
        assert_eq!(Runner::decode_windows_bytes(s.as_bytes()), s);
    }

    #[test]
    fn test_decode_utf16le() {
        let text = "La protection des ressources Windows";
        let mut bytes = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(Runner::decode_windows_bytes(&bytes), text);
    }

    #[test]
    fn test_decode_cp850() {
        // 'é' en CP850 = 0x82, 'è' = 0x8A
        let bytes = [b'R', 0x82, b's', b'e', b'a', b'u', b' ', 0x8A];
        assert_eq!(Runner::decode_windows_bytes(&bytes), "Réseau è");
    }
}
