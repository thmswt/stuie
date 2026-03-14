use anyhow::Result;
use object_store::path::Path;
use async_trait::async_trait;
use std::sync::Arc;

use crate::config::ProviderType;

pub mod s3;
pub mod gcs;
pub mod azure;

#[derive(Debug, Clone)]
pub struct Bucket {
    pub name: String,
    pub provider_index: usize,
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
    async fn get_object_preview(&self, bucket: &str, path: &Path, max_bytes: usize) -> Result<bytes::Bytes>;
    async fn download(&self, bucket: &str, path: &Path, local_path: &std::path::Path, progress_cb: ProgressCallback) -> Result<()>;
    async fn upload(&self, bucket: &str, path: &Path, local_path: &std::path::Path, progress_cb: ProgressCallback) -> Result<()>;
    async fn delete_objects(&self, bucket: &str, paths: &[Path]) -> Result<()>;
    fn clone_box(&self) -> Box<dyn StorageBackend>;
}
