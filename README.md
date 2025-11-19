# API Faker

Eine kleine Rust-Anwendung, die HTTP-Endpunkte aus einer JSON-Datei simuliert. Ideal, um Frontends oder Integrationen zu entwickeln, ohne auf echte Backends warten zu müssen.

## Features

- Beliebige HTTP-Methoden (GET, POST, PUT, PATCH, DELETE, ...)
- Frei definierbare Statuscodes, Header sowie JSON- oder Text-Bodies
- Routen, die optional auf bestimmte Query-Parameter reagieren – inklusive Varianten für unterschiedliche Werte
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
      },
      "variants": [
        {
          "description": "Filtered list for ?team=platform&active=true",
          "query": {
            "team": "platform",
            "active": "true"
          },
          "body": {
            "users": [
              { "id": 42, "name": "Grace" }
            ]
          }
        },
        {
          "description": "Empty list for ?team=support",
          "query": {
            "team": "support"
          },
          "body": {
            "users": []
          }
        },
        {
          "description": "Error when ?team=ops",
          "query": {
            "team": "ops"
          },
          "status": 503,
          "text_body": "team backend currently unavailable"
        }
      ]
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

| Feld          | Typ               | Beschreibung                                           |
| ------------- | ----------------- | ------------------------------------------------------ |
| `method`      | String            | HTTP-Methode (z. B. `GET`, `POST`, …)                  |
| `path`        | String            | Vollständiger Pfad, der exakt gematcht wird            |
| `status`      | Zahl (optional)   | HTTP-Statuscode (Default `200`)                        |
| `headers`     | Objekt (optional) | Key-Value-Paare für zusätzliche Header                 |
| `body`        | JSON (optional)   | Beliebiger JSON-Body                                   |
| `text_body`   | String (optional) | Plain-Text-Antwort (z. B. für einfache Meldungen)      |
| `query`       | Objekt (optional) | Key-Value-Paare, die als Query-Parameter verlangt sind |
| `delay_ms`    | Zahl (optional)   | Verzögerung in Millisekunden vor dem Antworten         |
| `variants`    | Array (optional)  | Liste von Varianten mit eigenen Overrides              |
| `description` | String (optional) | Freitext-Beschreibung; erscheint in `GET /__routes`    |

Jede Variante kann diese Felder überschreiben (alle optional, sonst erbt sie den Wert der Hauptroute): `query`, `headers`, `body`, `text_body`, `status`, `delay_ms`, `description`.

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

> Hinweise:
>
> - Pro Route kann entweder `body` **oder** `text_body` gesetzt werden. Bei Textantworten wird automatisch `text/plain; charset=utf-8` gesetzt, sofern kein eigener `Content-Type` angegeben ist.
> - Wenn mehrere Routen denselben Pfad besitzen, werden zuerst diejenigen mit passenden `query`-Parametern geprüft, bevor eine generische Route greift.
> - Innerhalb einer Route werden `variants` mit passenden Query-Parametern geprüft; ohne Treffer fällt der Server auf die Hauptroute zurück.
