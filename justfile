default:
    @just --list

# Build all crates
build:
    cargo build --workspace

# Run tests
test:
    cargo test --workspace --all-features

# Format code
fmt:
    cargo fmt --all

# Lint
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Full pre-PR check
ci: fmt lint test

# Run server in dev mode (single node)
dev:
    RUST_LOG=debug cargo run -p rusty-sched -- server

# Build release binary
release:
    cargo build --release -p rusty-sched
