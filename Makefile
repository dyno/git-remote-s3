SHELL = /bin/bash

export DOCKER_DEFAULT_PLATFORM := linux/amd64

.PHONY: start-minio setup-gpg test integration-test build-with-docker install-musl-target build-musl

start-minio:
	docker run -p 9001:9000 -i --rm   \
		-e MINIO_ROOT_USER=test         \
		-e MINIO_ROOT_PASSWORD=test1234 \
		-e MINIO_DOMAIN=localhost       \
		--name git_remote_s3_minio      \
		minio/minio server /home/shared \
		# END

define GEN_KEY_CONF
%echo Generating a basic OpenPGP key
Key-Type: RSA
Key-Length: 2048
Subkey-Type: RSA
Subkey-Length: 2048
Name-Real: Test User
Name-Comment: Test User
Name-Email: test@example.com
Expire-Date: 0
%no-ask-passphrase
%no-protection
%commit
%echo done
endef
export GEN_KEY_CONF

GNUPGHOME := $(PWD)/gnupg
export GNUPGHOME

setup-gpg:
	rm -rf gnupg
	mkdir -p gnupg
	chmod 700 gnupg
	gpg --batch --gen-key <<< "$${GEN_KEY_CONF}"
	gpg --list-secret-keys
	echo "test" > gnupg/test.txt
	gpg -v --batch -r test@example.com -o gnupg/test.enc -e gnupg/test.txt

test:
	RUST_BACKTRACE=full cargo test

integration-test:
	cargo test --test main_test

bootstrap-rustup:
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

bootstrap-cross:
	cargo install cross --git https://github.com/cross-rs/cross

cross-build-x86_64:
	cross build --release --target x86_64-unknown-linux-musl

cross-build-aarch64:
	cross build --release --target aarch64-unknown-linux-musl
