use stuie::config::{Config, KeymapConfig, ProviderConfig, ProviderType, SettingsConfig};

// ===== KeymapConfig =====

#[test]
fn keymap_default_has_no_empty_bindings() {
    let km = KeymapConfig::default();
    for i in 0..KeymapConfig::FIELD_COUNT {
        assert!(
            !km.get_field(i).is_empty(),
            "field {} ({}) has empty default binding",
            i,
            KeymapConfig::field_name(i)
        );
    }
}

#[test]
fn keymap_field_name_covers_every_index() {
    for i in 0..KeymapConfig::FIELD_COUNT {
        assert!(
            !KeymapConfig::field_name(i).is_empty(),
            "field_name({}) returned empty string",
            i
        );
    }
    assert!(KeymapConfig::field_name(KeymapConfig::FIELD_COUNT).is_empty());
}

#[test]
fn keymap_get_set_field_round_trip() {
    let mut km = KeymapConfig::default();
    for i in 0..KeymapConfig::FIELD_COUNT {
        let marker = format!("zz-{}", i);
        km.set_field(i, marker.clone());
        assert_eq!(km.get_field(i), marker, "round trip failed for index {}", i);
    }
}

#[test]
fn keymap_set_field_out_of_bounds_is_noop() {
    let mut km = KeymapConfig::default();
    let before: Vec<String> = (0..KeymapConfig::FIELD_COUNT)
        .map(|i| km.get_field(i).to_string())
        .collect();
    km.set_field(999, "bogus".into());
    let after: Vec<String> = (0..KeymapConfig::FIELD_COUNT)
        .map(|i| km.get_field(i).to_string())
        .collect();
    assert_eq!(before, after);
}

#[test]
fn keymap_section_headers_match_layout() {
    assert_eq!(KeymapConfig::section_header(0), Some("Normal Mode"));
    assert_eq!(KeymapConfig::section_header(17), Some("Edit Mode"));
    assert_eq!(KeymapConfig::section_header(1), None);
    assert_eq!(KeymapConfig::section_header(16), None);
}

#[test]
fn keymap_toggle_preview_default_is_p() {
    let km = KeymapConfig::default();
    assert_eq!(km.toggle_preview, "p");
}

// ===== SettingsConfig =====

#[test]
fn settings_defaults() {
    let s = SettingsConfig::default();
    assert!(s.show_icons);
    assert!(s.confirm_delete);
    assert_eq!(s.preview_max_bytes, 1024);
    assert!(!s.log_to_file, "log_to_file must default to false");
    assert!(
        !s.log_file_path.is_empty(),
        "log_file_path must have a default"
    );
}

#[test]
fn settings_field_name_covers_every_index() {
    for i in 0..SettingsConfig::FIELD_COUNT {
        assert!(
            !SettingsConfig::field_name(i).is_empty(),
            "field_name({}) returned empty string",
            i
        );
    }
    assert!(SettingsConfig::field_name(SettingsConfig::FIELD_COUNT).is_empty());
}

// ===== ProviderType =====

#[test]
fn provider_type_scheme() {
    assert_eq!(ProviderType::S3.scheme(), "s3");
    assert_eq!(ProviderType::GCS.scheme(), "gs");
    assert_eq!(ProviderType::Azure.scheme(), "az");
}

#[test]
fn provider_type_field_count_matches_names() {
    for pt in [ProviderType::S3, ProviderType::GCS, ProviderType::Azure] {
        assert_eq!(
            pt.field_count(),
            pt.field_names().len(),
            "field_count and field_names length disagree for {:?}",
            pt
        );
    }
}

#[test]
fn provider_type_default_is_s3() {
    assert_eq!(ProviderType::default(), ProviderType::S3);
}

// ===== ProviderConfig =====

#[test]
fn provider_config_s3_cycle_fields() {
    let pc = ProviderConfig {
        provider_type: ProviderType::S3,
        ..ProviderConfig::default()
    };
    assert!(pc.is_cycle_field(1), "Type field should be a cycle field");
    assert!(pc.is_cycle_field(7), "Allow HTTP (S3 idx 7) should cycle");
    assert!(!pc.is_cycle_field(0));
    assert!(!pc.is_cycle_field(2));
    assert!(pc.cycle_field_hint(1).is_some());
    assert!(pc.cycle_field_hint(7).is_some());
    assert!(pc.cycle_field_hint(0).is_none());
}

#[test]
fn provider_config_gcs_has_no_allow_http_cycle() {
    let pc = ProviderConfig {
        provider_type: ProviderType::GCS,
        ..ProviderConfig::default()
    };
    assert!(pc.is_cycle_field(1), "Type always cycles");
    assert!(!pc.is_cycle_field(7), "GCS has no Allow HTTP field");
    assert!(pc.cycle_field_hint(7).is_none());
}

#[test]
fn provider_config_get_field_mut_writes_through() {
    let mut pc = ProviderConfig {
        provider_type: ProviderType::S3,
        ..ProviderConfig::default()
    };
    // Index 0 is "Name"
    *pc.get_field_mut(0).unwrap() = "my-bucket".into();
    assert_eq!(pc.name, "my-bucket");
    // Index 1 is Type (cycle) — no text access
    assert!(pc.get_field_mut(1).is_none());
}

// ===== Config serialization =====

#[test]
fn config_serde_round_trip() {
    let mut cfg = Config::default();
    cfg.settings.log_to_file = true;
    cfg.settings.log_file_path = "/tmp/stuie.log".into();
    cfg.keymap.toggle_preview = "P".into();
    let json = serde_json::to_string(&cfg).expect("serialize");
    let parsed: Config = serde_json::from_str(&json).expect("deserialize");
    assert!(parsed.settings.log_to_file);
    assert_eq!(parsed.settings.log_file_path, "/tmp/stuie.log");
    assert_eq!(parsed.keymap.toggle_preview, "P");
}

#[test]
fn config_deserialize_with_missing_new_fields_uses_defaults() {
    // Simulate a pre-existing config file without the new log settings.
    let json = r#"{
        "providers": [],
        "settings": { "show_icons": false, "confirm_delete": false, "preview_max_bytes": 2048 }
    }"#;
    let cfg: Config = serde_json::from_str(json).expect("deserialize");
    assert!(!cfg.settings.show_icons);
    assert_eq!(cfg.settings.preview_max_bytes, 2048);
    // New fields fall back to defaults
    assert!(!cfg.settings.log_to_file);
    assert!(!cfg.settings.log_file_path.is_empty());
    // Keymap also falls back
    assert_eq!(cfg.keymap.toggle_preview, "p");
}
