RUSTUP := $(shell command -v rustup 2> /dev/null)
WASMPACK := $(shell command -v wasm-pack 2> /dev/null)

.PHONY: all wasm native setup

all: native wasm
ifndef RUSTUP
		$(error "rustup is not installed; make setup-rustup")
endif
ifndef WASMPACK
		$(error "wasm-pack is not installed; cargo install wasm-pack")
endif

setup-rustup:
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

setup:
ifndef RUSTUP
		$(error "rustup is not installed; make setup-rustup")
endif
	rustup target add wasm32-unknown-unknown

native:
	(cd nash-native-client && cargo build)

wasm:
ifndef WASMPACK
		$(error "wasm-pack is not installed; cargo install wasm-pack")
endif
	(cd nash-web-client && wasm-pack build --release --target bundler)

