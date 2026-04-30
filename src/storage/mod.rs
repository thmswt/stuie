use anyhow::Result;
use async_trait::async_trait;
use object_store::path::Path;
use std::sync::Arc;

use crate::config::ProviderType;

pub mod azure;
pub mod gcs;
pub mod s3;

#[derive(Debug, Clone)]
pub struct Bucket {
    pub name: String,
    pub provider_index: usize,
    pub location: Option<String>,
    /// For multi-project GCS providers, the project this bucket belongs to.
    /// Single-project and non-GCS providers leave this `None`.
    pub project_id: Option<String>,
}

/// A GCP project the user has access to. Only populated by GCS providers in
/// GcloudAdc mode (via Cloud Resource Manager API).
#[derive(Debug, Clone)]
pub struct GcpProject {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Object {
    pub path: Path,
    pub is_dir: bool,
    pub size: u64,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

pub type ProgressCallback = Arc<dyn Fn(u64) + Send + Sync>;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    fn name(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    /// Whether this backend has been put into read-only mode. The base
    /// implementations of every concrete backend return `false`; callers get
    /// a `true` answer only when the backend has been wrapped in
    /// `ReadOnlyStorage`. UI code uses this to badge the provider and to
    /// pre-empt destructive keybindings — but the *guarantee* that writes
    /// are blocked lives in the wrapper itself, not in the UI gate.
    fn is_read_only(&self) -> bool {
        false
    }
    /// Enumerate projects/accounts this backend can list buckets under.
    /// Only GCS in GcloudAdc mode returns a meaningful list; all other
    /// backends return an empty `Vec` by default.
    async fn list_projects(&self) -> Result<Vec<GcpProject>> {
        Ok(Vec::new())
    }
    async fn list_buckets(&self) -> Result<Vec<Bucket>>;
    /// Fetch buckets under a specific project. Only meaningful for backends
    /// that expose a project concept (GCS); other backends bail by default.
    async fn list_buckets_for_project(&self, project_id: &str) -> Result<Vec<Bucket>> {
        let _ = project_id;
        anyhow::bail!("Per-project bucket listing is not supported for this provider.")
    }
    async fn list_objects(&self, bucket: &str, prefix: Option<&Path>) -> Result<Vec<Object>>;
    async fn list_objects_recursive(
        &self,
        bucket: &str,
        prefix: Option<&Path>,
    ) -> Result<Vec<Object>>;
    async fn get_object_preview(
        &self,
        bucket: &str,
        path: &Path,
        max_bytes: usize,
    ) -> Result<bytes::Bytes>;
    async fn download(
        &self,
        bucket: &str,
        path: &Path,
        local_path: &std::path::Path,
        progress_cb: ProgressCallback,
    ) -> Result<()>;
    async fn upload(
        &self,
        bucket: &str,
        path: &Path,
        local_path: &std::path::Path,
        progress_cb: ProgressCallback,
    ) -> Result<()>;
    async fn create_folder(&self, bucket: &str, path: &Path) -> Result<()>;
    async fn delete_objects(&self, bucket: &str, paths: &[Path]) -> Result<()>;
    fn clone_box(&self) -> Box<dyn StorageBackend>;
}

/// Decorator that wraps any `StorageBackend` and hard-blocks every mutating
/// operation. Read paths (list/preview/download) pass through unchanged.
/// This is the single chokepoint that guarantees a provider marked
/// `read_only` in config cannot perform destructive operations, regardless
/// of which UI path or future code calls into it.
pub struct ReadOnlyStorage {
    inner: Box<dyn StorageBackend>,
}

impl ReadOnlyStorage {
    pub fn new(inner: Box<dyn StorageBackend>) -> Self {
        Self { inner }
    }

    /// The error message used for every blocked write. Kept as an associated
    /// constant so tests can assert against it without duplicating the
    /// wording.
    pub const BLOCKED_MESSAGE_PREFIX: &'static str = "Provider is read-only";
}

#[async_trait]
impl StorageBackend for ReadOnlyStorage {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn provider_type(&self) -> ProviderType {
        self.inner.provider_type()
    }
    fn is_read_only(&self) -> bool {
        true
    }

    async fn list_projects(&self) -> Result<Vec<GcpProject>> {
        self.inner.list_projects().await
    }
    async fn list_buckets(&self) -> Result<Vec<Bucket>> {
        self.inner.list_buckets().await
    }
    async fn list_buckets_for_project(&self, project_id: &str) -> Result<Vec<Bucket>> {
        self.inner.list_buckets_for_project(project_id).await
    }
    async fn list_objects(&self, bucket: &str, prefix: Option<&Path>) -> Result<Vec<Object>> {
        self.inner.list_objects(bucket, prefix).await
    }
    async fn list_objects_recursive(
        &self,
        bucket: &str,
        prefix: Option<&Path>,
    ) -> Result<Vec<Object>> {
        self.inner.list_objects_recursive(bucket, prefix).await
    }
    async fn get_object_preview(
        &self,
        bucket: &str,
        path: &Path,
        max_bytes: usize,
    ) -> Result<bytes::Bytes> {
        self.inner.get_object_preview(bucket, path, max_bytes).await
    }
    async fn download(
        &self,
        bucket: &str,
        path: &Path,
        local_path: &std::path::Path,
        progress_cb: ProgressCallback,
    ) -> Result<()> {
        self.inner
            .download(bucket, path, local_path, progress_cb)
            .await
    }

    async fn upload(
        &self,
        _bucket: &str,
        _path: &Path,
        _local_path: &std::path::Path,
        _progress_cb: ProgressCallback,
    ) -> Result<()> {
        anyhow::bail!(
            "{}: '{}' — upload blocked",
            Self::BLOCKED_MESSAGE_PREFIX,
            self.inner.name()
        )
    }
    async fn create_folder(&self, _bucket: &str, _path: &Path) -> Result<()> {
        anyhow::bail!(
            "{}: '{}' — folder creation blocked",
            Self::BLOCKED_MESSAGE_PREFIX,
            self.inner.name()
        )
    }
    async fn delete_objects(&self, _bucket: &str, _paths: &[Path]) -> Result<()> {
        anyhow::bail!(
            "{}: '{}' — delete blocked",
            Self::BLOCKED_MESSAGE_PREFIX,
            self.inner.name()
        )
    }

    fn clone_box(&self) -> Box<dyn StorageBackend> {
        Box::new(ReadOnlyStorage {
            inner: self.inner.clone_box(),
        })
    }
}
