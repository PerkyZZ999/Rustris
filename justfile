# Commands run from the workspace root; build prerequisites are in README.md.
build:
    cargo build --workspace --locked

run: build
    cargo run -p rustris-desktop --locked

check:
    bash tests/check.sh

release:
    cargo build --release --workspace --locked
