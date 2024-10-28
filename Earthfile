VERSION 0.8
IMPORT github.com/earthly/lib/rust:3.0.3 AS rust

env:
    FROM rustlang/rust:nightly-slim
    WORKDIR /app
    ENV CARGO_TERM_COLOR=always
    DO rust+INIT --keep-fingerprints=true

COPY_SOURCE:
    FUNCTION
    COPY --keep-ts Cargo.toml Cargo.lock README.md ./
    COPY --keep-ts --dir arcane  ./

test:
    FROM +env
    RUN cargo install cargo-nextest
    DO +COPY_SOURCE
    DO rust+CARGO --args="nextest run --all-features"

lint:
    FROM +env
    RUN rustup component add clippy
    RUN cargo install cargo-deny
    COPY --keep-ts deny.toml ./
    DO +COPY_SOURCE
    DO rust+CARGO --args="deny check"
    DO rust+CARGO --args="clippy --all-features -- -Dwarnings"

ci:
    BUILD +test
    BUILD +lint
    BUILD +build

docs:
    FROM +env
    DO +COPY_SOURCE
    DO rust+CARGO --args="doc" --output "doc/.*"
    SAVE ARTIFACT ./target/doc docs AS LOCAL "./artifacts/docs"

build:
    FROM +env

    ARG release=true

    DO +COPY_SOURCE

    IF $release
        DO rust+CARGO --args="build --release -p arcane" --output="release/arcane"
        SAVE ARTIFACT ./target/release/arcane arcane AS LOCAL "./artifacts/bin/arcane"
    ELSE
        DO rust+CARGO --args="build -p arcane" --output="debug/arcane"
        SAVE ARTIFACT ./target/debug/arcane arcane AS LOCAL "./artifacts/bin/arcane"
    END

run:
    FROM debian
    WORKDIR /app
    COPY +build/arcane ./artifacts/arcane
    RUN --interactive-keep ARCANE_LOG=log.txt ./artifacts/arcane
    SAVE ARTIFACT log.txt log AS LOCAL "./artifacts/log.txt"
