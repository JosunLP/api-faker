# API Faker

Eine kleine Rust-Anwendung, die HTTP-Endpunkte aus einer JSON-Datei simuliert. Ideal, um Frontends oder Integrationen zu entwickeln, ohne auf echte Backends warten zu müssen.

## Features

- Beliebige HTTP-Methoden (GET, POST, PUT, PATCH, DELETE, ...)
- Frei definierbare Statuscodes, Header sowie JSON- oder Text-Bodies
- Routen, die optional auf bestimmte Query-Parameter reagieren – inklusive Varianten für unterschiedliche Werte
- Optionale künstliche Verzögerungen (`delay_ms`), um Ladezustände zu testen
- Health-Check unter `GET /__health` und Auflistung aller konfigurierten Routen unter `GET /__routes`
- Fehlervarianten, die sich gezielt über `?__error=name` oder den Header `x-api-faker-error: name` erzwingen lassen
- Automatisch generierte OpenAPI-Dokumentation unter `GET /__openapi.json` sowie ein fertiges Swagger UI unter `GET /__swagger`

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

| Feld             | Typ               | Beschreibung                                                              |
| ---------------- | ----------------- | ------------------------------------------------------------------------- |
| `method`         | String            | HTTP-Methode (z. B. `GET`, `POST`, …)                                     |
| `path`           | String            | Vollständiger Pfad, der exakt gematcht wird                               |
| `status`         | Zahl (optional)   | HTTP-Statuscode (Default `200`)                                           |
| `headers`        | Objekt (optional) | Key-Value-Paare für zusätzliche Header                                    |
| `body`           | JSON (optional)   | Beliebiger JSON-Body                                                      |
| `text_body`      | String (optional) | Plain-Text-Antwort (z. B. für einfache Meldungen)                         |
| `query`          | Objekt (optional) | Key-Value-Paare, die als Query-Parameter verlangt sind                    |
| `delay_ms`       | Zahl (optional)   | Verzögerung in Millisekunden vor dem Antworten                            |
| `variants`       | Array (optional)  | Liste von Varianten mit eigenen Overrides                                 |
| `error_variants` | Array (optional)  | Benannte Fehlervarianten, die über `__error` oder Header aktiviert werden |
| `description`    | String (optional) | Freitext-Beschreibung; erscheint in `GET /__routes`                       |

Jede normale Variante kann diese Felder überschreiben (alle optional, sonst erbt sie den Wert der Hauptroute): `query`, `headers`, `body`, `text_body`, `status`, `delay_ms`, `description`.

### Fehlervarianten

Mit `error_variants` lassen sich gezielt Fehlerfälle triggern, ohne die realen Query-Parameter der Route zu verändern. Jede Fehlervariante benötigt ein eindeutiges `name`-Feld und kann optional dieselben Felder wie die Hauptroute überschreiben (`headers`, `body`, `text_body`, `status`, `delay_ms`, `description`).

Aktivierungsmöglichkeiten:

- Query-Parameter `?__error=<name>`
- HTTP-Header `x-api-faker-error: <name>`

Passt der Name, hat die Fehlervariante Vorrang vor allen anderen Varianten. In `GET /__routes` taucht sie mit ihrem `error_trigger` auf, damit ersichtlich bleibt, wie sie ausgelöst wird.

## Nutzung

### Voraussetzungen

- Rust 1.84+ (Edition 2024)

### Lokaler Start

```bash
cargo run -- --config mock_endpoints.json --host 0.0.0.0 --port 8080
```

Danach stehen alle konfigurierten Endpunkte unter `http://host:port` bereit.

### Eigene Konfiguration

1. Kopiere `mock_endpoints.example.json` in `mock_endpoints.json` und passe die Routen an.
2. Starte den Server default mit `cargo run` oder mit dem Pfad zu einer alternativen config: `cargo run -- --config my_routes.json`.

### Swagger & OpenAPI

- `GET /__openapi.json` liefert jederzeit die aktuell aus der Konfiguration generierte OpenAPI-3.1-Datei (inkl. Vendor-Extension `x-api-faker` mit Varianten/Fehlern).
- `GET /__swagger` stellt ein eingebautes Swagger UI bereit, das automatisch auf diese Datei verweist – praktischerweise ohne weitere Tools oder Konfiguration.
- Beide Endpunkte aktualisieren sich unmittelbar, sobald du die JSON-Konfiguration änderst und den Server erneut startest.

## CI/CD

| Workflow | Datei                           | Trigger                                | Zweck                                                                                                                                                                                            |
| -------- | ------------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Tests    | `.github/workflows/test.yml`    | `push`/`pull_request` auf `main`       | Führt `cargo fmt --check`, `cargo clippy` (mit `-D warnings`) und `cargo test` aus. Nutzt `dtolnay/rust-toolchain@stable` und `Swatinem/rust-cache@v2`.                                          |
| Build    | `.github/workflows/build.yml`   | `push` auf `main`, `workflow_dispatch` | Erstellt ein Release-Binary (`cargo build --release --locked`) für Linux, verpackt es als `api-faker-linux-x86_64.tar.gz` und lädt es als Artefakt hoch.                                         |
| Release  | `.github/workflows/release.yml` | Tags `v*`, `workflow_dispatch`         | Baut Release-Artefakte für Linux, macOS und Windows, lädt sie als Artefakte hoch und veröffentlicht sie automatisch über `softprops/action-gh-release` inklusive auto-generierter Release Notes. |

Alle Workflows geben ihre Logs als Artefakte aus und nutzen die GitHub Actions Cache-Mechanik, damit Folge-Läufe schneller durchlaufen.

## Tipps

- Mehrere Routen können denselben Pfad mit unterschiedlichen Methoden verwenden.
- Doppelte Kombinationen aus Methode + Pfad überschreiben sich; im Log erscheint ein Hinweis.
- Über `__routes` lässt sich schnell prüfen, welche Mocks aktuell aktiv sind.

> Hinweise:
>
> - Pro Route kann entweder `body` **oder** `text_body` gesetzt werden. Bei Textantworten wird automatisch `text/plain; charset=utf-8` gesetzt, sofern kein eigener `Content-Type` angegeben ist.
> - Wenn mehrere Routen denselben Pfad besitzen, werden zuerst diejenigen mit passenden `query`-Parametern geprüft, bevor eine generische Route greift.
> - Innerhalb einer Route werden `variants` mit passenden Query-Parametern geprüft; ohne Treffer fällt der Server auf die Hauptroute zurück.
