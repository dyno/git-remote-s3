SHELL = /bin/bash

export DOCKER_DEFAULT_PLATFORM := linux/amd64

start-minio:
	docker run -p 9001:9000 -i --rm   \
		-e MINIO_ROOT_USER=test         \
		-e MINIO_ROOT_PASSWORD=test1234 \
		-e MINIO_DOMAIN=localhost       \
		--name git_remote_s3_minio      \
		minio/minio server /home/shared \
		# END

test:
	RUST_BACKTRACE=full cargo test

build-with-docker:
	docker run -it --rm                      \
    --name git_remote_s3_builder           \
    -v $(shell pwd):/usr/src/git-remote-s3 \
    -w /usr/src/git-remote-s3 rust:1.82    \
    cargo build --release                  \
		# END
