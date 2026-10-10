fmt:
	@echo 'cargo +nightly fmt'
	@script -q -c 'cargo +nightly fmt' /dev/null

release:
	cargo build --release

r: release
