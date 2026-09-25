fmt:
	@echo 'cargo +nightly fmt'
	@script -q -c 'cargo +nightly fmt' /dev/null

build:
	cargo build --all-features

b: build

release:
	cargo build --bin jbot --release --all-features

r: release
