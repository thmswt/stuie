#!/bin/bash
set -e

echo "Starting MinIO..."
docker run -d --name stuie-minio -p 9000:9000 -p 9001:9001 \
  -e "MINIO_ROOT_USER=minioadmin" \
  -e "MINIO_ROOT_PASSWORD=minioadmin" \
  minio/minio server /data --console-address ":9001"

echo "Waiting for MinIO to start..."
until curl -s http://localhost:9000/minio/health/live; do
  sleep 1
done

echo "MinIO is up! Access Console at http://localhost:9001"
echo "Creating test bucket 'test-bucket'..."

# Use mc (MinIO client) to create bucket and upload some test files
# We can use docker to run mc if not installed locally
docker run --rm --net=host -e "MC_HOST_minio=http://minioadmin:minioadmin@localhost:9000" \
  minio/mc mb minio/test-bucket
docker run --rm --net=host -e "MC_HOST_minio=http://minioadmin:minioadmin@localhost:9000" \
  minio/mc cp /etc/hosts minio/test-bucket/test.txt
docker run --rm --net=host -e "MC_HOST_minio=http://minioadmin:minioadmin@localhost:9000" \
  minio/mc mb minio/test-bucket/folder1
docker run --rm --net=host -e "MC_HOST_minio=http://minioadmin:minioadmin@localhost:9000" \
  minio/mc cp /etc/hosts minio/test-bucket/folder1/inner.txt

echo "Setup complete."
