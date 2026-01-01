# sv Makefile

.PHONY: build release install uninstall test test-all lint clean help

# Default target
all: build

# Build debug binary
build:
	cargo build

# Build optimized release binary
release:
	cargo build --release

# Install globally via cargo
install:
	cargo install --path .

# Uninstall
uninstall:
	cargo uninstall sv

# Run library tests
test:
	cargo test --lib

# Run all tests (lib + integration)
test-all:
	cargo test

# Run lints and checks
lint:
	cargo clippy -- -D warnings
	cargo fmt --check

# Format code
fmt:
	cargo fmt

# Clean build artifacts
clean:
	cargo clean

# Show help
help:
	@echo "sv Makefile targets:"
	@echo "  build     - Build debug binary"
	@echo "  release   - Build optimized release binary"
	@echo "  install   - Install sv globally (~/.cargo/bin/)"
	@echo "  uninstall - Remove global installation"
	@echo "  test      - Run library tests"
	@echo "  test-all  - Run all tests (lib + integration)"
	@echo "  lint      - Run clippy and format check"
	@echo "  fmt       - Format code with rustfmt"
	@echo "  clean     - Remove build artifacts"
	@echo "  help      - Show this help"
