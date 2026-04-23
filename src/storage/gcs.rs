use crate::config::{ProviderConfig, ProviderType};
use crate::storage::{Bucket, Object, ProgressCallback, StorageBackend};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures::StreamExt;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

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
// GCS bucket-list API response
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct BucketListResponse {
    #[serde(default)]
    items: Vec<BucketItem>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
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
        let resolved_path = config
            .credentials_path
            .as_ref()
            .filter(|p| !p.trim().is_empty())
            .cloned()
            .or_else(|| std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok())
            .or_else(|| {
                let default =
                    dirs::home_dir()?.join(".config/gcloud/application_default_credentials.json");
                if default.exists() {
                    Some(default.to_string_lossy().to_string())
                } else {
                    None
                }
            });

        let resolved_path = resolved_path.context(
            "No GCS credentials found. Provide a Credentials Path in the provider config, \
             set the GOOGLE_APPLICATION_CREDENTIALS env var, or run \
             'gcloud auth application-default login'.",
        )?;

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
            "scope": "https://www.googleapis.com/auth/cloud-platform",
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

        // For service-account credentials pass the file explicitly so
        // object_store can handle its own token lifecycle independently.
        if let GcsCredentials::ServiceAccount { .. } = self.credentials.as_ref() {
            if let Some(path) = &self.credentials_path {
                builder = builder.with_service_account_path(path);
            }
        }
        // For authorized-user (ADC), object_store picks up credentials from
        // the default ADC location automatically.

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
        if let GcsCredentials::ServiceAccount { project_id, .. } = self.credentials.as_ref() {
            if let Some(pid) = project_id {
                return Ok(pid.clone());
            }
        }
        bail!(
            "No GCS project configured. Set the Project field in the provider config \
             or use a service account key that contains a project_id."
        )
    }
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
    async fn list_buckets(&self) -> Result<Vec<Bucket>> {
        // If a specific bucket is configured, skip the API call.
        if let Some(bucket_name) = &self.config.bucket_name {
            if !bucket_name.trim().is_empty() {
                tracing::info!(bucket = %bucket_name, "Using configured bucket; skipping GCS bucket listing");
                return Ok(vec![Bucket {
                    name: bucket_name.clone(),
                    provider_index: self.provider_index,
                    location: None,
                }]);
            }
        }

        let project = self.get_project()?;
        let token = self.get_access_token().await?;

        tracing::info!(
            provider = %self.config.name,
            project = %project,
            "Listing GCS buckets"
        );

        let resp = self
            .http_client
            .get("https://storage.googleapis.com/storage/v1/b")
            .bearer_auth(&token)
            .query(&[("project", project.as_str())])
            .send()
            .await
            .context("Failed to list GCS buckets")?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .context("Failed to read bucket list response body")?;

        if !status.is_success() {
            bail!("GCS bucket listing failed ({}): {}", status, body);
        }

        tracing::info!(
            provider = %self.config.name,
            project = %project,
            status = %status,
            body_len = body.len(),
            "GCS bucket listing response received"
        );

        let list: BucketListResponse = serde_json::from_str(&body).with_context(|| {
            format!(
                "Failed to parse bucket list response (status {}, body: {})",
                status, body
            )
        })?;

        tracing::info!(
            provider = %self.config.name,
            project = %project,
            item_count = list.items.len(),
            has_next_page = list.next_page_token.is_some(),
            "GCS bucket listing parsed"
        );

        if list.items.is_empty() {
            tracing::warn!(
                provider = %self.config.name,
                project = %project,
                "GCS returned an empty bucket list. Raw response body: {}",
                body
            );
        }

        let buckets = list
            .items
            .into_iter()
            .map(|b| {
                tracing::debug!(
                    provider = %self.config.name,
                    bucket = %b.name,
                    location = ?b.location,
                    "GCS bucket"
                );
                Bucket {
                    name: b.name,
                    provider_index: self.provider_index,
                    location: b.location,
                }
            })
            .collect();

        Ok(buckets)
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
                size: meta.size as u64,
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
                size: meta.size as u64,
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
        let store = self.create_store(bucket)?;
        let mut path_str = path.to_string();
        if !path_str.ends_with('/') {
            path_str.push('/');
        }
        let folder_path = Path::from(path_str);
        store
            .put(&folder_path, bytes::Bytes::new().into())
            .await
            .context("Failed to create folder in GCS")?;
        tracing::info!("GCS create_folder complete");
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(provider = %self.config.name, bucket = %bucket, target_count = paths.len()))]
    async fn delete_objects(&self, bucket: &str, paths: &[Path]) -> Result<()> {
        tracing::info!("GCS delete started");
        let store = self.create_store(bucket)?;
        let mut deleted_keys: u64 = 0;

        for path in paths {
            let path_str = path.to_string();

            // Delete the object itself.
            let _ = store.delete(path).await;

            // Also try with trailing slash (folder marker).
            let folder_marker = if path_str.ends_with('/') {
                path_str.clone()
            } else {
                format!("{}/", path_str)
            };
            let _ = store.delete(&Path::from(folder_marker.clone())).await;

            // Recursively delete everything under the prefix.
            let prefix = Path::from(folder_marker);
            let mut stream = store.list(Some(&prefix));
            while let Some(meta) = stream.next().await {
                if let Ok(meta) = meta {
                    store
                        .delete(&meta.location)
                        .await
                        .context("Failed to delete object in GCS")?;
                    deleted_keys += 1;
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
