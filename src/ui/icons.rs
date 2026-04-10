use ratatui::style::Color;
use crate::config::ProviderType;

pub struct FileIcon {
    pub symbol: char,
    pub color: Color,
}

fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128);
    Color::Rgb(r, g, b)
}

/// Cloud-storage-specific extensions not covered by the devicons crate.
fn cloud_storage_icon(ext: &str) -> Option<FileIcon> {
    match ext {
        "parquet" => Some(FileIcon { symbol: '\u{f0219}', color: Color::Rgb(80, 176, 80) }),
        "avro" => Some(FileIcon { symbol: '\u{f0219}', color: Color::Rgb(0, 116, 217) }),
        "orc" => Some(FileIcon { symbol: '\u{f0219}', color: Color::Rgb(177, 13, 201) }),
        "tar" => Some(FileIcon { symbol: '\u{f410}', color: Color::Rgb(236, 165, 23) }),
        "tfrecord" => Some(FileIcon { symbol: '\u{f0219}', color: Color::Rgb(255, 111, 0) }),
        "proto" => Some(FileIcon { symbol: '\u{f0219}', color: Color::Rgb(0, 150, 136) }),
        "ndjson" | "jsonl" => Some(FileIcon { symbol: '\u{e60b}', color: Color::Rgb(203, 203, 65) }),
        _ => None,
    }
}

pub fn get_file_icon(filename: &str, is_dir: bool) -> FileIcon {
    if is_dir {
        return FileIcon { symbol: '\u{f115}', color: Color::Cyan };
    }

    // Check extension against cloud-storage supplementary map
    if let Some(dot_pos) = filename.rfind('.') {
        let ext = &filename[dot_pos + 1..];
        if let Some(icon) = cloud_storage_icon(ext) {
            return icon;
        }
    }

    // Fall back to devicons crate
    let icon = devicons::icon_for_file(filename, &Some(devicons::Theme::Dark));
    FileIcon {
        symbol: icon.icon,
        color: hex_to_color(icon.color),
    }
}

// Sidebar icons

pub const ICON_AWS: FileIcon = FileIcon { symbol: '\u{e7ad}', color: Color::Rgb(255, 153, 0) };
pub const ICON_GCS: FileIcon = FileIcon { symbol: '\u{f1a0}', color: Color::Rgb(66, 133, 244) };
pub const ICON_AZURE: FileIcon = FileIcon { symbol: '\u{ebd8}', color: Color::Rgb(0, 171, 238) };
pub const ICON_BUCKET: FileIcon = FileIcon { symbol: '\u{f115}', color: Color::Cyan };

pub fn provider_icon(provider_type: &ProviderType) -> &'static FileIcon {
    match provider_type {
        ProviderType::S3 => &ICON_AWS,
        ProviderType::GCS => &ICON_GCS,
        ProviderType::Azure => &ICON_AZURE,
    }
}
