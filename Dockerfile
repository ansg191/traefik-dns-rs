FROM rust:1-alpine3.16 as chef

ENV RUSTFLAGS="-C target-feature=-crt-static"

RUN apk add --no-cache musl-dev && \
    cargo install cargo-chef

WORKDIR /app

FROM chef as planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release
RUN strip target/release/traefik-dns

FROM alpine:3.16
RUN apk add --no-cache libgcc

COPY --from=builder /app/target/release/traefik-dns .

ENTRYPOINT [ "/traefik-dns" ]
