# Changelog

Alle relevanten Änderungen an diesem Projekt werden in dieser Datei dokumentiert.

Das Format basiert auf [Keep a Changelog](https://keepachangelog.com/de/1.1.0/)
und dieses Projekt folgt [Semantic Versioning](https://semver.org/lang/de/).

## [1.0.0] - 2026-03-06

### Hinzugefügt
- Initiales Release
- SSH Dummy-Server auf konfigurierbarem Port (Standard: 2242)
- Public-Key-Authentifizierung
- Trust On First Use (TOFU) Modus mit konfigurierbarem Timeout
- Geofencing mit Haversine-Distanzberechnung
- Geofence-Override bei erneutem Versuch innerhalb von 45 Sekunden
- Web UI (Ingress) für Key-Verwaltung
  - Vertrauenswürdige Keys anzeigen/widerrufen
  - Ausstehende Keys anzeigen/vertrauen/löschen
  - TOFU-Modus aktivieren/deaktivieren
  - Verbindungslogs
- Webhook-Integration für Garagentor-Steuerung
- AppArmor-Profil für Container-Sicherheit
- Input-Sanitization (XSS-Schutz)
- Automatische Bereinigung alter untrusted Keys
- File-Locking gegen Lost Updates
- CI/CD mit GitHub Actions
  - Automatischer Build für amd64, aarch64, armv7, i386
  - Nightly Dependency Updates
- JSON-Strukturierte Logs
- Vollständig in Rust geschrieben (memory-safe)
