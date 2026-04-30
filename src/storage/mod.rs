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
