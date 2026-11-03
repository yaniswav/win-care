# 🛡 WinCare — Réparation, Optimisation & Debloat Windows (10/11)

<p align="center">
  <img src="assets/icon.png" width="110" alt="WinCare Icon" />
</p>

**WinCare** est un utilitaire natif ultra-léger (~3,5 Mo, écrit en **Rust** avec `egui`/`eframe`) qui regroupe dans un seul exécutable autonome (`.exe`) :

1. **🛠 Réparation Windows (Interactif & Mode Auto)** :
   - **DISM** (`CheckHealth`, `ScanHealth`, `RestoreHealth`, `/StartComponentCleanup /ResetBase`) avec progression en temps réel.
   - **SFC** (`sfc /scannow` avec décodage natif UTF-16LE).
   - **Windows Update** (arrêt propre des services, sauvegarde de `SoftwareDistribution` et `catroot2`, réenregistrement des 36 DLLs, purge des stratégies bloquantes).
   - **Reset Réseau** (`ipconfig /flushdns`, `Winsock`, `TCP/IP`, `WinHTTP Proxy`, `Pare-feu`).
   - **Nettoyage Fichiers Temporaires** (`%TEMP%`, `C:\Windows\Temp`, `Prefetch`, `INetCache` et `cleanmgr`).
   - **Windows Store & AppX** (`wsreset.exe` + réenregistrement des manifestes AppX).
   - **Services Critiques** (`wuauserv`, `BITS`, `CryptSvc`, `TrustedInstaller`, `Winmgmt`, `EventLog`, `Dhcp`, `Dnscache`).
   - **CHKDSK** (vérification rapide + planification optionnelle au redémarrage).

2. **⚡ Boost PC, Analyseur `%TEMP%` & Démarrage** :
   - Diagnostic instantané (CPU, RAM, Uptime, Disques SSD/HDD, Top processus RAM, Services Auto, taille des 9 dossiers de caches).
   - **Analyseur des gros dossiers `%TEMP%`** : identifie le Top 15 des sous-dossiers les plus lourds et permet de les purger en 1 clic.
   - **Gestionnaire des applications au démarrage** : bascule `[ON] / [OFF]` en temps réel via les clés binaires `StartupApproved` (100 % compatible avec le Gestionnaire des tâches Windows).
   - **Point de Restauration Windows** en 1 clic (`Checkpoint-Computer`).
   - Comparatif **Avant / Après** automatique.

3. **🌐 Boost Internet, Benchmark DNS & MTU** :
   - Détection automatique de la carte active, passerelle, signal Wi-Fi, ping et débit instantané.
   - **Benchmark DNS en millisecondes** (NextDNS, Cloudflare, Google, Quad9, OpenDNS) avec **protection automatique de NextDNS** s'il est actif.
   - Détection et application du **MTU optimal** sans fragmentation (`Don't Fragment`).

4. **🧹 Debloat & Tweaks Windows 11** :
   - Désactivation en 1 clic de **Copilot**, **Windows Recall**, **Click to Do**, **Télémétrie**, **Bing dans la recherche**, **Widgets**, **Publicités Windows 11**.
   - Activation du **Menu clic-droit classique complet (Windows 10)**, du bouton **Fin de tâche** dans la barre des tâches et de l'affichage des extensions de fichiers.

5. **🔄 Mise à jour intégrée (Auto-Updater GitHub)** :
   - Bouton **« Vérifier MAJ »** directement dans l'interface : télécharge la dernière version depuis GitHub Releases et met à jour `WinCare.exe` automatiquement.

---

## 🚀 Téléchargement & Utilisation

### Option 1 — Exécutable direct (pour les amis / utilisateurs)
1. Téléchargez **`WinCare.exe`** depuis la page [**Releases**](https://github.com/yaniswav/win-care/releases/latest).
2. Double-cliquez sur `WinCare.exe` (l'élévation Administrateur UAC est demandée automatiquement).
3. Pour mettre à jour plus tard, cliquez simplement sur **« 🔄 Vérifier MAJ »** en haut à droite de l'application !

### Option 2 — Options en ligne de commande (CLI)
```powershell
.\WinCare.exe --diagnose          # Diagnostic complet (PC + Internet) en lecture seule
.\WinCare.exe --auto              # Lance toutes les réparations Windows automatiquement
.\WinCare.exe --boost-pc          # Lance le nettoyage et les optimisations safe Boost-PC
.\WinCare.exe --boost-net         # Lance les optimisations safe Boost-Internet
.\WinCare.exe --update            # Vérifie et installe la dernière mise à jour depuis GitHub
.\WinCare.exe --dry-run           # Mode simulation (aucune modification appliquée au système)
```

---

## 🛠 Compilation depuis les sources

Prérequis : [Rust (stable MSVC)](https://rustup.rs/) sous Windows 10/11.

```powershell
git clone https://github.com/yaniswav/win-care.git
cd win-care
cargo build --release
```
L'exécutable optimisé (~3,5 Mo) est généré dans `target\release\win-care.exe`.
