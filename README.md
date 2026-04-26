# Garage SSH Gate

SSH-basierte Garagentor-Steuerung für Home Assistant. Nutzt SSH Public-Key-Authentifizierung mit Apple Shortcuts, um sicher dein Garagentor zu öffnen.

## Features

- 🔐 **SSH Public-Key Authentifizierung** - Sicherer als Passwörter
- 📱 **Apple Shortcuts Integration** - Ein Tap zum Öffnen
- 🗺️ **Geofencing** - Öffnet nur wenn du in der Nähe bist
- 🔑 **TOFU Modus** - Trust On First Use für einfache Einrichtung
- 🛡️ **AppArmor** - Container-Sicherheit
- 📋 **Strenge Logs** - Volle Nachverfolgbarkeit aller Zugriffsversuche
- 🌐 **Ingress Web UI** - Verwaltung direkt in Home Assistant
- 🦀 **100% Rust** - Memory-safe und performant
- 🔄 **Automatische Updates** - Nightly Dependency Updates via CI/CD

## Installation

### Voraussetzungen (Home Assistant)

- Home Assistant OS oder Supervised mit Add-on Store
- Unterstützte CPU-Architektur: `aarch64` (z. B. Raspberry Pi 4/5 64-bit) oder `amd64`
- Netzwerkzugriff auf Home Assistant über Port `2242/tcp` für den SSH-Shortcut

### Als Home Assistant Add-on

1. Gehe in Home Assistant zu **Settings → Add-ons → Add-on Store**.
1. Klicke oben rechts auf das **⋮ Menü** → **Repositories**.
1. Füge diese Repository-URL hinzu: `https://github.com/Max-42/garage-ssh`.
1. Klicke auf **Check for updates**.
1. Finde **Garage SSH Gate** in der Liste und installiere es.
1. Konfiguriere das Add-on (siehe unten).
1. Starte das Add-on.

### Schnellstart nach der Installation

1. Öffne den Tab **Configuration** des Add-ons.
1. Setze mindestens folgende Optionen:

- `webhook_url` auf deinen Home-Assistant Webhook/Automation-Endpunkt
- optional `home_latitude` und `home_longitude` für Geofencing

1. Klicke **Save** und dann **Start**.
1. Öffne **Logs** und prüfe, dass beide Dienste laufen.

- SSH-Server auf `0.0.0.0:2242`
- Web UI (Ingress) auf `0.0.0.0:8099`

1. Öffne das Add-on via **Open Web UI**.
1. Führe deinen Shortcut einmal aus und vertraue den neuen Key in der Web UI.

### Update des Add-ons

1. Gehe zu **Settings → Add-ons → Add-on Store**.
1. Klicke auf **Check for updates**.
1. Öffne **Garage SSH Gate** und klicke **Update**.
1. Nach dem Update: Add-on neu starten und kurz die Logs prüfen.

### Konfiguration

| Option | Beschreibung | Standard |
|--------|-------------|----------|
| `ssh_port` | SSH Server Port | `2242` |
| `webhook_url` | URL die bei erfolgreichem Zugang aufgerufen wird | - |
| `home_latitude` | Breitengrad deiner Garage | `0.0` |
| `home_longitude` | Längengrad deiner Garage | `0.0` |
| `geofence_radius_km` | Maximale Entfernung in km | `15` |
| `geofence_override_timeout_sec` | Sekunden für Geofence-Override | `45` |
| `tofu_timeout_sec` | TOFU-Modus Dauer in Sekunden | `45` |
| `untrusted_key_retention_days` | Aufbewahrung nicht-vertrauenswürdiger Keys (Tage) | `21` |
| `expected_json_version` | Erwartete JSON Version vom Client | `1.0.1` |
| `log_level` | Log-Level (trace/debug/info/warn/error) | `info` |
| `host_key_pem` | SSH Host Key (PEM) - auto-generiert, für Backup/Migration | (auto) |

## Apple Shortcut Einrichtung

### Schritt 1: SSH-Key generieren

Auf deinem iOS/iPadOS Gerät:

1. Öffne die **Shortcuts** App
2. Erstelle einen neuen Shortcut
3. Füge die Aktion **"Run Script Over SSH"** hinzu

Der Shortcut generiert automatisch einen SSH-Key beim ersten Mal.

### Schritt 2: Shortcut erstellen

Erstelle einen Shortcut mit folgenden Aktionen:

1. **"Ask for Input"** - Frage nach deinem Namen (nur beim ersten Mal)
2. **"Get Current Location"** - Aktuellen Standort holen
3. **"Get Device Details"** - Geräteinformationen sammeln
4. **"Dictionary"** - JSON-Payload erstellen:

```json
{
  "time": {
    "value": "[Current Date, ISO 8601]",
    "format": "ISO 8601"
  },
  "device": {
    "version": "[Device OS Version]",
    "model": "[Device Model]",
    "hostname": "[Device Hostname]",
    "name": "[Device Name]",
    "os": "[Device OS]",
    "build": "[Device Build]"
  },
  "position": {
    "longitude": [Current Location Longitude],
    "latitude": [Current Location Latitude],
    "altitude": [Current Location Altitude]
  },
  "version": "1.0.1"
}
```

5. **"Run Script Over SSH"**:
   - Host: `deine-homeassistant-ip`
   - Port: `2242`
   - Authentifizierung: **SSH Key**
   - Input: Das Dictionary von oben
   - Script: (leer lassen oder `cat`)

### Schritt 3: Key vertrauen

1. Führe den Shortcut einmal aus
2. Öffne die **Garage SSH Gate** Web UI in Home Assistant
3. Der neue Key erscheint unter "Ausstehende Keys"
4. Gib einen Namen und Gerätenamen ein
5. Klicke auf **"Vertrauen"**

**Alternativ (TOFU):**
1. Klicke in der Web UI auf **"TOFU aktivieren"**
2. Führe den Shortcut innerhalb von 45 Sekunden aus
3. Der Key wird automatisch vertraut

## Sicherheit

### Was ist sicher?
- **SSH Public Key** - Der private Schlüssel verlässt nie dein Gerät
- **Geofencing** - Verhindert versehentliches/unberechtigtes Öffnen aus der Ferne
- **AppArmor** - Container hat minimale Berechtigungen
- **Sanitized Inputs** - Alle Benutzereingaben werden gegen XSS bereinigt
- **File Locking** - Verhindert Race Conditions bei gleichzeitigen Zugriffen
- **Strenge Logs** - Jeder Zugriff wird protokolliert

### Was ist NICHT sicher / vertrauenswürdig?
- **Benutzername** - Kann vom Benutzer jederzeit geändert werden
- **Geräteinfo** - Kann gefälscht werden
- **Standortdaten** - Können trivial gespooft werden → **bieten keine echte Server-seitige Sicherheit**

> ⚠️ **Wichtig:** Die Geofence-Prüfung dient ausschließlich als Komfort-Feature für
> den Benutzer selbst (z.B. versehentliches Auslösen verhindern). Da die Positionsdaten
> vom Client kommen und der Shortcut jederzeit bearbeitet werden kann, bietet das
> Geofencing **keinen Schutz gegen absichtliches Spoofing**. Die einzige echte
> Authentifizierung ist der SSH Private Key.

### Geofence Override

Wenn du dich außerhalb des Geofence befindest, kannst du den Shortcut innerhalb von 45 Sekunden erneut ausführen, um das Tor trotzdem zu öffnen. Dies ist für Situationen gedacht, in denen der GPS-Standort ungenau ist.

## Android Support

Android-Geräte können sich auch verbinden. Da Android keine Apple Shortcuts unterstützt, kann das JSON-Payload optional sein. Der SSH-Key wird trotzdem gespeichert und kann manuell vertraut werden.

Empfohlene Android-Apps:
- **Tasker** mit SSH-Plugin
- **Termux** mit einem Bash-Script

## Webhook

Bei erfolgreichem Zugang wird ein POST-Request an die konfigurierte `webhook_url` gesendet:

```json
{
  "action": "open_garage"
}
```

Dies kann mit jeder Home Assistant Automation verbunden werden.

## Entwicklung

### Voraussetzungen

**Option A – NixOS / nix (empfohlen)**  
Das Repository enthält ein `flake.nix`, das automatisch eine vollständige
Entwicklungsumgebung mit Rust 1.85, Clippy, Rustfmt und OpenSSL bereitstellt.

```bash
# Vom Repo-Root aus, kein cd nötig:
nix develop --command cargo build --manifest-path garage_ssh_gate/src/Cargo.toml

# Schneller lokaler Release-Build (alle Kerne, kein LTO):
nix develop --command cargo build --profile release-local --manifest-path garage_ssh_gate/src/Cargo.toml

# Weitere nützliche Befehle:
nix develop --command cargo clippy --manifest-path garage_ssh_gate/src/Cargo.toml
nix develop --command cargo fmt   --manifest-path garage_ssh_gate/src/Cargo.toml
nix develop --command cargo test  --manifest-path garage_ssh_gate/src/Cargo.toml
```

Oder Dev-Shell einmalig betreten und dann ohne Prefix arbeiten:
```bash
nix develop          # Shell betreten
direnv allow         # alternativ: automatisch via direnv
```

> **Hinweis zur Build-Geschwindigkeit:**  
> `cargo build --release` verwendet `codegen-units = 1` (optimal für das Docker-Image,
> aber Single-Threaded). Für schnelle lokale Iteration:
> ```bash
> cargo build --profile release-local   # nutzt alle CPU-Kerne
> ```

**Option B – Rust direkt**  
- Rust 1.85+
- `pkg-config` und `openssl-dev` (oder `libssl-dev` auf Debian/Ubuntu)

```bash
cd garage_ssh_gate/src
cargo build --release
```

### Docker Image bauen
```bash
docker build -t garage-ssh-gate --build-arg BUILD_FROM=ghcr.io/home-assistant/amd64-base:latest garage_ssh_gate/
```

## Publish-Checkliste

Verwende diese Schritte, bevor du ein neues Add-on-Release veröffentlichst.

1. **Tests lokal ausführen**

```bash
# Rust Checks
nix develop --command cargo test  --manifest-path garage_ssh_gate/src/Cargo.toml
nix develop --command cargo clippy --manifest-path garage_ssh_gate/src/Cargo.toml -- -D warnings

# Container bauen + Smoke-Test
nix develop .#docker -c build-container garage-ssh-gate:local
nix develop .#docker -c test-container garage-ssh-gate:local

# Voller E2E-Test (Ports 2242/8099, TOFU, SSH Keys, Webhook)
nix develop .#docker -c bash tests/integration/e2e.sh --engine podman --image garage-ssh-gate:local
```

1. **Änderungen committen und pushen**

```bash
git add .
git commit -m "fix: release-ready changes"
git push
```

1. **Version-Tag erstellen (Produktiv-Release)**

```bash
git tag v1.0.1
git push origin v1.0.1
```

1. **CI Ergebnis prüfen**

- Workflow `Build and Publish` muss grün sein.
- Multi-Arch Images werden veröffentlicht:
  - `ghcr.io/max-42/garage-ssh/amd64-garage-ssh-gate:<version>`
  - `ghcr.io/max-42/garage-ssh/aarch64-garage-ssh-gate:<version>`
  - `ghcr.io/max-42/garage-ssh/garage-ssh-gate:<version>`
  - `ghcr.io/max-42/garage-ssh/garage-ssh-gate:latest`

1. **Home Assistant aktualisieren**

- Add-on Store → Repository aktualisieren → Add-on updaten.
- Danach Logs prüfen: SSH und Web UI starten ohne Fehler.

### Automatische Monats-Releases

Zusätzlich zu normalen Push/Tag-Builds läuft CI automatisch monatlich (1. Tag des Monats) und veröffentlicht ein neues Image (`-monthly.<YYYYMM>`), damit Abhängigkeiten aktuell bleiben.

## Lizenz

MIT License
