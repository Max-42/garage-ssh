# Garage SSH Gate - Dokumentation

## Übersicht

Garage SSH Gate ist ein Home Assistant Add-on, das einen Dummy-SSH-Server bereitstellt, um dein Garagentor über Apple Shortcuts sicher zu steuern.

## Funktionsweise

1. Ein Apple Shortcut sammelt Standort, Gerätedaten und die aktuelle Zeit
2. Diese Daten werden als JSON über SSH an den Server gesendet
3. Der Server prüft:
   - Ist der SSH-Key vertrauenswürdig?
   - Ist der Client innerhalb des Geofence?
   - Ist das JSON valide und die Version korrekt?
4. Bei Erfolg wird ein Webhook ausgelöst, der das Garagentor öffnet

## Konfiguration

Die Konfiguration erfolgt über die Home Assistant Add-on-Seite.

### Webhook URL

Setze die `webhook_url` auf eine Home Assistant Webhook-URL oder eine andere URL, die dein Garagentor steuert.

Beispiel für Home Assistant Webhook:
```
http://homeassistant.local:8123/api/webhook/mein-garagen-webhook
```

### Standort

Setze `home_latitude` und `home_longitude` auf die Koordinaten deiner Garage. Der `geofence_radius_km` bestimmt, wie nah du sein musst.

### Key-Verwaltung

Öffne die Web UI über den "Garage SSH" Eintrag im Home Assistant Seitenmenü.

- **Ausstehende Keys**: Hier erscheinen alle SSH-Keys, die sich verbunden haben, aber noch nicht vertraut werden
- **Vertrauenswürdige Keys**: Liste aller akzeptierten Keys
- **TOFU Modus**: Vertraue automatisch alle neuen Keys für 45 Sekunden
- **Logs**: Detailliertes Protokoll aller Verbindungsversuche

## Fehlerbehebung

### "Key not trusted"
Dein SSH-Key ist noch nicht als vertrauenswürdig markiert. Öffne die Web UI und vertraue den Key manuell oder aktiviere den TOFU-Modus.

### "Version mismatch"
Die JSON-Version in deinem Shortcut stimmt nicht mit der erwarteten Version überein. Aktualisiere den Shortcut.

### "Outside geofence"
Du bist zu weit von der Garage entfernt. Wenn das GPS ungenau ist, führe den Shortcut innerhalb von 45 Sekunden erneut aus.

## Sicherheit

Dieses Add-on nutzt:
- SSH Public-Key-Authentifizierung (einzige echte Sicherheitsschicht)
- AppArmor Profil
- Input-Sanitization gegen XSS
- File-Locking gegen Lost Updates
- Strenge Zugriffs-Logs

**Hinweis zum Geofencing:** Die Standortprüfung ist ein reines Komfort-Feature
zum Schutz vor versehentlichem Auslösen. Da die Position vom Client gesendet
wird, kann sie trivial gefälscht werden und bietet keine echte Server-seitige
Sicherheit. Der SSH-Key ist die einzige vertrauenswürdige Authentifizierung.
