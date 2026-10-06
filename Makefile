fmt:
	@echo 'cargo +nightly fmt'
	@script -q -c 'cargo +nightly fmt' /dev/null

release:
	cargo build --bin jbot --release --features 'roll merge spoiler group_config verify bili douyin twitter youtube soutu'

r: release
