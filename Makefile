SHELL = /bin/bash

export DOCKER_DEFAULT_PLATFORM := linux/amd64

.PHONY: start-minio setup-gpg test build-with-docker install-musl-target build-musl

start-minio:
	docker run -p 9001:9000 -i --rm   \
		-e MINIO_ROOT_USER=test         \
		-e MINIO_ROOT_PASSWORD=test1234 \
		-e MINIO_DOMAIN=localhost       \
		--name git_remote_s3_minio      \
		minio/minio server /home/shared \
		# END

setup-gpg:
	@if ! gpg --fingerprint --with-colons 'test@example.com' | grep "example.com" > /dev/null; then \
		echo "Generating GPG key for test@example.com"; \
		gpg --verbose --batch --gen-key <(cat <<-EOF \
			%echo Generating a basic OpenPGP key \
			Key-Type: RSA \
			Key-Length: 2048 \
			Subkey-Type: RSA \
			Subkey-Length: 2048 \
			Name-Real: Test User \
			Name-Comment: Test User \
			Name-Email: test@example.com \
			Expire-Date: 0 \
			%no-ask-passphrase \
			%no-protection \
			%commit \
			%echo done \
			EOF \
		); \
		gpg --list-secret-keys; \
		gpg -v --batch -r test@example.com -o /tmp/enc-test.out -e Makefile; \
	else \
		echo "GPG key for test@example.com already exists"; \
	fi

test: setup-gpg
	RUST_BACKTRACE=full cargo test


bootstrap-rustup:
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

bootstrap-cross:
	cargo install cross --git https://github.com/cross-rs/cross

cross-build-musl:
	cross build --release --target x86_64-unknown-linux-musl
