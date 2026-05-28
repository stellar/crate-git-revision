all: check test

export RUSTFLAGS=-Dwarnings

doc: fmt
	cargo test --doc
	cargo doc --open

test: fmt
	cargo test

check: fmt
	cargo check

watch:
	cargo watch --clear --watch-when-idle --shell '$(MAKE)'

watch-doc:
	cargo +nightly watch --clear --watch-when-idle --shell '$(MAKE) doc'

fmt:
	cargo fmt --all

readme:
	cargo +nightly rustdoc -- -Zunstable-options -wjson
	jq -r '.index[.root|tostring].docs' target/doc/crate_git_revision.json > README.md

clean:
	cargo clean
