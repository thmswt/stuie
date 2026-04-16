use stuie::storage::s3::S3Storage;
use stuie::storage::StorageBackend;
use stuie::config::{ProviderConfig, ProviderType};
use object_store::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[tokio::test]
async fn test_s3_provider_with_minio() {
    let config = ProviderConfig {
        name: "MinIO Test".to_string(),
        provider_type: ProviderType::S3,
        credentials_path: None,
        region: Some("us-east-1".to_string()),
        bucket_name: Some("test-bucket".to_string()),
        endpoint: Some("http://localhost:9000".to_string()),
        access_key_id: Some("minioadmin".to_string()),
        secret_access_key: Some("minioadmin".to_string()),
        allow_http: Some(true),
        project: None,
        enabled: true,
    };

    let storage = S3Storage::new(config, 0).await.expect("Failed to create S3 storage");

    // List buckets
    let buckets = storage.list_buckets().await.expect("Failed to list buckets");
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].name, "test-bucket");

    // List objects in root
    let objects = storage.list_objects("test-bucket", None).await.expect("Failed to list objects in root");
    
    // We expect test.txt and folder1/
    assert!(objects.iter().any(|o| o.path.to_string() == "test.txt"));
    assert!(objects.iter().any(|o| o.path.to_string() == "folder1" && o.is_dir));

    // List objects in folder1
    let folder1_path = Path::from("folder1");
    let objects_in_folder1 = storage.list_objects("test-bucket", Some(&folder1_path)).await.expect("Failed to list objects in folder1");
    
    // We expect inner.txt
    assert!(objects_in_folder1.iter().any(|o| o.path.to_string() == "folder1/inner.txt"));

    // Test preview
    let test_txt_path = Path::from("test.txt");
    let preview = storage.get_object_preview("test-bucket", &test_txt_path, 10).await.expect("Failed to get preview");
    assert!(!preview.is_empty());
    assert!(preview.len() <= 10);
}

#[tokio::test]
async fn test_s3_transfer() {
    let config = ProviderConfig {
        name: "MinIO Transfer Test".to_string(),
        provider_type: ProviderType::S3,
        credentials_path: None,
        region: Some("us-east-1".to_string()),
        bucket_name: Some("test-bucket".to_string()),
        endpoint: Some("http://localhost:9000".to_string()),
        access_key_id: Some("minioadmin".to_string()),
        secret_access_key: Some("minioadmin".to_string()),
        allow_http: Some(true),
        project: None,
        enabled: true,
    };

    let storage = S3Storage::new(config, 0).await.expect("Failed to create S3 storage");
    let bucket = "test-bucket";
    let remote_path = Path::from("upload_test.txt");
    let content = b"Hello, Stuie Transfer!";
    
    // 1. Create a local file to upload
    let temp_dir = std::env::temp_dir();
    let upload_local_path = temp_dir.join("stuie_upload_test.txt");
    tokio::fs::write(&upload_local_path, content).await.expect("Failed to write temp upload file");

    // 2. Upload
    let uploaded_bytes = Arc::new(AtomicU64::new(0));
    let up_bytes_clone = uploaded_bytes.clone();
    let progress_cb: Arc<dyn Fn(u64) + Send + Sync> = Arc::new(move |n| {
        up_bytes_clone.fetch_add(n, Ordering::SeqCst);
    });

    storage.upload(bucket, &remote_path, &upload_local_path, progress_cb).await.expect("Failed to upload");
    assert_eq!(uploaded_bytes.load(Ordering::SeqCst), content.len() as u64);

    // 3. Verify it exists
    let objects = storage.list_objects(bucket, None).await.expect("Failed to list objects");
    assert!(objects.iter().any(|o| o.path == remote_path));

    // 4. Download
    let download_local_path = temp_dir.join("stuie_download_test.txt");
    let downloaded_bytes = Arc::new(AtomicU64::new(0));
    let down_bytes_clone = downloaded_bytes.clone();
    let progress_cb_down: Arc<dyn Fn(u64) + Send + Sync> = Arc::new(move |n| {
        down_bytes_clone.fetch_add(n, Ordering::SeqCst);
    });

    storage.download(bucket, &remote_path, &download_local_path, progress_cb_down).await.expect("Failed to download");
    assert_eq!(downloaded_bytes.load(Ordering::SeqCst), content.len() as u64);

    // 5. Verify content
    let downloaded_content = tokio::fs::read(&download_local_path).await.expect("Failed to read downloaded file");
    assert_eq!(downloaded_content, content);

    // Cleanup
    let _ = tokio::fs::remove_file(upload_local_path).await;
    let _ = tokio::fs::remove_file(download_local_path).await;
}

#[tokio::test]
async fn test_s3_provider_list_all_buckets() {
    let config = ProviderConfig {
        name: "MinIO Test All".to_string(),
        provider_type: ProviderType::S3,
        credentials_path: None,
        region: Some("us-east-1".to_string()),
        bucket_name: None,
        endpoint: Some("http://localhost:9000".to_string()),
        access_key_id: Some("minioadmin".to_string()),
        secret_access_key: Some("minioadmin".to_string()),
        allow_http: Some(true),
        project: None,
        enabled: true,
    };

    let storage = S3Storage::new(config, 0).await.expect("Failed to create S3 storage");

    let buckets = storage.list_buckets().await.expect("Failed to list buckets");
    assert!(!buckets.is_empty());
    assert!(buckets.iter().any(|b| b.name == "test-bucket"));
}

#[tokio::test]
async fn test_s3_provider_with_empty_bucket_name() {
    let config = ProviderConfig {
        name: "MinIO Test Empty".to_string(),
        provider_type: ProviderType::S3,
        credentials_path: None,
        region: Some("us-east-1".to_string()),
        bucket_name: Some("".to_string()),
        endpoint: Some("http://localhost:9000".to_string()),
        access_key_id: Some("minioadmin".to_string()),
        secret_access_key: Some("minioadmin".to_string()),
        allow_http: Some(true),
        project: None,
        enabled: true,
    };

    let storage = S3Storage::new(config, 0).await.expect("Failed to create S3 storage");

    let buckets = storage.list_buckets().await.expect("Failed to list buckets");
    assert!(!buckets.is_empty());
    assert!(buckets.iter().any(|b| b.name == "test-bucket"));
    assert!(!buckets.iter().any(|b| b.name.is_empty()));
}
