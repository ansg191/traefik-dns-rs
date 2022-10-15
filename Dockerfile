FROM rust:1-alpine3.16 as builder

ENV RUSTFLAGS="-C target-feature=-crt-static"

RUN apk add --no-cache musl-dev

WORKDIR /app
RUN cargo init

COPY Cargo.toml ./Cargo.toml

RUN cargo build --release
RUN rm -rf src

COPY src/ ./src
RUN touch src/main.rs

RUN cargo build --release --offline
RUN strip target/release/traefik-dns

FROM alpine:3.16
RUN apk add --no-cache libgcc

COPY --from=builder /app/target/release/traefik-dns .

ENTRYPOINT [ "/traefik-dns" ]
