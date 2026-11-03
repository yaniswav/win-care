use crate::runner::Runner;
use crate::types::{DnsBenchResult, MetricDiff, NetDiagnosis, NetOptimOptions, NetProcInfo};
use serde::Deserialize;
use winreg::enums::*;
use winreg::RegKey;

const MTU_CANDIDATES: [u32; 7] = [1500, 1492, 1472, 1454, 1450, 1430, 1400];
const WIFI_SUBGROUP_GUID: &str = "SUB_WIRELESSADAPTER";
const WIFI_POWER_SETTING_GUID: &str = "12bbebe6-58d6-4636-95bb-3217ef867c1a";
const DELIVERY_OPT_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PsNetDiagPayload {
    if_index: u32,
    adapter_name: String,
    adapter_desc: String,
    media_type: String,
    physical_media_type: String,
    link_speed: String,
    gateway: String,
    current_dns: Vec<String>,
    current_mtu: u32,
    ping_gw_ms: Option<f64>,
    ping_net_ms: Option<f64>,
    ping_loss_count: u32,
    uses_nextdns: bool,
    nextdns_mode: String,
    dns_results: Vec<DnsBenchResult>,
    top_net_processes: Vec<NetProcInfo>,
    rx_mbps: f64,
    tx_mbps: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PsAfterNetPayload {
    ping_net_ms: Option<f64>,
    ping_loss_count: u32,
    dns_lookup_ms: Option<u64>,
}

pub fn run_net_diagnosis(runner: &Runner, run_speedtest: bool) -> NetDiagnosis {
    runner
        .logger
        .section("DIAGNOSTIC INTERNET & RÉSEAU");
    runner
        .logger
        .progress(1, 5, "Analyse de l'interface active, du ping et des DNS...");

    let ps_script = r#"
$activeIf = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue |
    Sort-Object -Property RouteMetric | Select-Object -First 1

if (-not $activeIf) {
    $upAdapter = Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object Status -eq 'Up' | Select-Object -First 1
    $ifIndex = if ($upAdapter) { [uint32]$upAdapter.ifIndex } else { [uint32]0 }
    $gateway = ''
} else {
    $ifIndex = [uint32]$activeIf.InterfaceIndex
    $gateway = [string]$activeIf.NextHop
}

$adapterName = ''
$adapterDesc = ''
$mediaType = ''
$physMediaType = ''
$linkSpeed = ''
$currentMtu = [uint32]1500
$currentDns = @()

if ($ifIndex -gt 0) {
    $ifAdapter = Get-NetAdapter -InterfaceIndex $ifIndex -ErrorAction SilentlyContinue
    if ($ifAdapter) {
        $adapterName = [string]$ifAdapter.Name
        $adapterDesc = [string]$ifAdapter.InterfaceDescription
        $mediaType = [string]$ifAdapter.MediaType
        $physMediaType = [string]$ifAdapter.PhysicalMediaType
        $linkSpeed = [string]$ifAdapter.LinkSpeed
    }
    $ifIp = Get-NetIPInterface -InterfaceIndex $ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
    if ($ifIp -and $ifIp.NlMtu) {
        $currentMtu = [uint32]$ifIp.NlMtu
    }
    $dnsObj = Get-DnsClientServerAddress -InterfaceIndex $ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
    if ($dnsObj -and $dnsObj.ServerAddresses) {
        $currentDns = @($dnsObj.ServerAddresses | ForEach-Object { [string]$_ })
    }
}

$pingGwMs = $null
if ($gateway -and $gateway -ne '0.0.0.0') {
    $pgw = @(Test-Connection -ComputerName $gateway -Count 4 -ErrorAction SilentlyContinue | Where-Object { $null -ne $_.ResponseTime })
    if ($pgw.Count -gt 0) {
        $pingGwMs = [math]::Round((($pgw | Measure-Object ResponseTime -Average).Average), 1)
    }
}

$pingNetMs = $null
$pingLoss = [uint32]4
$pnet = @(Test-Connection -ComputerName '1.1.1.1' -Count 4 -ErrorAction SilentlyContinue | Where-Object { $null -ne $_.ResponseTime })
if ($pnet.Count -gt 0) {
    $pingNetMs = [math]::Round((($pnet | Measure-Object ResponseTime -Average).Average), 1)
    $pingLoss = [uint32](4 - $pnet.Count)
}

$nextDnsIps = @('45.90.28.0','45.90.30.0','45.90.28.165','45.90.30.165')
$usesNextDnsByIp = @($currentDns | Where-Object { $_ -in $nextDnsIps -or $_ -like '45.90.*' }).Count -gt 0
$usesNextDnsApp  = $null -ne (Get-Process -Name 'NextDNS','nextdns' -ErrorAction SilentlyContinue)
$usesNextDnsSvc  = $null -ne (Get-Service -Name '*nextdns*' -ErrorAction SilentlyContinue | Where-Object Status -eq 'Running')
$usesNextDns     = [bool]($usesNextDnsByIp -or $usesNextDnsApp -or $usesNextDnsSvc)

$nextDnsMode = ''
if ($usesNextDnsApp -or $usesNextDnsSvc) {
    $nextDnsMode = 'Appli/Service local (DoH/DoT)'
} elseif ($usesNextDnsByIp) {
    $nextDnsMode = 'IP anycast classique'
}

$sysDns = $currentDns | Select-Object -First 1
$dnsTargets = @(
    @{ Name = 'NextDNS anycast'; Ip = '45.90.28.0' },
    @{ Name = 'Cloudflare';      Ip = '1.1.1.1' },
    @{ Name = 'Google';          Ip = '8.8.8.8' },
    @{ Name = 'Quad9';           Ip = '9.9.9.9' },
    @{ Name = 'OpenDNS';         Ip = '208.67.222.222' }
)
if ($sysDns) {
    $dnsTargets += @{ Name = 'Actuel/système'; Ip = [string]$sysDns }
}

$dnsResults = @()
foreach ($d in $dnsTargets) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        Resolve-DnsName -Name 'www.google.com' -Server $d.Ip -DnsOnly -QuickTimeout -ErrorAction Stop | Out-Null
        $sw.Stop()
        $dnsResults += [pscustomobject]@{
            resolver   = [string]$d.Name
            ip         = [string]$d.Ip
            latency_ms = [uint64]$sw.ElapsedMilliseconds
            ok         = $true
        }
    } catch {
        $sw.Stop()
        $dnsResults += [pscustomobject]@{
            resolver   = [string]$d.Name
            ip         = [string]$d.Ip
            latency_ms = $null
            ok         = $false
        }
    }
}

$topProcs = @()
$conns = Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue |
    Group-Object OwningProcess | Sort-Object Count -Descending | Select-Object -First 10
foreach ($g in $conns) {
    $procId = 0
    if ([int]::TryParse($g.Name, [ref]$procId) -and $procId -gt 0) {
        $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
        if ($p) {
            $topProcs += [pscustomobject]@{
                pid         = [uint32]$p.Id
                name        = [string]$p.ProcessName
                connections = [int]$g.Count
                ram_mb      = [uint64][math]::Round($p.WorkingSet64 / 1MB, 0)
            }
        }
    }
}

$rxMbps = 0.0
$txMbps = 0.0
if ($adapterName) {
    $s1 = Get-NetAdapterStatistics -Name $adapterName -ErrorAction SilentlyContinue
    if ($s1) {
        Start-Sleep -Milliseconds 1200
        $s2 = Get-NetAdapterStatistics -Name $adapterName -ErrorAction SilentlyContinue
        if ($s2) {
            $rxMbps = [math]::Round((($s2.ReceivedBytes - $s1.ReceivedBytes) * 8.0 / 1.2 / 1MB), 2)
            $txMbps = [math]::Round((($s2.SentBytes     - $s1.SentBytes)     * 8.0 / 1.2 / 1MB), 2)
            if ($rxMbps -lt 0) { $rxMbps = 0.0 }
            if ($txMbps -lt 0) { $txMbps = 0.0 }
        }
    }
}

$result = [pscustomobject]@{
    if_index             = $ifIndex
    adapter_name         = $adapterName
    adapter_desc         = $adapterDesc
    media_type           = $mediaType
    physical_media_type  = $physMediaType
    link_speed           = $linkSpeed
    gateway              = $gateway
    current_dns          = $currentDns
    current_mtu          = $currentMtu
    ping_gw_ms           = $pingGwMs
    ping_net_ms          = $pingNetMs
    ping_loss_count      = $pingLoss
    uses_nextdns         = $usesNextDns
    nextdns_mode         = $nextDnsMode
    dns_results          = $dnsResults
    top_net_processes    = $topProcs
    rx_mbps              = $rxMbps
    tx_mbps              = $txMbps
}
$result | ConvertTo-Json -Depth 5 -Compress
"#;

    let mut diag = NetDiagnosis::default();
    let ps_out = runner.run_ps_query(ps_script);

    let mut phys_media = String::new();
    if let Some(json_str) = extract_json_object(&ps_out.stdout) {
        if let Ok(payload) = serde_json::from_str::<PsNetDiagPayload>(json_str) {
            diag.if_index = payload.if_index;
            diag.adapter_name = payload.adapter_name;
            diag.adapter_desc = payload.adapter_desc;
            diag.media_type = payload.media_type;
            phys_media = payload.physical_media_type;
            diag.link_speed = payload.link_speed;
            diag.gateway = payload.gateway;
            diag.current_dns = payload.current_dns;
            diag.current_mtu = if payload.current_mtu > 0 {
                payload.current_mtu
            } else {
                1500
            };
            diag.ping_gw_ms = payload.ping_gw_ms;
            diag.ping_net_ms = payload.ping_net_ms;
            diag.ping_loss_count = payload.ping_loss_count;
            diag.uses_nextdns = payload.uses_nextdns;
            diag.nextdns_mode = payload.nextdns_mode;
            diag.dns_results = payload.dns_results;
            diag.top_net_processes = payload.top_net_processes;
            diag.rx_mbps = payload.rx_mbps;
            diag.tx_mbps = payload.tx_mbps;
        } else {
            runner
                .logger
                .warn("Impossible de désérialiser les données réseau PowerShell.");
        }
    } else {
        runner
            .logger
            .warn("Aucune réponse JSON reçue lors du diagnostic réseau PowerShell.");
    }

    // Détermination du meilleur DNS (ok == true et latence minimale)
    diag.best_dns = diag
        .dns_results
        .iter()
        .filter(|d| d.ok && d.latency_ms.is_some())
        .min_by_key(|d| d.latency_ms.unwrap_or(u64::MAX))
        .cloned();

    runner.logger.info(format!(
        "Interface active : {} ({}) [Index #{}]",
        if diag.adapter_name.is_empty() {
            "Inconnue"
        } else {
            &diag.adapter_name
        },
        if diag.adapter_desc.is_empty() {
            "N/A"
        } else {
            &diag.adapter_desc
        },
        diag.if_index
    ));
    runner.logger.info(format!(
        "Média : {} | Vitesse : {} | Passerelle : {} | MTU : {}",
        if diag.media_type.is_empty() {
            "N/A"
        } else {
            &diag.media_type
        },
        if diag.link_speed.is_empty() {
            "N/A"
        } else {
            &diag.link_speed
        },
        if diag.gateway.is_empty() {
            "N/A"
        } else {
            &diag.gateway
        },
        diag.current_mtu
    ));
    runner.logger.info(format!(
        "DNS actuels : {}",
        if diag.current_dns.is_empty() {
            String::from("DHCP / Aucun")
        } else {
            diag.current_dns.join(", ")
        }
    ));

    if let Some(gw_ms) = diag.ping_gw_ms {
        runner.logger.info(format!(
            "Latence passerelle ({}) : {:.1} ms",
            diag.gateway, gw_ms
        ));
    }
    if let Some(net_ms) = diag.ping_net_ms {
        runner.logger.info(format!(
            "Latence Internet (1.1.1.1) : {:.1} ms (pertes : {}/4)",
            net_ms, diag.ping_loss_count
        ));
    } else {
        runner
            .logger
            .warn("Ping vers 1.1.1.1 indisponible ou bloqué.");
    }

    if diag.uses_nextdns {
        runner.logger.info(format!(
            "NextDNS détecté ({}) — le filtrage actif sera préservé.",
            diag.nextdns_mode
        ));
    }
    if let Some(best) = &diag.best_dns {
        runner.logger.info(format!(
            "Meilleur résolveur DNS : {} ({}) en {} ms",
            best.resolver,
            best.ip,
            best.latency_ms.unwrap_or(0)
        ));
    }

    // 2. Analyse Wi-Fi
    runner
        .logger
        .progress(2, 5, "Vérification du signal et du canal Wi-Fi...");
    let media_combined = format!(
        "{} {}",
        diag.media_type.to_lowercase(),
        phys_media.to_lowercase()
    );
    if media_combined.contains("802.11")
        || media_combined.contains("wireless")
        || media_combined.contains("wi-fi")
        || media_combined.contains("wlan")
    {
        diag.is_wifi = true;
    }

    let wlan_out = runner.run_query("netsh", &["wlan", "show", "interfaces"]);
    if wlan_out.success() {
        let mut wifi_lines = Vec::new();
        let mut signal_pct = None;

        for line in wlan_out.stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lower = trimmed.to_lowercase();
            let is_useful = lower.contains("ssid")
                || lower.contains("signal")
                || lower.contains("radio")
                || lower.contains("channel")
                || lower.contains("canal")
                || lower.contains("receive")
                || lower.contains("réception")
                || lower.contains("reception")
                || lower.contains("transmit")
                || lower.contains("transmission");

            if is_useful {
                wifi_lines.push(trimmed.to_string());
            }
            if signal_pct.is_none() && lower.contains("signal") {
                signal_pct = extract_percentage(trimmed);
            }
        }

        if signal_pct.is_some() {
            diag.is_wifi = true;
        }
        if diag.is_wifi {
            diag.wifi_signal_pct = signal_pct;
            diag.wifi_info_lines = wifi_lines;
            if let Some(sig) = diag.wifi_signal_pct {
                runner
                    .logger
                    .info(format!("Carte Wi-Fi active — Signal : {sig}%"));
            }
        }
    }

    // 4. MTU optimal (Don't Fragment)
    runner
        .logger
        .progress(3, 5, "Recherche du MTU optimal (Don't Fragment)...");
    for size in MTU_CANDIDATES {
        let payload = size - 28;
        let payload_str = payload.to_string();
        let ping_out = runner.run_query(
            "ping",
            &["1.1.1.1", "-f", "-l", &payload_str, "-n", "1"],
        );
        let stdout_upper = ping_out.stdout.to_ascii_uppercase();
        if ping_out.exit_code == 0 && stdout_upper.contains("TTL=") {
            diag.optimal_mtu = Some(size);
            runner
                .logger
                .info(format!("MTU optimal détecté : {size} octets (payload {payload})"));
            break;
        }
    }

    if diag.optimal_mtu.is_none() {
        runner.logger.warn(
            "Impossible de déterminer le MTU par ping (ICMP possiblement filtré).",
        );
    }

    runner.logger.info(format!(
        "Débit instantané : Réception {:.2} Mbps | Émission {:.2} Mbps",
        diag.rx_mbps, diag.tx_mbps
    ));

    // 5. Speedtest optionnel
    if run_speedtest {
        runner
            .logger
            .progress(4, 5, "Exécution du test de débit Speedtest CLI...");
        if let Some((dl, ul, ping)) = run_ookla_speedtest(runner) {
            diag.speedtest_dl_mbps = Some(dl);
            diag.speedtest_ul_mbps = Some(ul);
            diag.speedtest_ping_ms = Some(ping);
        }
    }

    runner
        .logger
        .progress(5, 5, "Diagnostic réseau terminé.");
    runner
        .logger
        .success("Diagnostic Internet terminé avec succès.");

    diag
}

pub fn run_net_optimizations(
    runner: &Runner,
    opts: &NetOptimOptions,
    diag: &NetDiagnosis,
) -> bool {
    runner
        .logger
        .section("OPTIMISATIONS INTERNET & RÉSEAU");

    let mut need_reboot = false;
    let total_steps = 7usize;

    // 1. DNS
    runner
        .logger
        .progress(1, total_steps, "Configuration des serveurs DNS...");
    if opts.apply_best_dns {
        if diag.uses_nextdns {
            runner.logger.warn(format!(
                "NextDNS est actif ({}) : préservé (aucun écrasement DNS appliqué).",
                if diag.nextdns_mode.is_empty() {
                    "filtrage actif"
                } else {
                    &diag.nextdns_mode
                }
            ));
        } else if let Some(best) = &diag.best_dns {
            let current_primary = diag.current_dns.first().map(|s| s.as_str()).unwrap_or("");
            if best.ok && !best.ip.is_empty() && diag.if_index > 0 {
                if best.ip != current_primary {
                    let ps_cmd = format!(
                        "Set-DnsClientServerAddress -InterfaceIndex {} -ServerAddresses ('{}','8.8.8.8')",
                        diag.if_index, best.ip
                    );
                    let desc = format!(
                        "Application du DNS {} ({}) + 8.8.8.8 sur l'interface #{}",
                        best.resolver, best.ip, diag.if_index
                    );
                    let out = runner.run_ps_action(&desc, &ps_cmd, true);
                    if out.success() {
                        runner.logger.success(format!(
                            "Serveurs DNS configurés sur {} (primaire) et 8.8.8.8 (secondaire).",
                            best.ip
                        ));
                    } else {
                        runner
                            .logger
                            .warn("Échec lors de la modification des serveurs DNS.");
                    }
                } else {
                    runner.logger.info(format!(
                        "Le résolveur DNS le plus rapide ({}) est déjà configuré.",
                        best.ip
                    ));
                }
            } else {
                runner
                    .logger
                    .info("Aucun résolveur DNS plus rapide ou interface valide détectée.");
            }
        }
    }

    // 2. MTU optimal
    runner
        .logger
        .progress(2, total_steps, "Ajustement du MTU de l'interface...");
    if opts.apply_optimal_mtu {
        if let Some(mtu) = diag.optimal_mtu {
            if mtu != diag.current_mtu && !diag.adapter_name.is_empty() {
                let mtu_arg = format!("mtu={mtu}");
                let out = runner.run_action(
                    "netsh",
                    &[
                        "interface",
                        "ipv4",
                        "set",
                        "subinterface",
                        &diag.adapter_name,
                        &mtu_arg,
                        "store=persistent",
                    ],
                    true,
                );
                if out.success() {
                    runner.logger.success(format!(
                        "MTU réglé à {mtu} sur '{}' (précédent : {}).",
                        diag.adapter_name, diag.current_mtu
                    ));
                } else {
                    runner.logger.warn(format!(
                        "Impossible d'appliquer le MTU {mtu} sur '{}'.",
                        diag.adapter_name
                    ));
                }
            } else {
                runner.logger.info(format!(
                    "Le MTU actuel ({}) est déjà optimal.",
                    diag.current_mtu
                ));
            }
        } else {
            runner
                .logger
                .info("MTU optimal non mesuré, aucun changement appliqué.");
        }
    }

    // 3. Reset Winsock + TCP/IP
    runner
        .logger
        .progress(3, total_steps, "Réinitialisation Winsock & TCP/IP...");
    if opts.reset_winsock_tcp {
        runner
            .logger
            .info("Réinitialisation du catalogue Winsock et de la pile TCP/IP...");
        let out_ws = runner.run_action("netsh", &["winsock", "reset"], true);
        let out_ip = runner.run_action("netsh", &["int", "ip", "reset"], true);
        need_reboot = true;
        if out_ws.success() && out_ip.success() {
            runner
                .logger
                .success("Winsock et TCP/IP réinitialisés (redémarrage requis).");
        } else {
            runner.logger.warn(
                "Réinitialisation Winsock/TCP exécutée avec des avertissements (redémarrage requis).",
            );
        }
    }

    // 4. Flush DNS
    runner
        .logger
        .progress(4, total_steps, "Vidage du cache DNS...");
    if opts.flush_dns {
        let out = runner.run_action("ipconfig", &["/flushdns"], true);
        if out.success() {
            runner.logger.success("Cache DNS vidé (ipconfig /flushdns).");
        } else {
            runner.logger.warn("Impossible de vider le cache DNS.");
        }
    }

    // 5. Désactiver l'économie d'énergie Wi-Fi
    runner
        .logger
        .progress(5, total_steps, "Optimisation de l'alimentation Wi-Fi...");
    if opts.disable_wifi_power_saving {
        if diag.is_wifi {
            let r1 = runner.run_action(
                "powercfg",
                &[
                    "/setacvalueindex",
                    "SCHEME_CURRENT",
                    WIFI_SUBGROUP_GUID,
                    WIFI_POWER_SETTING_GUID,
                    "0",
                ],
                true,
            );
            let r2 = runner.run_action(
                "powercfg",
                &[
                    "/setdcvalueindex",
                    "SCHEME_CURRENT",
                    WIFI_SUBGROUP_GUID,
                    WIFI_POWER_SETTING_GUID,
                    "0",
                ],
                true,
            );
            let r3 = runner.run_action("powercfg", &["/setactive", "SCHEME_CURRENT"], true);
            if r1.success() && r2.success() && r3.success() {
                runner
                    .logger
                    .success("Carte Wi-Fi configurée en performance maximale (sans économie d'énergie).");
            } else {
                runner
                    .logger
                    .warn("Certaines commandes powercfg Wi-Fi ont retourné un avertissement.");
            }
        } else {
            runner
                .logger
                .info("Connexion non Wi-Fi : réglage d'économie d'énergie sans-fil ignoré.");
        }
    }

    // 6. Désactiver Windows Update P2P (Delivery Optimization)
    runner
        .logger
        .progress(6, total_steps, "Restriction du P2P Windows Update...");
    if opts.disable_wu_p2p {
        if runner.dry_run {
            runner.logger.info(format!(
                "[DRY-RUN] Registre simulé : HKLM\\{DELIVERY_OPT_KEY}\\DODownloadMode = 0 (DWORD)"
            ));
        } else {
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            match hklm.create_subkey(DELIVERY_OPT_KEY) {
                Ok((key, _)) => match key.set_value("DODownloadMode", &0u32) {
                    Ok(()) => runner.logger.success(
                        "Delivery Optimization P2P désactivé (DODownloadMode = 0).",
                    ),
                    Err(e) => runner.logger.warn(format!(
                        "Impossible d'écrire DODownloadMode dans le registre : {e}"
                    )),
                },
                Err(e) => runner.logger.warn(format!(
                    "Impossible d'ouvrir la clé DeliveryOptimization (droits admin requis) : {e}"
                )),
            }
        }
    }

    // 7. Désactiver SMB1
    runner
        .logger
        .progress(7, total_steps, "Désactivation du protocole SMB1...");
    if opts.disable_smb1 {
        let out = runner.run_ps_action(
            "Désactivation de la fonctionnalité SMB1Protocol",
            "Disable-WindowsOptionalFeature -Online -FeatureName SMB1Protocol -NoRestart",
            true,
        );
        if out.success() {
            runner.logger.success("Protocole SMB1 désactivé.");
        } else {
            runner
                .logger
                .warn("Impossible de modifier l'état de SMB1Protocol (droits admin requis).");
        }
    }

    runner
        .logger
        .progress(total_steps, total_steps, "Optimisations réseau terminées.");
    need_reboot
}

pub fn compute_net_comparison(
    runner: &Runner,
    before: &NetDiagnosis,
    run_speedtest: bool,
) -> Vec<MetricDiff> {
    runner
        .logger
        .section("COMPARAISON RÉSEAU AVANT / APRÈS");
    runner
        .logger
        .progress(1, 2, "Re-mesure de la latence ping et DNS...");

    let ps_script = r#"
$pingNetMs = $null
$pingLoss = [uint32]4
$pnet = @(Test-Connection -ComputerName '1.1.1.1' -Count 4 -ErrorAction SilentlyContinue | Where-Object { $null -ne $_.ResponseTime })
if ($pnet.Count -gt 0) {
    $pingNetMs = [math]::Round((($pnet | Measure-Object ResponseTime -Average).Average), 1)
    $pingLoss = [uint32](4 - $pnet.Count)
}

$dnsMs = $null
$sw = [System.Diagnostics.Stopwatch]::StartNew()
try {
    Resolve-DnsName -Name 'www.google.com' -DnsOnly -QuickTimeout -ErrorAction Stop | Out-Null
    $sw.Stop()
    $dnsMs = [uint64]$sw.ElapsedMilliseconds
} catch {
    $sw.Stop()
}

[pscustomobject]@{
    ping_net_ms     = $pingNetMs
    ping_loss_count = $pingLoss
    dns_lookup_ms   = $dnsMs
} | ConvertTo-Json -Compress
"#;

    let ps_out = runner.run_ps_query(ps_script);
    let after_net = extract_json_object(&ps_out.stdout)
        .and_then(|s| serde_json::from_str::<PsAfterNetPayload>(s).ok())
        .unwrap_or_default();

    let mut diffs = Vec::new();

    // 1. Ping 1.1.1.1
    diffs.push(build_f64_diff(
        "Ping 1.1.1.1",
        before.ping_net_ms,
        after_net.ping_net_ms,
        "ms",
        true,
    ));

    // 2. Pertes de paquets
    let loss_before = before.ping_loss_count as i64;
    let loss_after = after_net.ping_loss_count as i64;
    let loss_delta = loss_after - loss_before;
    let loss_improved = if loss_delta < 0 {
        Some(true)
    } else if loss_delta > 0 {
        Some(false)
    } else {
        None
    };
    diffs.push(MetricDiff {
        metric: String::from("Pertes ping (/4)"),
        before: format!("{loss_before}/4"),
        after: format!("{loss_after}/4"),
        delta: format!("{loss_delta:+}"),
        improved: loss_improved,
    });

    // 3. Latence de résolution DNS
    let before_dns_ms = before
        .dns_results
        .iter()
        .find(|d| d.resolver.contains("Actuel") || before.current_dns.first().map(|ip| ip == &d.ip).unwrap_or(false))
        .and_then(|d| d.latency_ms)
        .or_else(|| before.best_dns.as_ref().and_then(|d| d.latency_ms));

    diffs.push(build_u64_diff(
        "Résolution DNS",
        before_dns_ms,
        after_net.dns_lookup_ms,
        "ms",
        true,
    ));

    // 4. Speedtest optionnel
    if run_speedtest {
        runner
            .logger
            .progress(2, 2, "Relance de Speedtest pour comparaison...");
        let after_st = run_ookla_speedtest(runner);
        let (after_dl, after_ul, after_pg) = match after_st {
            Some((dl, ul, pg)) => (Some(dl), Some(ul), Some(pg)),
            None => (None, None, None),
        };

        diffs.push(build_f64_diff(
            "Débit Descendant (DL)",
            before.speedtest_dl_mbps,
            after_dl,
            "Mbps",
            false,
        ));
        diffs.push(build_f64_diff(
            "Débit Montant (UL)",
            before.speedtest_ul_mbps,
            after_ul,
            "Mbps",
            false,
        ));
        diffs.push(build_f64_diff(
            "Ping Speedtest",
            before.speedtest_ping_ms,
            after_pg,
            "ms",
            true,
        ));
    }

    for d in &diffs {
        runner.logger.info(format!(
            "{:<22} | Avant: {:<12} | Après: {:<12} | Delta: {}",
            d.metric, d.before, d.after, d.delta
        ));
    }

    runner
        .logger
        .progress(2, 2, "Comparaison réseau terminée.");
    diffs
}

fn run_ookla_speedtest(runner: &Runner) -> Option<(f64, f64, f64)> {
    runner
        .logger
        .info("Lancement de speedtest.exe (~20-30s)...");
    let out = runner.run_query(
        "speedtest.exe",
        &["--accept-license", "--accept-gdpr", "--format=json"],
    );
    if out.exit_code == -1 || out.stdout.trim().is_empty() {
        runner.logger.info(
            "speedtest.exe non disponible dans le PATH : test de débit ignoré.",
        );
        return None;
    }

    let json_str = extract_json_object(&out.stdout)?;
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let dl_bw = val.get("download")?.get("bandwidth")?.as_f64()?;
    let ul_bw = val.get("upload")?.get("bandwidth")?.as_f64()?;
    let ping_lat = val.get("ping")?.get("latency")?.as_f64()?;

    let dl_mbps = ((dl_bw * 8.0 / 1_000_000.0) * 10.0).round() / 10.0;
    let ul_mbps = ((ul_bw * 8.0 / 1_000_000.0) * 10.0).round() / 10.0;
    let ping_ms = (ping_lat * 10.0).round() / 10.0;

    runner.logger.success(format!(
        "Speedtest : DL {dl_mbps:.1} Mbps | UL {ul_mbps:.1} Mbps | Ping {ping_ms:.1} ms"
    ));
    Some((dl_mbps, ul_mbps, ping_ms))
}

fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end >= start {
        Some(&s[start..=end])
    } else {
        None
    }
}

fn extract_percentage(line: &str) -> Option<u32> {
    let pct_idx = line.find('%')?;
    let before = line[..pct_idx].trim_end();
    let digits: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse::<u32>().ok()
}

fn build_f64_diff(
    metric: &str,
    before: Option<f64>,
    after: Option<f64>,
    unit: &str,
    lower_is_better: bool,
) -> MetricDiff {
    let before_str = before
        .map(|v| format!("{v:.1} {unit}"))
        .unwrap_or_else(|| String::from("N/A"));
    let after_str = after
        .map(|v| format!("{v:.1} {unit}"))
        .unwrap_or_else(|| String::from("N/A"));

    match (before, after) {
        (Some(b), Some(a)) => {
            let diff = ((a - b) * 10.0).round() / 10.0;
            let improved = if diff.abs() < 0.05 {
                None
            } else if lower_is_better {
                Some(diff < 0.0)
            } else {
                Some(diff > 0.0)
            };
            MetricDiff {
                metric: metric.to_string(),
                before: before_str,
                after: after_str,
                delta: format!("{diff:+.1} {unit}"),
                improved,
            }
        }
        _ => MetricDiff {
            metric: metric.to_string(),
            before: before_str,
            after: after_str,
            delta: String::from("N/A"),
            improved: None,
        },
    }
}

fn build_u64_diff(
    metric: &str,
    before: Option<u64>,
    after: Option<u64>,
    unit: &str,
    lower_is_better: bool,
) -> MetricDiff {
    let before_str = before
        .map(|v| format!("{v} {unit}"))
        .unwrap_or_else(|| String::from("N/A"));
    let after_str = after
        .map(|v| format!("{v} {unit}"))
        .unwrap_or_else(|| String::from("N/A"));

    match (before, after) {
        (Some(b), Some(a)) => {
            let diff = a as i64 - b as i64;
            let improved = if diff == 0 {
                None
            } else if lower_is_better {
                Some(diff < 0)
            } else {
                Some(diff > 0)
            };
            MetricDiff {
                metric: metric.to_string(),
                before: before_str,
                after: after_str,
                delta: format!("{diff:+} {unit}"),
                improved,
            }
        }
        _ => MetricDiff {
            metric: metric.to_string(),
            before: before_str,
            after: after_str,
            delta: String::from("N/A"),
            improved: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_percentage() {
        assert_eq!(extract_percentage("    Signal                 : 88%"), Some(88));
        assert_eq!(extract_percentage("Signal : 100 %"), Some(100));
        assert_eq!(extract_percentage("SSID : MyBox"), None);
    }

    #[test]
    fn test_extract_json_object() {
        let raw = "WARNING: something\n{\"if_index\":12,\"adapter_name\":\"Ethernet\"}\n";
        assert_eq!(
            extract_json_object(raw),
            Some("{\"if_index\":12,\"adapter_name\":\"Ethernet\"}")
        );
    }
}
