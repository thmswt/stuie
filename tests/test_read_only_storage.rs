use anyhow::Result;
use async_trait::async_trait;
use object_store::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use stuie::config::ProviderType;
use stuie::storage::{
    Bucket, GcpProject, Object, ProgressCallback, ReadOnlyStorage, StorageBackend,
};

/// In-memory fake backend that records every method call and either reports
/// success or returns a sentinel value. The wrapper tests use it to verify
/// that read paths reach the inner backend and write paths do not.
#[derive(Default)]
struct FakeBackend {
    calls_list_buckets: AtomicUsize,
    calls_list_objects: AtomicUsize,
    calls_list_objects_recursive: AtomicUsize,
    calls_preview: AtomicUsize,
    calls_download: AtomicUsize,
    calls_upload: AtomicUsize,
    calls_create_folder: AtomicUsize,
    calls_delete: AtomicUsize,
}

impl FakeBackend {
    fn writes_attempted(&self) -> usize {
        self.calls_upload.load(Ordering::SeqCst)
            + self.calls_create_folder.load(Ordering::SeqCst)
            + self.calls_delete.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl StorageBackend for FakeBackend {
    fn name(&self) -> &str {
        "fake"
    }
    fn provider_type(&self) -> ProviderType {
        ProviderType::S3
    }
    async fn list_projects(&self) -> Result<Vec<GcpProject>> {
        Ok(Vec::new())
    }
    async fn list_buckets(&self) -> Result<Vec<Bucket>> {
        self.calls_list_buckets.fetch_add(1, Ordering::SeqCst);
        Ok(vec![Bucket {
            name: "fake-bucket".into(),
            provider_index: 0,
            location: None,
            project_id: None,
        }])
    }
    async fn list_objects(&self, _bucket: &str, _prefix: Option<&Path>) -> Result<Vec<Object>> {
        self.calls_list_objects.fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    }
    async fn list_objects_recursive(
        &self,
        _bucket: &str,
        _prefix: Option<&Path>,
    ) -> Result<Vec<Object>> {
        self.calls_list_objects_recursive
            .fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    }
    async fn get_object_preview(
        &self,
        _bucket: &str,
        _path: &Path,
        _max_bytes: usize,
    ) -> Result<bytes::Bytes> {
        self.calls_preview.fetch_add(1, Ordering::SeqCst);
        Ok(bytes::Bytes::from_static(b"hello"))
    }
    async fn download(
        &self,
        _bucket: &str,
        _path: &Path,
        _local_path: &std::path::Path,
        _progress_cb: ProgressCallback,
    ) -> Result<()> {
        self.calls_download.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn upload(
        &self,
        _bucket: &str,
        _path: &Path,
        _local_path: &std::path::Path,
        _progress_cb: ProgressCallback,
    ) -> Result<()> {
        self.calls_upload.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn create_folder(&self, _bucket: &str, _path: &Path) -> Result<()> {
        self.calls_create_folder.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn delete_objects(&self, _bucket: &str, _paths: &[Path]) -> Result<()> {
        self.calls_delete.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn clone_box(&self) -> Box<dyn StorageBackend> {
        Box::new(FakeBackend::default())
    }
}

/// Wrap a `FakeBackend` in `ReadOnlyStorage` and keep a second `Arc` reference
/// to the same fake so we can inspect call counts after the wrapper has been
/// boxed away.
fn wrap_readonly() -> (Box<dyn StorageBackend>, Arc<FakeBackend>) {
    let fake = Arc::new(FakeBackend::default());

    // The trait demands an owned `Box<dyn StorageBackend>` for `inner`. To
    // share a handle with the test we use a tiny adapter that defers to the
    // shared Arc.
    struct ArcAdapter(Arc<FakeBackend>);
    #[async_trait]
    impl StorageBackend for ArcAdapter {
        fn name(&self) -> &str {
            self.0.name()
        }
        fn provider_type(&self) -> ProviderType {
            self.0.provider_type()
        }
        async fn list_buckets(&self) -> Result<Vec<Bucket>> {
            self.0.list_buckets().await
        }
        async fn list_objects(&self, b: &str, p: Option<&Path>) -> Result<Vec<Object>> {
            self.0.list_objects(b, p).await
        }
        async fn list_objects_recursive(&self, b: &str, p: Option<&Path>) -> Result<Vec<Object>> {
            self.0.list_objects_recursive(b, p).await
        }
        async fn get_object_preview(&self, b: &str, p: &Path, m: usize) -> Result<bytes::Bytes> {
            self.0.get_object_preview(b, p, m).await
        }
        async fn download(
            &self,
            b: &str,
            p: &Path,
            lp: &std::path::Path,
            cb: ProgressCallback,
        ) -> Result<()> {
            self.0.download(b, p, lp, cb).await
        }
        async fn upload(
            &self,
            b: &str,
            p: &Path,
            lp: &std::path::Path,
            cb: ProgressCallback,
        ) -> Result<()> {
            self.0.upload(b, p, lp, cb).await
        }
        async fn create_folder(&self, b: &str, p: &Path) -> Result<()> {
            self.0.create_folder(b, p).await
        }
        async fn delete_objects(&self, b: &str, ps: &[Path]) -> Result<()> {
            self.0.delete_objects(b, ps).await
        }
        fn clone_box(&self) -> Box<dyn StorageBackend> {
            Box::new(ArcAdapter(self.0.clone()))
        }
    }

    let inner: Box<dyn StorageBackend> = Box::new(ArcAdapter(fake.clone()));
    let wrapped: Box<dyn StorageBackend> = Box::new(ReadOnlyStorage::new(inner));
    (wrapped, fake)
}

fn noop_progress() -> ProgressCallback {
    Arc::new(|_| {})
}

#[tokio::test]
async fn read_only_wrapper_reports_is_read_only_true() {
    let (wrapped, _) = wrap_readonly();
    assert!(wrapped.is_read_only());
}

#[tokio::test]
async fn unwrapped_backend_reports_is_read_only_false() {
    let raw = FakeBackend::default();
    assert!(!raw.is_read_only());
}

#[tokio::test]
async fn read_only_wrapper_passes_through_reads() {
    let (wrapped, fake) = wrap_readonly();

    wrapped.list_buckets().await.expect("list_buckets");
    wrapped
        .list_objects("fake-bucket", None)
        .await
        .expect("list_objects");
    wrapped
        .list_objects_recursive("fake-bucket", None)
        .await
        .expect("list_objects_recursive");
    wrapped
        .get_object_preview("fake-bucket", &Path::from("k"), 16)
        .await
        .expect("preview");
    wrapped
        .download(
            "fake-bucket",
            &Path::from("k"),
            &std::path::Path::new("/tmp/stuie-ro-test.bin"),
            noop_progress(),
        )
        .await
        .expect("download");

    assert_eq!(fake.calls_list_buckets.load(Ordering::SeqCst), 1);
    assert_eq!(fake.calls_list_objects.load(Ordering::SeqCst), 1);
    assert_eq!(fake.calls_list_objects_recursive.load(Ordering::SeqCst), 1);
    assert_eq!(fake.calls_preview.load(Ordering::SeqCst), 1);
    assert_eq!(fake.calls_download.load(Ordering::SeqCst), 1);
    assert_eq!(fake.writes_attempted(), 0);
}

#[tokio::test]
async fn read_only_wrapper_blocks_upload_and_does_not_call_inner() {
    let (wrapped, fake) = wrap_readonly();
    let err = wrapped
        .upload(
            "fake-bucket",
            &Path::from("k"),
            std::path::Path::new("/tmp/no-such-file"),
            noop_progress(),
        )
        .await
        .expect_err("upload must be blocked");
    assert!(
        err.to_string()
            .contains(ReadOnlyStorage::BLOCKED_MESSAGE_PREFIX),
        "error must surface the read-only marker: {err}"
    );
    assert_eq!(fake.calls_upload.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn read_only_wrapper_blocks_create_folder_and_does_not_call_inner() {
    let (wrapped, fake) = wrap_readonly();
    let err = wrapped
        .create_folder("fake-bucket", &Path::from("foo"))
        .await
        .expect_err("create_folder must be blocked");
    assert!(
        err.to_string()
            .contains(ReadOnlyStorage::BLOCKED_MESSAGE_PREFIX),
        "error must surface the read-only marker: {err}"
    );
    assert_eq!(fake.calls_create_folder.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn read_only_wrapper_blocks_delete_and_does_not_call_inner() {
    let (wrapped, fake) = wrap_readonly();
    let err = wrapped
        .delete_objects("fake-bucket", &[Path::from("foo")])
        .await
        .expect_err("delete_objects must be blocked");
    assert!(
        err.to_string()
            .contains(ReadOnlyStorage::BLOCKED_MESSAGE_PREFIX),
        "error must surface the read-only marker: {err}"
    );
    assert_eq!(fake.calls_delete.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn read_only_wrapper_clone_box_preserves_read_only_flag() {
    let (wrapped, _) = wrap_readonly();
    let cloned = wrapped.clone_box();
    assert!(cloned.is_read_only());
    let err = cloned
        .delete_objects("fake-bucket", &[Path::from("foo")])
        .await
        .expect_err("clone must also block writes");
    assert!(
        err.to_string()
            .contains(ReadOnlyStorage::BLOCKED_MESSAGE_PREFIX)
    );
}

#[tokio::test]
async fn read_only_wrapper_preserves_name_and_provider_type() {
    let (wrapped, _) = wrap_readonly();
    assert_eq!(wrapped.name(), "fake");
    assert_eq!(wrapped.provider_type(), ProviderType::S3);
}
