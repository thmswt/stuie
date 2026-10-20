# STUIE - Storage Terminal UI Explorer

A keyboard-driven terminal UI for browsing cloud object storage. Browse buckets, upload and download files, and manage objects across AWS S3, MinIO and Google Cloud Storage from one place.

[![CI](https://img.shields.io/github/actions/workflow/status/thmswt/stuie/ci.yml?branch=main&logo=github&label=CI)](https://github.com/thmswt/stuie/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Rust-2024%20edition-orange)
![License](https://img.shields.io/badge/License-MIT-blue)

STUIE is a hobby project and is only distributed as source. Expect rough edges, and see [Limitations](#limitations) for what is not implemented yet.

## Features

- **Multiple providers at once** - AWS S3, MinIO (and other S3-compatible backends) and Google Cloud Storage side by side in one sidebar
- **File browser** - Buckets and objects in a multi-pane layout: sidebar, file list and an optional preview pane
- **Upload & download** - Files and folders, with progress tracking
- **Object operations** - Create folders, delete objects (batch, with confirmation), filter by filename, copy a path to the clipboard
- **Read-Only Mode** - A per-provider switch that hard-blocks every mutating operation ([details](#read-only-mode))
- **GCS multi-project** - Browse buckets across several GCP projects in one provider through `gcloud` ADC
- **Customisable** - Seven built-in themes, rebindable keys, Nerd Font icons and mouse support
- **In-app configuration** - Add and edit providers, settings, theme and keybindings without leaving the UI

## Getting started

### Prerequisites

- A recent stable Rust toolchain (2024 edition) and Cargo
- A [Nerd Font](https://www.nerdfonts.com/) set as your terminal font (for the file icons)
- Access to an S3-compatible service and/or a GCS account

STUIE is built and tested in CI on Linux and macOS. Windows is not covered by CI.

### Build and run

```bash
git clone https://github.com/thmswt/stuie.git
cd stuie
cargo run --release
```

The optimised binary ends up at `./target/release/stuie`. To put it on your `PATH`, run `cargo install --path .`.

On first launch, press `c` to open the configuration modal and add a storage provider.

## Configuration

Everything lives in `~/.config/stuie/config.json`. You can edit it by hand or through the in-app configuration modal (`c`), which has tabs for providers, settings, theme and keybindings.

Example with one S3 provider and one read-only GCS provider:

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

For MinIO or another S3-compatible service, use `"provider_type": "S3"`, set `endpoint` to its URL and set `allow_http` to `true` if it is served over plain HTTP.

### GCS authentication

- **`ServiceAccount`** - point `credentials_path` at a service-account JSON key (or set `GOOGLE_APPLICATION_CREDENTIALS`).
- **`GcloudAdc`** - reuse the credentials from `gcloud auth application-default login`. Multi-project mode lets you list buckets across several projects.

### Settings, themes and keys

| Setting | Default | Description |
|---|---|---|
| `show_icons` | `true` | Show Nerd Font file-type icons |
| `confirm_delete` | `true` | Ask before deleting objects |
| `preview_max_bytes` | `1024` | How much of a file the preview pane loads |
| `log_to_file` / `log_file_path` | `false` / `~/.stuie/session.log` | Write a summary of finished tasks (uploads, downloads, deletes) to a file |
| `diagnostics_enabled` / `diagnostics_log_dir` | `true` / `logs` | Write a diagnostics log (see below) |

These live under `"settings"` in `config.json`. Set the top-level `"theme"` key to one of `default`, `dracula`, `light`, `retro`, `ocean`, `solarized` or `nord`. Every key in the [shortcut table](#keyboard-shortcuts) can be changed under `"keymap"`.

**Diagnostics log:** with the default settings, STUIE writes a daily-rotating `stuie.log` to a `logs/` directory **relative to where you launch it**. Set `diagnostics_log_dir` to an absolute path, or turn diagnostics off, if you do not want that. The log can contain bucket and object paths.

## Read-Only Mode

Each provider has a `read_only` flag. Toggle it on the *Read-Only Mode* row in the config UI, or edit `config.json` directly. When it is set:

- The storage backend is wrapped in a `ReadOnlyStorage` decorator that returns an error for `upload`, `create_folder` and `delete_objects`, so no destructive call gets through regardless of the UI path.
- The destructive keybindings (`n`, `u`, `x`/`Delete`) short-circuit with an inline error instead of showing a misleading prompt first.
- The provider's sidebar row is badged `[RO]` in the warning colour.
- **GCS service-account mode additionally restricts the access token's OAuth scope to `devstorage.read_only`.** This applies to our JSON-API calls and to the bearer handed to `object_store` through a custom `CredentialProvider`. Even a bug that bypassed the wrapper would be rejected by GCS itself with HTTP 403.

The wrapper is the always-on guard. Scope restriction is a second layer where the provider supports it. See [Limitations](#limitations) for where it does not.

## Keyboard shortcuts

These are the defaults. All of them can be rebound.

| Key | Action |
|---|---|
| `q` | Quit |
| `?` | Toggle help |
| `c` | Configuration modal |
| `a` | Add provider (in the configuration modal) |
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

## Limitations

Not implemented yet:

- **Azure Blob Storage** - the provider type exists in the config and in the UI cycle, but the backend is a stub. An Azure provider has no effect at runtime.
- **GCS GcloudAdc read-only scope restriction** - service-account credentials get the OAuth scope restriction in Read-Only Mode, but the scopes of a gcloud ADC refresh token are fixed at `gcloud auth application-default login` time and cannot be narrowed per request. ADC providers fall back to the wrapper-only guard. Workaround: log in again with `--scopes=https://www.googleapis.com/auth/devstorage.read_only,https://www.googleapis.com/auth/cloud-platform.read-only`.
- **S3 credential-side read-only enforcement** - S3 has no OAuth-scope equivalent. It would need an STS session token with an inline `Deny` policy for write actions. Today Read-Only Mode on S3 relies on the wrapper plus your IAM setup.
- **Object versioning** - object versions are not listed or operated on.
- **Cross-provider copy/move** - download then upload; there is no single-step transfer.

## Security

STUIE handles cloud credentials, so it is worth being explicit about a few things. The full threat model is in [`SECURITY.md`](SECURITY.md).

- **No telemetry.** STUIE only makes network calls to the storage endpoints you configure (your S3 endpoint, the GCS API, and OAuth token endpoints for ADC). It sends no analytics or crash reports, does not check for updates, and sends credentials and tokens only to the provider they belong to. If you see other network activity, that is a bug.
- **Credentials are stored in plaintext.** `config.json` holds static credentials (S3 keys, GCS service-account paths) unencrypted. The trust boundary is your OS user account. Restrict the file with `chmod 600 ~/.config/stuie/config.json`.
- **Prefer short-lived credentials.** For GCS use `GcloudAdc` mode, which keeps no secrets in the config. For AWS, assume a role via SSO or AWS-Vault and only fill in the config when needed.
- **Turn on Read-Only Mode for production buckets.** It is the most effective way to make a wrong keypress harmless.

To report a vulnerability, use [GitHub's private vulnerability reporting](https://github.com/thmswt/stuie/security/advisories/new) and please do not open a public issue.

## Development

Unit tests need no external services:

```bash
cargo test --lib --test config_tests --test ui_utils_tests --test app_tests --test test_read_only_storage --test test_gcs_scope
```

The S3 integration tests run against a local MinIO container, so they need Docker:

```bash
./tests/minio_setup.sh    # start MinIO and seed test data
cargo test --test test_s3_provider
./tests/minio_teardown.sh # clean up
```

CI runs formatting and clippy, the unit tests on Linux and macOS, and the MinIO integration tests on Linux. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, commit conventions and the pull-request process.

## Stability

STUIE is on `0.x`. Config keys, keybindings and the UI may change between versions without notice. [`CHANGELOG.md`](CHANGELOG.md) tracks notable changes.

## License

Released under the MIT License. See [LICENSE](LICENSE).
