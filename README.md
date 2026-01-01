# API Faker

API Faker is a lightweight Rust application that serves HTTP endpoints from a JSON configuration file. It is ideal for frontend or integration work whenever the real backend is still under construction.

## Features

- Arbitrary HTTP methods (GET, POST, PUT, PATCH, DELETE, …)
- Fully customizable status codes, headers, JSON bodies, and text bodies
- Routes can react to specific query parameters, including dedicated variants for different values
- Optional artificial delays (`delay_ms`) for simulating loading states
- Built-in health check at `GET /__health` and a complete route overview at `GET /__routes`
- Error variants triggered via `?__error=name` or the `x-api-faker-error: name` header
- Auto-generated OpenAPI document at `GET /__openapi.json` plus an embedded Swagger UI at `GET /__swagger`
- Permissive CORS configuration so local browsers can call the mock API without extra setup
- Host and port can be configured via CLI flags or directly inside the JSON file
- Request and response flags help you toggle variants and label responses without touching headers manually
- Built-in placeholder responses for `/robots.txt` and `/favicon.ico` keep browser requests from cluttering the logs

## Configuration file

The configuration lives in a JSON file (default: `mock_endpoints.json`). Example:

```json
{
  "server": {
    "host": "0.0.0.0",
    "port": 8080
  },
  "response_flags": ["example-env"],
  "routes": [
    {
      "method": "GET",
      "path": "/users",
      "description": "Returns a list of demo users.",
      "status": 200,
      "response_flags": ["users-base"],
      "body": {
        "users": [
          { "id": 1, "name": "Ada" },
          { "id": 2, "name": "Linus" }
        ]
      },
      "variants": [
        {
          "description": "Filtered list when ?team=platform&active=true",
          "query": {
            "team": "platform",
            "active": "true"
          },
          "response_flags": ["users-filtered"],
          "body": {
            "users": [{ "id": 42, "name": "Grace" }]
          }
        },
        {
          "description": "Empty list for ?team=support",
          "query": {
            "team": "support"
          },
          "request_flags": ["beta-list"],
          "response_flags": ["users-beta"],
          "body": {
            "users": []
          }
        }
      ],
      "error_variants": [
        {
          "name": "users-timeout",
          "description": "Force a temporary outage via ?__error=users-timeout or header x-api-faker-error",
          "status": 503,
          "delay_ms": 1500,
          "text_body": "upstream dependency unavailable"
        }
      ]
    }
  ]
}
```

### Server settings

The optional `server` block controls which interface and port the mock server binds to:

| Field  | Type   | Description                                            |
| ------ | ------ | ------------------------------------------------------ |
| `host` | String | Hostname or IP address, e.g., `127.0.0.1` or `0.0.0.0` |
| `port` | Number | TCP port (for example `8080`)                          |

CLI flags (`--host`, `-p/--port`) always win. If you omit them, API Faker reads the values from the configuration file and ultimately falls back to `127.0.0.1:8080`.

### Route fields

| Field            | Type              | Description                                                          |
| ---------------- | ----------------- | -------------------------------------------------------------------- |
| `method`         | String            | HTTP method (e.g., `GET`, `POST`, …)                                 |
| `path`           | String            | Path segment that must match exactly                                 |
| `status`         | Number (optional) | HTTP status code (default `200`)                                     |
| `headers`        | Object (optional) | Additional headers (name → value)                                    |
| `body`           | JSON (optional)   | Arbitrary JSON response body                                         |
| `text_body`      | String (optional) | Plain-text response body                                             |
| `query`          | Object (optional) | Required query parameters                                            |
| `request_flags`  | Array (optional)  | Flags the request must include (`__flags` parameter or header)       |
| `response_flags` | Array (optional)  | Flags automatically added to the `x-api-faker-flags` response header |
| `delay_ms`       | Number (optional) | Artificial delay in milliseconds before sending the response         |
| `variants`       | Array (optional)  | Variant list with overrides                                          |
| `error_variants` | Array (optional)  | Named error variants triggered via `__error` or header               |
| `description`    | String (optional) | Free-form description that also appears in `GET /__routes`           |

Each variant can override the same fields (all optional). Non-specified values fall back to the base route.

### Flags

Flags can be configured globally (`response_flags` at the root), per route, and per variant or error variant.

1. **Request flags** determine whether a route or variant matches. A request can send flags via the `__flags` query parameter (repeat it or comma-separate values) or via the `x-api-faker-flags` header. Every expected flag must be present.
2. **Response flags** are automatically appended to the `x-api-faker-flags` header of the response. Values from global configuration, the route, and the current variant are merged (duplicates removed, order preserved).

This makes it easy to simulate feature toggles: send `x-api-faker-flags: beta-users`, return a different payload, and expose the active flags in the response headers.

### Error variants

`error_variants` let you simulate failure scenarios without altering query parameters. Each error variant needs a unique `name` and can optionally override the same fields as the base route (`headers`, `body`, `text_body`, `status`, `delay_ms`, `description`).

Trigger options:

- Query parameter `?__error=<name>`
- HTTP header `x-api-faker-error: <name>`

  When the name matches, the error variant takes precedence over every other variant. The `GET /__routes` endpoint lists each error trigger so you can see how to activate it.

## Usage

### Installation

#### Quick Install (Recommended)

Download and run the installation script:

```bash
curl -fsSL https://raw.githubusercontent.com/JosunLP/api-faker/main/install.sh | sh
```

> **Security note**: Only run install scripts from sources you trust. If you're unsure, use the download-and-inspect method below before executing the script.

Or download and inspect first:

```bash
wget https://raw.githubusercontent.com/JosunLP/api-faker/main/install.sh
chmod +x install.sh
./install.sh
```

The script automatically:

- Detects your platform (Linux, macOS, Windows/WSL)
- Downloads the latest release
- Verifies the checksum
- Installs to `/usr/local/bin` (or custom location via `INSTALL_DIR`)

**macOS users:** The binary is not code-signed. On first run, you may need to allow it in System Preferences > Security & Privacy, or run:

```bash
xattr -d com.apple.quarantine /usr/local/bin/api-faker
```

#### Package Managers

**Homebrew (macOS/Linux):**

```bash
brew install josunlp/tap/api-faker
```

**Winget (Windows):**

```powershell
winget install JosunLP.APIFaker
```

**APT (Debian/Ubuntu):**

```bash
# Add repository
echo "deb [trusted=yes] https://josunlp.github.io/api-faker stable main" | sudo tee /etc/apt/sources.list.d/api-faker.list

# Install
sudo apt update
sudo apt install api-faker
```

Or download the `.deb` package directly from the [releases page](https://github.com/JosunLP/api-faker/releases):

```bash
# Replace VERSION with the actual version number from the releases page
curl -LO https://github.com/JosunLP/api-faker/releases/latest/download/api-faker_VERSION_amd64.deb
sudo dpkg -i api-faker_VERSION_amd64.deb
```

#### Manual Installation

Download the appropriate binary for your platform from the [releases page](https://github.com/JosunLP/api-faker/releases):

- **Linux**: `api-faker-linux-x86_64.tar.gz`
- **macOS**: `api-faker-macos-x86_64.tar.gz`
- **Windows**: `api-faker-windows-x86_64.zip`

Extract and place the binary in your PATH.

#### Build from Source

```bash
git clone https://github.com/JosunLP/api-faker.git
cd api-faker
cargo build --release
```

The binary will be in `target/release/api-faker`.

### Requirements

- For binary installation: No dependencies required
- For building from source: Rust 1.84+ (Edition 2024)

### Updates

API Faker includes built-in update functionality:

```bash
# Check for updates
api-faker --check

# Update to the latest version
api-faker --update
```

The updater:

- Fetches the latest release from GitHub
- Verifies checksums for security
- Automatically replaces the binary

### Local start

```bash
cargo run -- --config mock_endpoints.json --host 0.0.0.0 --port 8080
```

Or with the installed binary:

```bash
api-faker --config mock_endpoints.json --host 0.0.0.0 --port 8080
```

All configured endpoints will then be available under `http://host:port`.

### CLI flags & environment variables

| Flag / Env                         | Description                                                                                             |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `--config`, `API_FAKER_CONFIG`     | Path to the JSON file with your routes (defaults to `mock_endpoints.json`).                             |
| `--host`, `API_FAKER_HOST`         | Override the bind host (`127.0.0.1` fallback).                                                          |
| `-p/--port`, `API_FAKER_PORT`      | Preferred port (`8080` fallback). If the port is occupied, API Faker auto-increments until it succeeds. |
| `--port-max`, `API_FAKER_PORT_MAX` | Highest port that the fallback logic may probe (default `65535`).                                       |
| `--log-level`, `API_FAKER_LOG`     | Override tracing verbosity for both API Faker and Axum (`error`, `warn`, `info`, `debug`, `trace`).     |
| `--dry-run`                        | Validate the configuration file and exit without binding a socket.                                      |
| `--check`                          | Check for updates without installing.                                                                   |
| `--update`                         | Update to the latest version.                                                                           |

Example: `api-faker --config qa.json --host 0.0.0.0 --port 5000 --port-max 5100 --log-level debug` will bind to the first free port in `[5000, 5100]` and emit verbose diagnostics. Pair `--dry-run` with CI to ensure configuration changes stay valid without spinning up the server.

### Custom configuration

1. Copy `mock_endpoints.example.json` to `mock_endpoints.json` and adjust the routes.
2. Start the server with `cargo run` (default config) or point to a different file via `cargo run -- --config my_routes.json`.

### Swagger & OpenAPI

- `GET /__openapi.json` always returns the generated OpenAPI 3.1 document (including the `x-api-faker` vendor extension with variants/errors/flags).
- `GET /__swagger` serves an embedded Swagger UI that automatically points to that document.
- Both endpoints refresh instantly whenever you restart the server with a new JSON configuration.

## CI/CD

| Workflow | File                            | Trigger                               | Purpose                                                                                                                                                |
| -------- | ------------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Tests    | `.github/workflows/test.yml`    | `push` / `pull_request` on `main`     | Runs `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test`. Uses `dtolnay/rust-toolchain@stable` plus `Swatinem/rust-cache@v2`.            |
| Build    | `.github/workflows/build.yml`   | `push` on `main`, `workflow_dispatch` | Produces a release binary (`cargo build --release --locked`) for Linux, packages it as `api-faker-linux-x86_64.tar.gz`, and uploads it as an artifact. |
| Release  | `.github/workflows/release.yml` | Tags `v*`, `workflow_dispatch`        | Builds release artifacts for Linux, macOS, and Windows, uploads them, and publishes via `softprops/action-gh-release` with auto-generated notes.       |

All workflows store their logs as artifacts and tap into the GitHub Actions cache for faster follow-up runs.

## Tips

- You can define multiple routes with the same path as long as the method differs.
- Duplicate method+path combinations overwrite the previous route; a warning is logged.
- `GET /__routes` quickly shows which mocks are currently active, including query expectations, flags, and error triggers.

  > Notes:
  >
  > - Each route may provide either `body` **or** `text_body`. When serving text, the default `Content-Type` is `text/plain; charset=utf-8` unless you override it.
  > - When multiple routes share a path, the ones with matching `query` constraints are evaluated first before falling back to a generic route.
  > - Within a route, `variants` are checked for matching query parameters and request flags; if none apply, the server falls back to the base route.
