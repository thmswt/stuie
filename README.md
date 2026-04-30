# STUIE - Storage Terminal UI Explorer

A modern, interactive TUI application for managing cloud object storage from your terminal. Browse buckets, upload/download files, and manage objects across multiple cloud storage providers with a keyboard-driven interface.

[![CI](https://img.shields.io/github/actions/workflow/status/thmswt/stuie/ci.yml?branch=main&logo=github&label=CI)](https://github.com/thmswt/stuie/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Rust-2026_edition-orange)
![License](https://img.shields.io/badge/License-MIT-blue)

## Features

- **Multi-provider support** - Connect to AWS S3, MinIO (and other S3-compatible backends), and Google Cloud Storage simultaneously
- **Interactive file browser** - Navigate buckets and objects with a multi-pane TUI layout (sidebar, file list, preview)
- **Upload & download** - Transfer files with real-time progress tracking
- **File operations** - Create folders, delete objects (batch), search/filter by filename
- **Read-Only Mode** - Per-provider safety switch that hard-blocks every mutating operation (see below)
- **GCS multi-project support** - Browse buckets across multiple GCP projects in a single provider via `gcloud` ADC
- **Modern UI** - Color-coded file type icons (Nerd Fonts), mouse support, clipboard integration
- **Configuration management** - In-app config editor for adding/editing storage providers

## Prerequisites

- Rust 1.85+ and Cargo
- A Nerd Font installed in your terminal (for file icons)
- Access to an S3-compatible storage provider and/or a GCS account

## Installation

```bash
git clone https://github.com/thmswt/stuie.git
cd stuie
cargo build --release
```

The binary will be at `./target/release/stuie`.

## Usage

```bash
cargo run
# or
./target/release/stuie
```

On first launch, press `c` to open the configuration modal and add a storage provider.

### Configuration

STUIE stores its configuration at `~/.config/stuie/config.json`. Example with one S3 provider and one read-only GCS provider:

```json
{
  "providers": [
    {
      "name": "My S3",
      "provider_type": "S3",
      "region": "us-east-1",
      "endpoint": "https://s3.amazonaws.com",
      "access_key_id": "YOUR_ACCESS_KEY",
      "secret_access_key": "YOUR_SECRET_KEY",
      "allow_http": false,
      "enabled": true,
      "read_only": false
    },
    {
      "name": "Prod GCS",
      "provider_type": "GCS",
      "gcs_auth_mode": "ServiceAccount",
      "credentials_path": "/path/to/service-account.json",
      "project": "my-gcp-project",
      "enabled": true,
      "read_only": true
    }
  ]
}
```

GCS supports two authentication modes:

- **`ServiceAccount`** - point `credentials_path` at a service-account JSON key (or set `GOOGLE_APPLICATION_CREDENTIALS`).
- **`GcloudAdc`** - reuse the credentials from `gcloud auth application-default login`. Multi-project mode lets you list buckets across several projects.

### Read-Only Mode

Each provider has a `read_only` flag (toggleable from the config UI on the *Read-Only Mode* row, or editable directly in `config.json`). When set:

- The storage backend is wrapped in a `ReadOnlyStorage` decorator that returns an error for `upload`, `create_folder`, and `delete_objects` — guaranteeing no destructive call slips through, regardless of UI path.
- The destructive keybindings (`n`, `u`, `x`/`Delete`) short-circuit with an inline error so you don't get a misleading prompt before the failure.
- The provider's sidebar row is badged `[RO]` in the warning colour.
- **GCS service-account mode additionally restricts the access token's OAuth scope to `devstorage.read_only`.** This applies to both our JSON-API calls and the bearer handed to `object_store` via a custom `CredentialProvider`, so even a hypothetical bug bypassing the wrapper would be rejected by GCS itself with HTTP 403.

The intent is defense-in-depth for production buckets: the wrapper is the always-on guard, and credential-side scope restriction is the belt-and-braces second layer where the provider supports it.

### Keyboard Shortcuts

| Key | Action |
|---|---|
| `q` | Quit |
| `?` | Toggle help |
| `c` | Configuration modal |
| `/` | Search / filter objects |
| `Enter` | Select / open folder |
| `Backspace` | Go back |
| `Tab` | Switch panes |
| `j` / `k` / `↑` / `↓` | Navigate |
| `←` (sidebar) | Jump to parent (bucket → project → provider) |
| `Space` | Multi-select |
| `u` | Upload file |
| `d` | Download selected |
| `n` | New folder |
| `x` / `Delete` | Delete selected |
| `y` | Copy path to clipboard |
| `p` | Toggle preview pane |

## Testing

Unit tests run without any external dependencies:

```bash
cargo test --test config_tests --test test_read_only_storage --test test_gcs_scope
```

The S3 integration tests use a local MinIO instance via Docker:

```bash
./tests/minio_setup.sh    # Start MinIO + seed test data
cargo test
./tests/minio_teardown.sh # Cleanup
```

## Pending Features

These are planned but not yet implemented:

- **Azure Blob Storage** - the provider type exists in config and shows up in the UI cycle, but the backend is a stub. Adding/enabling an Azure provider currently has no effect at runtime.
- **GCS GcloudAdc read-only scope restriction** - service-account credentials get full credential-side enforcement in RO mode, but the gcloud-ADC refresh-token's scopes are baked in at `gcloud auth application-default login` time and can't be shrunk at request. Today this falls back to the wrapper-only guard. Workaround: re-authenticate gcloud with `--scopes=https://www.googleapis.com/auth/devstorage.read_only,https://www.googleapis.com/auth/cloud-platform.read-only`.
- **S3 STS-backed read-only enforcement** - S3 has no OAuth scope concept, so credential-side enforcement would require minting an STS session token with an inline `Deny` policy for write actions. Today RO mode for S3 relies on the wrapper plus the user's IAM hygiene.
- **Object versioning awareness** - listing/operating on object versions isn't surfaced.
- **Cross-provider copy/move** - currently you can download then upload manually; a single-step transfer is not implemented.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
