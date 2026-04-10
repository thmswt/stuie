use anyhow::{Result, Context};
use async_trait::async_trait;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt};
use crate::config::{ProviderConfig, ProviderType};
use crate::storage::{StorageBackend, Bucket, Object, ProgressCallback};
use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone)]
pub struct S3Storage {
    config: ProviderConfig,
    provider_index: usize,
    client: aws_sdk_s3::Client,
}

impl S3Storage {
    pub async fn new(config: ProviderConfig, provider_index: usize) -> Result<Self> {
        let mut sdk_config_builder = aws_config::defaults(aws_config::BehaviorVersion::latest());

        if let Some(region) = &config.region {
            sdk_config_builder = sdk_config_builder.region(aws_sdk_s3::config::Region::new(region.clone()));
        }

        if let (Some(access_key), Some(secret_key)) = (&config.access_key_id, &config.secret_access_key) {
            sdk_config_builder = sdk_config_builder.credentials_provider(
                aws_sdk_s3::config::Credentials::new(
                    access_key,
                    secret_key,
                    None,
                    None,
                    "manual"
                )
            );
        }

        if let Some(endpoint) = &config.endpoint {
            sdk_config_builder = sdk_config_builder.endpoint_url(endpoint);
        }

        let sdk_config = sdk_config_builder.load().await;
        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);

        if config.allow_http.unwrap_or(false) {
            // MinIO usually needs this
            s3_config_builder = s3_config_builder.force_path_style(true);
        }

        let client = aws_sdk_s3::Client::from_conf(s3_config_builder.build());

        Ok(Self {
            config,
            provider_index,
            client,
        })
    }

    fn create_store(&self, bucket: &str) -> Result<Box<dyn ObjectStore>> {
        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(bucket);

        if let Some(region) = &self.config.region {
            builder = builder.with_region(region);
        }

        if let Some(endpoint) = &self.config.endpoint {
            builder = builder.with_endpoint(endpoint);
        }

        if let Some(access_key) = &self.config.access_key_id {
            builder = builder.with_access_key_id(access_key);
        }

        if let Some(secret_key) = &self.config.secret_access_key {
            builder = builder.with_secret_access_key(secret_key);
        }

        if self.config.allow_http.unwrap_or(false) {
            builder = builder.with_allow_http(true);
        }

        let store = builder.build().context("Failed to build S3 object store")?;

        Ok(Box::new(store))
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::S3
    }

    async fn list_buckets(&self) -> Result<Vec<Bucket>> {
        if let Some(bucket_name) = &self.config.bucket_name {
            if !bucket_name.trim().is_empty() {
                return Ok(vec![Bucket {
                    name: bucket_name.clone(),
                    provider_index: self.provider_index,
                }]);
            }
        }

        let resp = self.client.list_buckets().send().await.context("Failed to list buckets from S3")?;
        let buckets = resp.buckets.unwrap_or_default()
            .into_iter()
            .filter_map(|b| {
                let name = b.name?;
                if name.trim().is_empty() {
                    None
                } else {
                    Some(Bucket {
                        name,
                        provider_index: self.provider_index,
                    })
                }
            })
            .collect();
        Ok(buckets)
    }

    async fn list_objects(&self, bucket: &str, prefix: Option<&Path>) -> Result<Vec<Object>> {
        let mut objects = Vec::new();
        let store = self.create_store(bucket)?;

        // Use list_with_delimiter to get both objects and common prefixes (folders)
        let list_result = store.list_with_delimiter(prefix).await?;

        // Folders
        for common_prefix in list_result.common_prefixes {
            // Filter out the directory itself if it matches the prefix
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

        // Files
        for meta in list_result.objects {
            // Filter out the directory itself if it matches the prefix
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

        Ok(objects)
    }

    async fn list_objects_recursive(&self, bucket: &str, prefix: Option<&Path>) -> Result<Vec<Object>> {
        let store = self.create_store(bucket)?;
        let mut objects = Vec::new();
        let mut stream = store.list(prefix);
        while let Some(meta) = stream.next().await {
            let meta = meta?;
            // Skip zero-byte folder marker objects
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
        Ok(objects)
    }

    async fn get_object_preview(&self, bucket: &str, path: &Path, max_bytes: usize) -> Result<bytes::Bytes> {
        let store = self.create_store(bucket)?;
        let options = object_store::GetOptions {
            range: Some((0..max_bytes as u64).into()),
            ..Default::default()
        };
        let result = store.get_opts(path, options).await?;
        let bytes = result.bytes().await?;
        Ok(bytes)
    }

    async fn download(&self, bucket: &str, path: &Path, local_path: &std::path::Path, progress_cb: ProgressCallback) -> Result<()> {
        let store = self.create_store(bucket)?;
        let result = store.get(path).await?;

        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file = tokio::fs::File::create(local_path).await?;
        let mut stream = result.into_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            progress_cb(chunk.len() as u64);
        }

        file.flush().await?;
        Ok(())
    }

    async fn upload(&self, bucket: &str, path: &Path, local_path: &std::path::Path, progress_cb: ProgressCallback) -> Result<()> {
        let store = self.create_store(bucket)?;
        let mut file = tokio::fs::File::open(local_path).await?;
        let mut upload = store.put_multipart(path).await?;

        let mut buffer = vec![0u8; 5 * 1024 * 1024]; // 5MB chunks
        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 { break; }
            let chunk = bytes::Bytes::copy_from_slice(&buffer[..n]);
            upload.put_part(chunk.into()).await?;
            progress_cb(n as u64);
        }

        upload.complete().await?;
        Ok(())
    }

    async fn create_folder(&self, bucket: &str, path: &Path) -> Result<()> {
        // Object stores don't have real folders, we create a zero-byte object with a trailing slash
        // Ensure path ends with / if it doesn't already
        let mut path_str = path.to_string();
        if !path_str.ends_with('/') {
            path_str.push('/');
        }
        
        self.client.put_object()
            .bucket(bucket)
            .key(path_str)
            .body(aws_sdk_s3::primitives::ByteStream::from_static(b""))
            .send()
            .await
            .context("Failed to create folder using S3 SDK")?;

        Ok(())
    }

    async fn delete_objects(&self, bucket: &str, paths: &[Path]) -> Result<()> {
        for path in paths {
            let path_str = path.to_string();
            
            // We use the S3 SDK directly to handle recursive deletion to ensure we 
            // correctly handle keys with trailing slashes, which Path might strip.
            
            // 1. Delete the object or prefix-as-folder-marker
            // Try both with and without trailing slash for the target itself
            let _ = self.client.delete_object()
                .bucket(bucket)
                .key(&path_str)
                .send()
                .await;
                
            let folder_marker = if path_str.ends_with('/') {
                path_str.clone()
            } else {
                format!("{}/", path_str)
            };
            
            let _ = self.client.delete_object()
                .bucket(bucket)
                .key(&folder_marker)
                .send()
                .await;

            // 2. Recursively delete everything with the prefix
            // Use list_objects_v2 to get all keys (including those with slashes)
            let mut continuation_token = None;
            loop {
                let mut request = self.client.list_objects_v2()
                    .bucket(bucket)
                    .prefix(&folder_marker);
                
                if let Some(token) = continuation_token {
                    request = request.continuation_token(token);
                }
                
                let output = request.send().await.context("Failed to list objects for deletion")?;
                
                if let Some(contents) = output.contents {
                    for object in contents {
                        if let Some(key) = object.key {
                            self.client.delete_object()
                                .bucket(bucket)
                                .key(key)
                                .send()
                                .await
                                .context("Failed to delete object")?;
                        }
                    }
                }
                
                if output.next_continuation_token.is_some() {
                    continuation_token = output.next_continuation_token;
                } else {
                    break;
                }
            }
        }
        
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn StorageBackend> {
        Box::new(self.clone())
    }
}
