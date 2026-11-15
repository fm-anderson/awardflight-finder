.PHONY: build test clippy search

build:
	cargo build --manifest-path cli/Cargo.toml

test:
	cargo test --manifest-path cli/Cargo.toml

clippy:
	cargo clippy --manifest-path cli/Cargo.toml -- -D warnings

# Usage: make search FROM=GRU TO=MCO DEPART=2026-12-10 [RETURN=2026-12-20] [MAX_STOPS=1] [CABIN=ALL] [PROGRAMS=all]
search:
	cargo run --manifest-path cli/Cargo.toml -- \
		--from $(FROM) \
		--to $(TO) \
		--depart $(DEPART) \
		$(if $(RETURN),--return $(RETURN),) \
		$(if $(ADULTS),--adults $(ADULTS),) \
		$(if $(CABIN),--cabin $(CABIN),) \
		$(if $(MAX_STOPS),--max-stops $(MAX_STOPS),) \
		$(if $(PROGRAMS),--programs $(PROGRAMS),) \
		--config config.json
