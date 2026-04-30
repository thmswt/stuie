use stuie::storage::gcs::{
    SaCredentialStrategy, choose_sa_credential_strategy, service_account_scope,
};

#[test]
fn service_account_scope_is_read_only_when_flag_set() {
    let scope = service_account_scope(true);
    assert_eq!(
        scope, "https://www.googleapis.com/auth/devstorage.read_only",
        "RO mode must request the devstorage.read_only scope so GCS itself rejects writes"
    );
}

#[test]
fn service_account_scope_is_cloud_platform_when_flag_unset() {
    let scope = service_account_scope(false);
    assert_eq!(
        scope, "https://www.googleapis.com/auth/cloud-platform",
        "Default scope must remain cloud-platform to preserve existing read+write behaviour"
    );
}

#[test]
fn ro_mode_routes_object_store_through_scoped_provider_even_with_key_file() {
    // The whole point of the adapter: even when the SA key file is present
    // (which used to be the unconditional path into `with_service_account_path`),
    // RO mode must take the scoped-provider branch instead so object_store's
    // own HTTP calls inherit the read-only scope.
    assert_eq!(
        choose_sa_credential_strategy(true, Some("/tmp/key.json")),
        SaCredentialStrategy::ScopedProvider
    );
}

#[test]
fn ro_mode_uses_scoped_provider_when_no_key_file() {
    assert_eq!(
        choose_sa_credential_strategy(true, None),
        SaCredentialStrategy::ScopedProvider
    );
}

#[test]
fn non_ro_mode_passes_key_file_to_object_store() {
    // Pin the legacy behaviour: when RO is off, object_store keeps minting
    // its own token from the SA key file, exactly as before this feature.
    assert_eq!(
        choose_sa_credential_strategy(false, Some("/tmp/key.json")),
        SaCredentialStrategy::KeyFile("/tmp/key.json")
    );
}

#[test]
fn non_ro_mode_falls_through_to_default_discovery_when_no_key_file() {
    assert_eq!(
        choose_sa_credential_strategy(false, None),
        SaCredentialStrategy::DefaultDiscovery
    );
}
