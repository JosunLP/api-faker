# API Faker

Eine kleine Rust-Anwendung, die HTTP-Endpunkte aus einer JSON-Datei simuliert. Ideal, um Frontends oder Integrationen zu entwickeln, ohne auf echte Backends warten zu müssen.

## Features

- Beliebige HTTP-Methoden (GET, POST, PUT, PATCH, DELETE, ...)
- Frei definierbare Statuscodes, Header sowie JSON- oder Text-Bodies
- Optionale künstliche Verzögerungen (`delay_ms`), um Ladezustände zu testen
- Health-Check unter `GET /__health` und Auflistung aller konfigurierten Routen unter `GET /__routes`

## Konfigurationsdatei

Die Konfiguration liegt als JSON-Datei vor (Standard: `mock_endpoints.json`). Beispiel:

```json
{
  "routes": [
    {
      "method": "GET",
      "path": "/users",
      "description": "Returns a list of demo users.",
      "status": 200,
      "body": {
        "users": [
          { "id": 1, "name": "Ada" },
          { "id": 2, "name": "Linus" }
        ]
      }
    },
    {
      "method": "POST",
      "path": "/users",
      "description": "Pretend to create a new user.",
      "status": 201,
      "body": {
        "id": 999,
        "message": "User created"
      }
    },
    {
      "method": "GET",
      "path": "/reports/slow",
      "description": "Simulates a slow endpoint for testing loading states.",
      "status": 200,
      "delay_ms": 1200,
      "body": {
        "status": "still crunching numbers"
      }
    },
    {
      "method": "DELETE",
      "path": "/jobs/42",
      "description": "Returns a simple confirmation as plain text.",
      "status": 202,
      "text_body": "Job 42 scheduled for deletion"
    }
  ]
}
```

### Routenfelder

| Feld          | Typ               | Beschreibung                                        |
| ------------- | ----------------- | --------------------------------------------------- |
| `method`      | String            | HTTP-Methode (z. B. `GET`, `POST`, …)               |
| `path`        | String            | Vollständiger Pfad, der exakt gematcht wird         |
| `status`      | Zahl (optional)   | HTTP-Statuscode (Default `200`)                     |
| `headers`     | Objekt (optional) | Key-Value-Paare für zusätzliche Header              |
| `body`        | JSON (optional)   | Beliebiger JSON-Body                                |
| `text_body`   | String (optional) | Plain-Text-Antwort (z. B. für einfache Meldungen)   |
| `delay_ms`    | Zahl (optional)   | Verzögerung in Millisekunden vor dem Antworten      |
| `description` | String (optional) | Freitext-Beschreibung; erscheint in `GET /__routes` |

## Nutzung

### Voraussetzungen

- Rust 1.84+ (Edition 2024)

### Lokaler Start

```bash
cargo run -- --config mock_endpoints.json --host 0.0.0.0 --port 8080
```

Danach stehen alle konfigurierten Endpunkte unter `http://host:port` bereit.

### Eigene Konfiguration

1. Kopiere `mock_endpoints.json` und passe die Routen an.
2. Starte den Server mit dem neuen Pfad: `cargo run -- --config my_routes.json`.

## Tipps

- Mehrere Routen können denselben Pfad mit unterschiedlichen Methoden verwenden.
- Doppelte Kombinationen aus Methode + Pfad überschreiben sich; im Log erscheint ein Hinweis.
- Über `__routes` lässt sich schnell prüfen, welche Mocks aktuell aktiv sind.

> Hinweis: Pro Route kann entweder `body` **oder** `text_body` gesetzt werden. Bei Textantworten wird automatisch `text/plain; charset=utf-8` gesetzt, sofern kein eigener `Content-Type` angegeben ist.
