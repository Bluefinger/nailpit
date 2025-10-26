FROM rust:alpine AS builder
RUN apk add build-base musl-dev cmake
RUN rustup target add x86_64-unknown-linux-musl
WORKDIR /nailpit
COPY ./src ./src
COPY ./crates ./crates
COPY ./Cargo.lock .
COPY ./Cargo.toml .
RUN cargo build --target x86_64-unknown-linux-musl --release

FROM alpine:latest AS runtime
WORKDIR /app
RUN apk add --no-cache curl
COPY --from=builder ./nailpit/target/x86_64-unknown-linux-musl/release/nailpit .

ENTRYPOINT ["./app/nailpit"]
