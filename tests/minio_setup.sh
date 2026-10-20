#!/bin/bash
set -e

# MinIO no longer publishes its community images to Docker Hub/Quay, so
# use Chainguard's free-tier builds. Override to test another image.
MINIO_IMAGE="${MINIO_IMAGE:-cgr.dev/chainguard/minio:latest}"
MC_IMAGE="${MC_IMAGE:-cgr.dev/chainguard/minio-client:latest}"

# Clean up any leftover container from a previous run so this script
# is idempotent. Safe in CI (no-op on a fresh runner) and saves the
# manual teardown step locally.
docker rm -f stuie-minio >/dev/null 2>&1 || true

echo "Starting MinIO..."
docker run -d --name stuie-minio -p 9000:9000 -p 9001:9001 \
  -e "MINIO_ROOT_USER=minioadmin" \
  -e "MINIO_ROOT_PASSWORD=minioadmin" \
  "$MINIO_IMAGE" server /data --console-address ":9001"

echo "Waiting for MinIO to start..."
until curl -s http://localhost:9000/minio/health/live; do
  sleep 1
done

echo "MinIO is up! Access Console at http://localhost:9001"
echo "Creating test bucket 'test-bucket'..."

# Use mc (MinIO client) to create bucket and upload some test files
# We can use docker to run mc if not installed locally
docker run --rm --net=host -e "MC_HOST_minio=http://minioadmin:minioadmin@localhost:9000" \
  "$MC_IMAGE" mb minio/test-bucket
docker run --rm --net=host -e "MC_HOST_minio=http://minioadmin:minioadmin@localhost:9000" \
  "$MC_IMAGE" cp /etc/hosts minio/test-bucket/test.txt
docker run --rm --net=host -e "MC_HOST_minio=http://minioadmin:minioadmin@localhost:9000" \
  "$MC_IMAGE" mb minio/test-bucket/folder1
docker run --rm --net=host -e "MC_HOST_minio=http://minioadmin:minioadmin@localhost:9000" \
  "$MC_IMAGE" cp /etc/hosts minio/test-bucket/folder1/inner.txt

echo "Setup complete."
