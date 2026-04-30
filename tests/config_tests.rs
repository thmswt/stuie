use stuie::config::{
    Config, GcsAuthMode, KeymapConfig, ProviderConfig, ProviderType, SettingsConfig,
};

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

// ===== GCS auth mode =====

#[test]
fn gcs_default_auth_mode_is_service_account() {
    let pc = ProviderConfig {
        provider_type: ProviderType::GCS,
        ..ProviderConfig::default()
    };
    assert_eq!(pc.gcs_auth_mode, GcsAuthMode::ServiceAccount);
}

#[test]
fn legacy_gcs_config_deserializes_with_defaults() {
    // Reader fixture mimicking an older config file that predates the
    // gcs_auth_mode / oauth_projects fields. Must deserialize into
    // ServiceAccount mode with an empty project list.
    let json = r#"{
        "providers": [{
            "name": "my-gcs",
            "provider_type": "GCS",
            "project": "my-project",
            "credentials_path": "/tmp/key.json"
        }]
    }"#;
    let cfg: Config = serde_json::from_str(json).expect("deserialize");
    assert_eq!(cfg.providers.len(), 1);
    let p = &cfg.providers[0];
    assert_eq!(p.gcs_auth_mode, GcsAuthMode::ServiceAccount);
    assert!(p.oauth_projects.is_empty());
    assert!(p.enabled);
}

#[test]
fn gcs_service_account_layout() {
    let pc = ProviderConfig {
        provider_type: ProviderType::GCS,
        gcs_auth_mode: GcsAuthMode::ServiceAccount,
        ..ProviderConfig::default()
    };
    // 6 provider-specific fields + 1 trailing Read-Only Mode field
    assert_eq!(pc.field_count(), 7);
    assert_eq!(pc.field_names()[0], "Name *");
    assert_eq!(pc.field_names()[2], "Auth Mode *");
    assert_eq!(pc.field_names()[3], "Project");
    assert_eq!(pc.field_names()[6], "Read-Only Mode");
}

#[test]
fn gcs_gcloud_adc_layout() {
    let pc = ProviderConfig {
        provider_type: ProviderType::GCS,
        gcs_auth_mode: GcsAuthMode::GcloudAdc,
        ..ProviderConfig::default()
    };
    // 5 provider-specific fields + 1 trailing Read-Only Mode field
    assert_eq!(pc.field_count(), 6);
    assert_eq!(pc.field_names()[2], "Auth Mode *");
    assert_eq!(pc.field_names()[3], "ADC Status");
    assert_eq!(pc.field_names()[4], "Selected Projects");
    assert_eq!(pc.field_names()[5], "Read-Only Mode");
}

#[test]
fn gcs_auth_mode_is_a_cycle_field_at_index_2() {
    let pc = ProviderConfig {
        provider_type: ProviderType::GCS,
        ..ProviderConfig::default()
    };
    assert!(pc.is_cycle_field(2));
    assert!(pc.cycle_field_hint(2).is_some());
}

#[test]
fn gcloud_adc_action_fields_are_not_free_text() {
    let mut pc = ProviderConfig {
        provider_type: ProviderType::GCS,
        gcs_auth_mode: GcsAuthMode::GcloudAdc,
        ..ProviderConfig::default()
    };
    // Index 3 (ADC status) and 4 (project picker) are read-only / action.
    assert!(pc.is_action_field(3));
    assert!(pc.is_action_field(4));
    assert!(pc.get_field_mut(3).is_none());
    assert!(pc.get_field_mut(4).is_none());
}

#[test]
fn gcs_project_count_is_rendered_in_selected_projects_field() {
    let mut pc = ProviderConfig {
        provider_type: ProviderType::GCS,
        gcs_auth_mode: GcsAuthMode::GcloudAdc,
        ..ProviderConfig::default()
    };
    assert!(pc.get_field_value(4).contains("none"));
    pc.oauth_projects = vec!["proj-a".into(), "proj-b".into()];
    let v = pc.get_field_value(4);
    assert!(v.contains("2 selected"));
    assert!(v.contains("proj-a"));
    assert!(v.contains("proj-b"));
}

#[test]
fn read_only_field_default_is_false_for_every_provider_type() {
    for pt in [ProviderType::S3, ProviderType::GCS, ProviderType::Azure] {
        let pc = ProviderConfig {
            provider_type: pt.clone(),
            ..ProviderConfig::default()
        };
        assert!(!pc.read_only, "read_only must default to false for {:?}", pt);
    }
}

#[test]
fn read_only_field_is_last_and_a_cycle_field_for_every_layout() {
    let layouts = [
        ProviderConfig {
            provider_type: ProviderType::S3,
            ..ProviderConfig::default()
        },
        ProviderConfig {
            provider_type: ProviderType::Azure,
            ..ProviderConfig::default()
        },
        ProviderConfig {
            provider_type: ProviderType::GCS,
            gcs_auth_mode: GcsAuthMode::ServiceAccount,
            ..ProviderConfig::default()
        },
        ProviderConfig {
            provider_type: ProviderType::GCS,
            gcs_auth_mode: GcsAuthMode::GcloudAdc,
            ..ProviderConfig::default()
        },
    ];
    for pc in &layouts {
        let last_idx = pc.field_count() - 1;
        assert_eq!(
            pc.read_only_field_index(),
            last_idx,
            "Read-Only field must be the last field for {:?}/{:?}",
            pc.provider_type,
            pc.gcs_auth_mode,
        );
        assert!(pc.is_read_only_field(last_idx));
        assert!(pc.is_cycle_field(last_idx));
        assert!(pc.cycle_field_hint(last_idx).is_some());
        assert_eq!(pc.field_names()[last_idx], "Read-Only Mode");
        assert_eq!(pc.get_field_value(last_idx), "false");
    }
}

#[test]
fn read_only_value_renders_as_true_when_set() {
    let pc = ProviderConfig {
        provider_type: ProviderType::S3,
        read_only: true,
        ..ProviderConfig::default()
    };
    assert_eq!(pc.get_field_value(pc.read_only_field_index()), "true");
}

#[test]
fn legacy_config_without_read_only_deserializes_with_false() {
    // A config file from before the RO flag existed: must round-trip with
    // read_only defaulting to false (not Some/None).
    let json = r#"{
        "providers": [{
            "name": "s3-prod",
            "provider_type": "S3",
            "region": "us-east-1"
        }]
    }"#;
    let cfg: Config = serde_json::from_str(json).expect("deserialize");
    assert_eq!(cfg.providers.len(), 1);
    assert!(!cfg.providers[0].read_only);
}

#[test]
fn read_only_round_trips_through_serde() {
    let cfg = Config {
        providers: vec![ProviderConfig {
            name: "s3-readonly".into(),
            provider_type: ProviderType::S3,
            read_only: true,
            ..ProviderConfig::default()
        }],
        ..Config::default()
    };
    let json = serde_json::to_string(&cfg).expect("serialize");
    let parsed: Config = serde_json::from_str(&json).expect("deserialize");
    assert!(parsed.providers[0].read_only);
}

#[test]
fn config_round_trip_preserves_oauth_projects() {
    let cfg = Config {
        providers: vec![ProviderConfig {
            name: "gcs-one".into(),
            provider_type: ProviderType::GCS,
            gcs_auth_mode: GcsAuthMode::GcloudAdc,
            oauth_projects: vec!["alpha".into(), "beta".into()],
            ..ProviderConfig::default()
        }],
        ..Config::default()
    };
    let json = serde_json::to_string(&cfg).expect("serialize");
    let parsed: Config = serde_json::from_str(&json).expect("deserialize");
    let p = &parsed.providers[0];
    assert_eq!(p.gcs_auth_mode, GcsAuthMode::GcloudAdc);
    assert_eq!(p.oauth_projects, vec!["alpha", "beta"]);
}
