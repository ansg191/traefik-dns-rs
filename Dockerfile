FROM rust:1-alpine3.18 as builder

ENV RUSTFLAGS="-C target-feature=-crt-static"

RUN apk add --no-cache musl-dev

WORKDIR /app

ADD Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release

COPY . .
RUN cargo build --release && \
    strip target/release/traefik-dns

FROM alpine:3.18
RUN apk add --no-cache libgcc

COPY --from=builder /app/target/release/traefik-dns .

ENTRYPOINT [ "/traefik-dns" ]
