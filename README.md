# STUIE - Storage Terminal UI Explorer

A modern, interactive TUI application for managing cloud object storage from your terminal. Browse buckets, upload/download files, and manage objects across multiple S3-compatible providers with a keyboard-driven interface.

![Rust](https://img.shields.io/badge/Rust-2026_edition-orange)
![License](https://img.shields.io/badge/License-MIT-blue)

## Features

- **Multi-provider support** - Connect to AWS S3, MinIO, and other S3-compatible storage backends simultaneously
- **Interactive file browser** - Navigate buckets and objects with a multi-pane TUI layout (sidebar, file list, preview)
- **Upload & download** - Transfer files with real-time progress tracking
- **File operations** - Create folders, delete objects (batch), search/filter by filename
- **Modern UI** - Color-coded file type icons (Nerd Fonts), mouse support, clipboard integration
- **Configuration management** - In-app config editor for adding/editing storage providers

## Prerequisites

- Rust 1.85+ and Cargo
- A Nerd Font installed in your terminal (for file icons)
- Access to an S3-compatible storage provider

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

STUIE stores its configuration at `~/.config/stuie/config.json`:

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
      "enabled": true
    }
  ],
  "show_icons": true
}
```

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
| `j` / `k` / arrows | Navigate |
| `Space` | Multi-select |
| `u` | Upload file |
| `d` | Download selected |
| `n` | New folder |
| `x` / `Delete` | Delete selected |
| `y` | Copy path to clipboard |

## Testing

Integration tests use a local MinIO instance via Docker:

```bash
./tests/minio_setup.sh    # Start MinIO + seed test data
cargo test
./tests/minio_teardown.sh # Cleanup
```

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
