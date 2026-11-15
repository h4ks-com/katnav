default:
    @just --list

# clippy with ci's denials, after every change
check:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

test:
    cargo test

# the gate every commit passes
ci: fmt-check check test

# open a page, like `just run example.com`
run url="https://example.com":
    cargo run --release -- {{url}}

# install the katnav binary from this checkout
install:
    cargo install --path .
