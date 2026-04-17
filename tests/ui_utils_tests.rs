use ratatui::layout::Rect;
use stuie::ui::{centered_fixed, format_hex, format_size, is_text, shorten_path};

#[test]
fn format_size_bytes() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(1023), "1023 B");
}

#[test]
fn format_size_scales_to_kb_mb_gb() {
    assert_eq!(format_size(1024), "1.00 KB");
    assert_eq!(format_size(1024 * 1024), "1.00 MB");
    assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    assert_eq!(format_size(1536), "1.50 KB");
}

#[test]
fn is_text_plain_ascii() {
    assert!(is_text(b""));
    assert!(is_text(b"hello world"));
    assert!(is_text(b"line 1\nline 2\r\n"));
    assert!(is_text(b"tab\there"));
}

#[test]
fn is_text_rejects_binary() {
    assert!(!is_text(&[0u8]));
    assert!(!is_text(&[0x01, 0x02, 0x03]));
    assert!(!is_text(b"has\x07bell"));
}

#[test]
fn format_hex_basic() {
    let out = format_hex(b"AB");
    assert!(out.contains("41 42"));
    assert!(out.ends_with("| AB"));
}

#[test]
fn format_hex_non_printable_becomes_dot() {
    let out = format_hex(&[0x00, 0xff, 0x41]);
    assert!(out.contains("00 ff 41"));
    // 0x00 and 0xff render as dots; 0x41 -> 'A'
    assert!(out.ends_with("| ..A"));
}

#[test]
fn format_hex_chunks_every_16_bytes() {
    let data: Vec<u8> = (0..32u8).collect();
    let out = format_hex(&data);
    assert_eq!(out.lines().count(), 2);
}

#[test]
fn centered_fixed_clamps_to_parent() {
    let parent = Rect::new(0, 0, 50, 10);
    let centered = centered_fixed(100, 100, parent);
    assert_eq!(centered.width, 50);
    assert_eq!(centered.height, 10);
    assert_eq!(centered.x, 0);
    assert_eq!(centered.y, 0);
}

#[test]
fn centered_fixed_centers_within_parent() {
    let parent = Rect::new(0, 0, 100, 20);
    let c = centered_fixed(40, 6, parent);
    assert_eq!(c.width, 40);
    assert_eq!(c.height, 6);
    assert_eq!(c.x, 30);
    assert_eq!(c.y, 7);
}

#[test]
fn shorten_path_no_change_when_within_width() {
    let s = "s3://bucket/a/b";
    assert_eq!(shorten_path(s, 100), s);
}

#[test]
fn shorten_path_replaces_leading_segments_with_dots() {
    let full = "s3://bucket/very/long/nested/path/to/file.txt";
    let short = shorten_path(full, 30);
    assert!(short.starts_with("s3://bucket/"));
    assert!(short.contains(".."));
    assert!(short.ends_with("file.txt"));
}

#[test]
fn shorten_path_returns_input_when_not_remote_format() {
    let s = "just-a-local-name";
    assert_eq!(shorten_path(s, 5), s);
}
