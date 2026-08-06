use crate::config::{GcsAuthMode, ProviderConfig, ProviderType};
use crate::storage::{Bucket, GcpProject, Object, ProgressCallback, StorageBackend};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures::StreamExt;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

/// Returns the path where gcloud stores application-default credentials on
/// all platforms. (gcloud uses `~/.config/gcloud/...` on macOS too, not
/// Library/Application Support — so we explicitly avoid `dirs::config_dir`
/// here.)
pub fn default_adc_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config/gcloud/application_default_credentials.json"))
}

/// Locate an Application Default Credentials file on disk. Checks the
/// `GOOGLE_APPLICATION_CREDENTIALS` env var first, then the default gcloud
/// ADC path. Returns `None` if neither is present.
pub fn detect_adc_path() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        let p = PathBuf::from(v);
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(p) = default_adc_path() {
        if p.exists() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Credential types
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct CredentialFile {
    #[serde(rename = "type")]
    credential_type: String,
    // Service-account fields
    client_email: Option<String>,
    private_key: Option<String>,
    token_uri: Option<String>,
    project_id: Option<String>,
    // Authorized-user (ADC) fields
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
}

/// OAuth scope to request for a service-account access token. When the
/// provider is in read-only mode we request `devstorage.read_only`, which
/// GCS itself rejects writes against (HTTP 403) — defense-in-depth on top
/// of the `ReadOnlyStorage` wrapper. Otherwise we keep the historical
/// `cloud-platform` scope so the existing read+write behaviour is unchanged.
pub fn service_account_scope(read_only: bool) -> &'static str {
    if read_only {
        "https://www.googleapis.com/auth/devstorage.read_only"
    } else {
        "https://www.googleapis.com/auth/cloud-platform"
    }
}

/// How `create_store` should populate object_store's credential builder for
/// service-account auth. Pulled out as its own enum so the wiring decision
/// (RO ⇒ scope-restricted provider, otherwise key-file) has a unit test that
/// doesn't require touching object_store's builder internals.
#[derive(Debug, PartialEq, Eq)]
pub enum SaCredentialStrategy<'a> {
    /// Use our own `ScopedTokenProvider`, which mints a token via
    /// `service_account_scope(read_only=true)` ⇒ devstorage.read_only.
    /// object_store then signs every request with that bearer, so even
    /// internal-to-object_store code paths cannot perform writes.
    ScopedProvider,
    /// Hand object_store the SA key file path and let it manage its own
    /// token (cloud-platform scope) — historical behaviour for non-RO mode.
    KeyFile(&'a str),
    /// Neither RO nor a usable key-file path; fall through to whatever
    /// object_store discovers via ADC.
    DefaultDiscovery,
}

pub fn choose_sa_credential_strategy(
    read_only: bool,
    credentials_path: Option<&str>,
) -> SaCredentialStrategy<'_> {
    if read_only {
        SaCredentialStrategy::ScopedProvider
    } else if let Some(path) = credentials_path {
        SaCredentialStrategy::KeyFile(path)
    } else {
        SaCredentialStrategy::DefaultDiscovery
    }
}

enum GcsCredentials {
    ServiceAccount {
        client_email: String,
        private_key: String,
        token_uri: String,
        project_id: Option<String>,
    },
    AuthorizedUser {
        client_id: String,
        client_secret: String,
        refresh_token: String,
    },
}

struct CachedToken {
    access_token: String,
    expires_at: std::time::Instant,
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

// ---------------------------------------------------------------------------
// CredentialProvider adapter for object_store
// ---------------------------------------------------------------------------

/// Bridges our own `GcsStorage::get_access_token` (which honours the
/// `read_only` flag via `service_account_scope`) into the
/// `object_store::CredentialProvider` shape. Used in read-only mode so
/// that object_store's internal request signing uses the exact same
/// scope-restricted bearer as our JSON-API path.
struct ScopedTokenProvider {
    storage: GcsStorage,
}

impl std::fmt::Debug for ScopedTokenProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedTokenProvider")
            .field("provider", &self.storage.config.name)
            .field("read_only", &self.storage.config.read_only)
            .finish()
    }
}

#[async_trait]
impl object_store::CredentialProvider for ScopedTokenProvider {
    type Credential = object_store::gcp::GcpCredential;

    async fn get_credential(&self) -> object_store::Result<Arc<object_store::gcp::GcpCredential>> {
        let bearer =
            self.storage
                .get_access_token()
                .await
                .map_err(|e| object_store::Error::Generic {
                    store: "GCS",
                    source: e.into(),
                })?;
        Ok(Arc::new(object_store::gcp::GcpCredential { bearer }))
    }
}

// ---------------------------------------------------------------------------
// GCS bucket-list API response
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct BucketListResponse {
    #[serde(default)]
    items: Vec<BucketItem>,
}

#[derive(serde::Deserialize)]
struct BucketItem {
    name: String,
    location: Option<String>,
}

// ---------------------------------------------------------------------------
// GcsStorage
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct GcsStorage {
    config: ProviderConfig,
    provider_index: usize,
    http_client: reqwest::Client,
    token_cache: Arc<RwLock<Option<CachedToken>>>,
    credentials: Arc<GcsCredentials>,
    /// Resolved path to credential file (may differ from config if found via
    /// env var or ADC default location).
    credentials_path: Option<String>,
}

impl GcsStorage {
    pub async fn new(config: ProviderConfig, provider_index: usize) -> Result<Self> {
        let http_client = reqwest::Client::new();
        let (credentials, resolved_path) = Self::load_credentials(&config)?;

        Ok(Self {
            config,
            provider_index,
            http_client,
            token_cache: Arc::new(RwLock::new(None)),
            credentials: Arc::new(credentials),
            credentials_path: resolved_path,
        })
    }

    // -- credential loading -------------------------------------------------

    fn load_credentials(config: &ProviderConfig) -> Result<(GcsCredentials, Option<String>)> {
        // In GcloudAdc mode we always go through the ADC path, ignoring any
        // value the user might have typed into credentials_path (so the two
        // auth modes never interfere with each other).
        let resolved_path = match config.gcs_auth_mode {
            GcsAuthMode::GcloudAdc => detect_adc_path().map(|p| p.to_string_lossy().to_string()),
            GcsAuthMode::ServiceAccount => config
                .credentials_path
                .as_ref()
                .filter(|p| !p.trim().is_empty())
                .cloned()
                .or_else(|| std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok())
                .or_else(|| detect_adc_path().map(|p| p.to_string_lossy().to_string())),
        };

        let resolved_path = resolved_path.with_context(|| match config.gcs_auth_mode {
            GcsAuthMode::GcloudAdc => "No Application Default Credentials found. Run \
                 'gcloud auth application-default login \
                 --scopes=https://www.googleapis.com/auth/cloud-platform.read-only' \
                 then reopen this provider."
                .to_string(),
            GcsAuthMode::ServiceAccount => "No GCS credentials found. Provide a Credentials Path \
                 in the provider config, set the GOOGLE_APPLICATION_CREDENTIALS env var, or run \
                 'gcloud auth application-default login'."
                .to_string(),
        })?;

        let content = std::fs::read_to_string(&resolved_path)
            .with_context(|| format!("Failed to read credentials file: {}", resolved_path))?;

        let cred_file: CredentialFile =
            serde_json::from_str(&content).context("Failed to parse credentials file")?;

        let credentials = match cred_file.credential_type.as_str() {
            "service_account" => GcsCredentials::ServiceAccount {
                client_email: cred_file
                    .client_email
                    .context("Missing client_email in service account key")?,
                private_key: cred_file
                    .private_key
                    .context("Missing private_key in service account key")?,
                token_uri: cred_file
                    .token_uri
                    .unwrap_or_else(|| "https://oauth2.googleapis.com/token".to_string()),
                project_id: cred_file.project_id,
            },
            "authorized_user" => GcsCredentials::AuthorizedUser {
                client_id: cred_file
                    .client_id
                    .context("Missing client_id in authorized user credentials")?,
                client_secret: cred_file
                    .client_secret
                    .context("Missing client_secret in authorized user credentials")?,
                refresh_token: cred_file
                    .refresh_token
                    .context("Missing refresh_token in authorized user credentials")?,
            },
            other => bail!("Unsupported GCS credential type: {}", other),
        };

        Ok((credentials, Some(resolved_path)))
    }

    // -- token management ---------------------------------------------------

    #[tracing::instrument(skip_all, fields(provider = %self.config.name))]
    async fn get_access_token(&self) -> Result<String> {
        // Return cached token if still valid (with 60 s margin).
        {
            let cache = self.token_cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.expires_at
                    > std::time::Instant::now() + std::time::Duration::from_secs(60)
                {
                    tracing::debug!("GCS token cache hit");
                    return Ok(cached.access_token.clone());
                }
            }
        }
        tracing::info!("GCS token cache miss; fetching new access token");

        let token_resp = match self.credentials.as_ref() {
            GcsCredentials::ServiceAccount {
                client_email,
                private_key,
                token_uri,
                ..
            } => {
                self.fetch_service_account_token(client_email, private_key, token_uri)
                    .await?
            }
            GcsCredentials::AuthorizedUser {
                client_id,
                client_secret,
                refresh_token,
            } => {
                self.fetch_user_token(client_id, client_secret, refresh_token)
                    .await?
            }
        };

        let cached = CachedToken {
            access_token: token_resp.access_token.clone(),
            expires_at: std::time::Instant::now()
                + std::time::Duration::from_secs(token_resp.expires_in.saturating_sub(120)),
        };

        {
            let mut cache = self.token_cache.write().await;
            *cache = Some(cached);
        }

        Ok(token_resp.access_token)
    }

    async fn fetch_service_account_token(
        &self,
        client_email: &str,
        private_key: &str,
        token_uri: &str,
    ) -> Result<TokenResponse> {
        let now = chrono::Utc::now();
        let claims = serde_json::json!({
            "iss": client_email,
            "scope": service_account_scope(self.config.read_only),
            "aud": token_uri,
            "iat": now.timestamp(),
            "exp": (now + chrono::TimeDelta::hours(1)).timestamp(),
        });

        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key.as_bytes())
            .context("Failed to parse service account private key")?;
        let jwt = jsonwebtoken::encode(&header, &claims, &key)
            .context("Failed to create JWT assertion")?;

        let resp = self
            .http_client
            .post(token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt),
            ])
            .send()
            .await
            .context("Failed to exchange JWT for access token")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Token exchange failed ({}): {}", status, body);
        }

        resp.json().await.context("Failed to parse token response")
    }

    async fn fetch_user_token(
        &self,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<TokenResponse> {
        let resp = self
            .http_client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await
            .context("Failed to refresh access token")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Token refresh failed ({}): {}", status, body);
        }

        resp.json().await.context("Failed to parse token response")
    }

    // -- helpers ------------------------------------------------------------

    fn create_store(&self, bucket: &str) -> Result<Box<dyn ObjectStore>> {
        let mut builder = GoogleCloudStorageBuilder::new().with_bucket_name(bucket);

        if let GcsCredentials::ServiceAccount { .. } = self.credentials.as_ref() {
            match choose_sa_credential_strategy(
                self.config.read_only,
                self.credentials_path.as_deref(),
            ) {
                SaCredentialStrategy::ScopedProvider => {
                    let provider: object_store::gcp::GcpCredentialProvider =
                        Arc::new(ScopedTokenProvider {
                            storage: self.clone(),
                        });
                    builder = builder.with_credentials(provider);
                }
                SaCredentialStrategy::KeyFile(path) => {
                    builder = builder.with_service_account_path(path);
                }
                SaCredentialStrategy::DefaultDiscovery => {}
            }
        }
        // For authorized-user (ADC), object_store picks up credentials from
        // the default ADC location automatically. ADC tokens are minted by
        // `gcloud auth application-default login` with whatever scope the
        // user authenticated with — see the docs for restricting RO mode.

        let store = builder
            .build()
            .context("Failed to build GCS object store")?;
        Ok(Box::new(store))
    }

    /// Returns the project ID detected from credentials (e.g. service account key), if any.
    pub fn detected_project_id(&self) -> Option<&str> {
        if let GcsCredentials::ServiceAccount { project_id, .. } = self.credentials.as_ref() {
            project_id.as_deref()
        } else {
            None
        }
    }

    /// Resolve the GCP project to use for bucket listing.
    fn get_project(&self) -> Result<String> {
        if let Some(project) = &self.config.project {
            if !project.trim().is_empty() {
                return Ok(project.clone());
            }
        }
        if let GcsCredentials::ServiceAccount {
            project_id: Some(pid),
            ..
        } = self.credentials.as_ref()
        {
            return Ok(pid.clone());
        }
        bail!(
            "No GCS project configured. Set the Project field in the provider config \
             or use a service account key that contains a project_id."
        )
    }

    /// Delete a single GCS object by exact name via the JSON API. The name
    /// is preserved verbatim (including any trailing `/`) so we can target
    /// folder-marker keys that `object_store::path::Path` would otherwise
    /// strip.
    async fn gcs_delete_object(&self, token: &str, bucket: &str, object_name: &str) -> Result<()> {
        let url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}",
            bucket,
            percent_encode_object_name(object_name)
        );
        let resp = self
            .http_client
            .delete(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("Failed to delete GCS object '{}'", object_name))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!(
                "GCS delete failed for '{}' ({}): {}",
                object_name,
                status,
                body
            );
        }
        Ok(())
    }

    /// List buckets under a specific project. Shared by the aggregating
    /// `list_buckets` call (GcloudAdc mode) and the trait-level
    /// `list_buckets_for_project` override used by the sidebar.
    async fn list_buckets_for_project_inner(&self, project: &str) -> Result<Vec<Bucket>> {
        let token = self.get_access_token().await?;
        tracing::info!(
            provider = %self.config.name,
            project = %project,
            "Listing GCS buckets for project"
        );

        let resp = self
            .http_client
            .get("https://storage.googleapis.com/storage/v1/b")
            .bearer_auth(&token)
            .query(&[("project", project)])
            .send()
            .await
            .context("Failed to list GCS buckets")?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .context("Failed to read bucket list response body")?;

        if !status.is_success() {
            bail!(
                "GCS bucket listing for project '{}' failed ({}): {}",
                project,
                status,
                body
            );
        }

        let list: BucketListResponse = serde_json::from_str(&body).with_context(|| {
            format!(
                "Failed to parse bucket list response for project '{}' (status {}, body: {})",
                project, status, body
            )
        })?;

        let buckets = list
            .items
            .into_iter()
            .map(|b| Bucket {
                name: b.name,
                provider_index: self.provider_index,
                location: b.location,
                project_id: Some(project.to_string()),
            })
            .collect();

        Ok(buckets)
    }
}

// ---------------------------------------------------------------------------
// GCS object-list API response (used by recursive delete)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ObjectListResponse {
    #[serde(default)]
    items: Vec<ObjectListItem>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(serde::Deserialize)]
struct ObjectListItem {
    name: String,
}

/// Percent-encode bytes that aren't unreserved per RFC 3986. Used to embed
/// a GCS object name (which may contain `/`, spaces, etc.) inside the URL
/// path of the JSON API.
fn percent_encode_object_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Cloud Resource Manager API response
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ProjectListResponse {
    #[serde(default)]
    projects: Vec<ProjectItem>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(serde::Deserialize)]
struct ProjectItem {
    #[serde(rename = "projectId")]
    project_id: String,
    name: Option<String>,
    #[serde(rename = "lifecycleState")]
    lifecycle_state: Option<String>,
}

// ---------------------------------------------------------------------------
// StorageBackend implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl StorageBackend for GcsStorage {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::GCS
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name))]
    async fn list_projects(&self) -> Result<Vec<GcpProject>> {
        let token = self.get_access_token().await?;
        let mut projects: Vec<GcpProject> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut query: Vec<(&str, String)> = vec![
                ("filter", "lifecycleState:ACTIVE".to_string()),
                ("pageSize", "200".to_string()),
            ];
            if let Some(tok) = &page_token {
                query.push(("pageToken", tok.clone()));
            }

            let resp = self
                .http_client
                .get("https://cloudresourcemanager.googleapis.com/v1/projects")
                .bearer_auth(&token)
                .query(&query)
                .send()
                .await
                .context("Failed to list GCP projects")?;

            let status = resp.status();
            let body = resp
                .text()
                .await
                .context("Failed to read projects list response body")?;

            if !status.is_success() {
                bail!(
                    "GCP project listing failed ({}): {}. \
                     Ensure your gcloud ADC token has the cloud-platform or \
                     cloud-platform.read-only scope.",
                    status,
                    body
                );
            }

            let parsed: ProjectListResponse = serde_json::from_str(&body).with_context(|| {
                format!(
                    "Failed to parse projects list response (status {}, body: {})",
                    status, body
                )
            })?;

            for p in parsed.projects {
                // Belt-and-braces: the filter should already exclude non-active.
                if let Some(state) = &p.lifecycle_state {
                    if state != "ACTIVE" {
                        continue;
                    }
                }
                projects.push(GcpProject {
                    name: p.name.clone().unwrap_or_else(|| p.project_id.clone()),
                    id: p.project_id,
                });
            }

            match parsed.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
        }

        tracing::info!(
            provider = %self.config.name,
            project_count = projects.len(),
            "GCP project listing complete"
        );

        Ok(projects)
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name))]
    async fn list_buckets(&self) -> Result<Vec<Bucket>> {
        // If a specific bucket is configured, skip the API call.
        if let Some(bucket_name) = &self.config.bucket_name {
            if !bucket_name.trim().is_empty() {
                tracing::info!(bucket = %bucket_name, "Using configured bucket; skipping GCS bucket listing");
                return Ok(vec![Bucket {
                    name: bucket_name.clone(),
                    provider_index: self.provider_index,
                    location: None,
                    project_id: None,
                }]);
            }
        }

        match self.config.gcs_auth_mode {
            GcsAuthMode::ServiceAccount => {
                let project = self.get_project()?;
                let mut buckets = self.list_buckets_for_project_inner(&project).await?;
                // Single-project mode: leave project_id=None so the sidebar
                // renders the existing 2-level tree (Provider > Bucket).
                for b in &mut buckets {
                    b.project_id = None;
                }
                Ok(buckets)
            }
            GcsAuthMode::GcloudAdc => {
                // Multi-project mode does NOT aggregate here any more.
                // Buckets are fetched lazily when the user presses Enter on
                // a Project row in the sidebar (see
                // `list_buckets_for_project` trait override). Returning an
                // empty Vec keeps `ExpandProvider` from hitting the network
                // during provider expansion.
                Ok(Vec::new())
            }
        }
    }

    async fn list_buckets_for_project(&self, project_id: &str) -> Result<Vec<Bucket>> {
        self.list_buckets_for_project_inner(project_id).await
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name, bucket = %bucket, prefix = ?prefix))]
    async fn list_objects(&self, bucket: &str, prefix: Option<&Path>) -> Result<Vec<Object>> {
        let store = self.create_store(bucket)?;
        let list_result = store.list_with_delimiter(prefix).await?;
        let mut objects = Vec::new();

        for common_prefix in list_result.common_prefixes {
            if let Some(p) = prefix {
                if &common_prefix == p {
                    continue;
                }
            }
            objects.push(Object {
                path: common_prefix,
                is_dir: true,
                size: 0,
                last_modified: chrono::Utc::now(),
            });
        }

        for meta in list_result.objects {
            if let Some(p) = prefix {
                if &meta.location == p {
                    continue;
                }
            }
            objects.push(Object {
                path: meta.location,
                is_dir: false,
                size: meta.size,
                last_modified: meta.last_modified,
            });
        }

        tracing::info!(object_count = objects.len(), "GCS list_objects complete");
        Ok(objects)
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name, bucket = %bucket, prefix = ?prefix))]
    async fn list_objects_recursive(
        &self,
        bucket: &str,
        prefix: Option<&Path>,
    ) -> Result<Vec<Object>> {
        let store = self.create_store(bucket)?;
        let mut objects = Vec::new();
        let mut stream = store.list(prefix);
        while let Some(meta) = stream.next().await {
            let meta = meta?;
            if meta.size == 0 {
                continue;
            }
            objects.push(Object {
                path: meta.location,
                is_dir: false,
                size: meta.size,
                last_modified: meta.last_modified,
            });
        }
        tracing::info!(
            object_count = objects.len(),
            "GCS list_objects_recursive complete"
        );
        Ok(objects)
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name, bucket = %bucket, path = %path, max_bytes))]
    async fn get_object_preview(
        &self,
        bucket: &str,
        path: &Path,
        max_bytes: usize,
    ) -> Result<bytes::Bytes> {
        let store = self.create_store(bucket)?;
        let options = object_store::GetOptions {
            range: Some((0..max_bytes as u64).into()),
            ..Default::default()
        };
        let result = store.get_opts(path, options).await?;
        let bytes = result.bytes().await?;
        tracing::debug!(byte_count = bytes.len(), "GCS preview fetched");
        Ok(bytes)
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name, bucket = %bucket, path = %path, local_path = %local_path.display()))]
    async fn download(
        &self,
        bucket: &str,
        path: &Path,
        local_path: &std::path::Path,
        progress_cb: ProgressCallback,
    ) -> Result<()> {
        tracing::info!("GCS download started");
        let store = self.create_store(bucket)?;
        let result = store.get(path).await?;

        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file = tokio::fs::File::create(local_path).await?;
        let mut stream = result.into_stream();
        let mut total_bytes: u64 = 0;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            total_bytes += chunk.len() as u64;
            progress_cb(chunk.len() as u64);
        }

        file.flush().await?;
        tracing::info!(bytes = total_bytes, "GCS download complete");
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name, bucket = %bucket, path = %path, local_path = %local_path.display()))]
    async fn upload(
        &self,
        bucket: &str,
        path: &Path,
        local_path: &std::path::Path,
        progress_cb: ProgressCallback,
    ) -> Result<()> {
        tracing::info!("GCS upload started");
        let store = self.create_store(bucket)?;
        let mut file = tokio::fs::File::open(local_path).await?;
        let mut upload = store.put_multipart(path).await?;

        let mut buffer = vec![0u8; 5 * 1024 * 1024]; // 5 MB chunks
        let mut total_bytes: u64 = 0;
        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            let chunk = bytes::Bytes::copy_from_slice(&buffer[..n]);
            upload.put_part(chunk.into()).await?;
            total_bytes += n as u64;
            progress_cb(n as u64);
        }

        upload.complete().await?;
        tracing::info!(bytes = total_bytes, "GCS upload complete");
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name, bucket = %bucket, path = %path))]
    async fn create_folder(&self, bucket: &str, path: &Path) -> Result<()> {
        // We must talk to the GCS JSON upload API directly here:
        // `object_store::path::Path` normalizes paths and strips the trailing
        // slash that distinguishes a folder marker from a regular object.
        let token = self.get_access_token().await?;
        let mut path_str = path.to_string();
        if !path_str.ends_with('/') {
            path_str.push('/');
        }

        let url = format!(
            "https://storage.googleapis.com/upload/storage/v1/b/{}/o",
            bucket
        );

        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&token)
            .query(&[("uploadType", "media"), ("name", path_str.as_str())])
            .header("Content-Length", "0")
            .body(Vec::<u8>::new())
            .send()
            .await
            .context("Failed to create folder in GCS")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("GCS folder creation failed ({}): {}", status, body);
        }

        tracing::info!("GCS create_folder complete");
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name, bucket = %bucket, target_count = paths.len()))]
    async fn delete_objects(&self, bucket: &str, paths: &[Path]) -> Result<()> {
        // We must talk to the GCS JSON API directly here for the same reason
        // as `create_folder`: `object_store::path::Path` strips trailing
        // slashes, so going through `store.delete` / `store.list` cannot
        // address folder-marker keys (`foo/`) at all — listing returns the
        // marker stripped to `foo`, and deleting `foo` then 404s.
        tracing::info!("GCS delete started");
        let token = self.get_access_token().await?;
        let mut deleted_keys: u64 = 0;

        for path in paths {
            let path_str = path.to_string();

            // 1. Delete the target itself, both with and without trailing
            //    slash (we don't know if it's a file or a folder marker).
            let _ = self.gcs_delete_object(&token, bucket, &path_str).await;

            let folder_marker = if path_str.ends_with('/') {
                path_str.clone()
            } else {
                format!("{}/", path_str)
            };
            let _ = self.gcs_delete_object(&token, bucket, &folder_marker).await;

            // 2. Recursively delete everything under the prefix.
            let mut page_token: Option<String> = None;
            loop {
                let mut query: Vec<(&str, String)> = vec![
                    ("prefix", folder_marker.clone()),
                    ("maxResults", "1000".to_string()),
                ];
                if let Some(t) = &page_token {
                    query.push(("pageToken", t.clone()));
                }

                let url = format!("https://storage.googleapis.com/storage/v1/b/{}/o", bucket);
                let resp = self
                    .http_client
                    .get(&url)
                    .bearer_auth(&token)
                    .query(&query)
                    .send()
                    .await
                    .context("Failed to list objects for deletion")?;

                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .context("Failed to read object list response body")?;

                if !status.is_success() {
                    bail!("GCS list for deletion failed ({}): {}", status, body);
                }

                let list: ObjectListResponse = serde_json::from_str(&body).with_context(|| {
                    format!(
                        "Failed to parse object list response (status {}, body: {})",
                        status, body
                    )
                })?;

                for item in list.items {
                    self.gcs_delete_object(&token, bucket, &item.name)
                        .await
                        .context("Failed to delete object in GCS")?;
                    deleted_keys += 1;
                }

                match list.next_page_token {
                    Some(t) if !t.is_empty() => page_token = Some(t),
                    _ => break,
                }
            }
        }

        tracing::info!(deleted_keys, "GCS delete complete");
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn StorageBackend> {
        Box::new(self.clone())
    }
}
