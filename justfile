# Show available commands
default:
    @just --list --justfile {{justfile()}}

set shell := ["bash", "-cu"]
set dotenv-load := true

format-rust:
    @cargo fmt --all

lint-rust:
    @cargo clippy --all-targets --all-features --workspace
