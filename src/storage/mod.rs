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
    async fn list_buckets(&self) -> Result<Vec<Bucket>>;
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
