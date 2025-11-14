FROM --platform=$BUILDPLATFORM rust:slim AS builder
SHELL ["/bin/bash", "-uo", "pipefail", "-c"]

RUN apt-get update -y && apt-get install -y cmake curl xz-utils tar
ARG ZIG_VERSION=0.15.2
RUN curl -L "https://ziglang.org/download/${ZIG_VERSION}/zig-$(uname -m)-linux-${ZIG_VERSION}.tar.xz" | tar -J -x -C /usr/local && \
    ln -s "/usr/local/zig-$(uname -m)-linux-${ZIG_VERSION}/zig" /usr/local/bin/zig

RUN cargo install --locked cargo-zigbuild

WORKDIR /nailpit
COPY ./src ./src
COPY ./crates ./crates
COPY ./Cargo.lock .
COPY ./Cargo.toml .
RUN cargo fetch --locked

ARG TARGETPLATFORM

# add the rust target for the target architecture
RUN if   [ "$TARGETPLATFORM" == "linux/amd64"  ]; then echo "x86_64-unknown-linux-musl"      >/.target; \
    elif [ "$TARGETPLATFORM" == "linux/arm64"  ]; then echo "aarch64-unknown-linux-musl"     >/.target; \
    elif [ "$TARGETPLATFORM" == "linux/arm/v7" ]; then echo "armv7-unknown-linux-musleabihf" >/.target; \
    elif [ "$TARGETPLATFORM" == "linux/riscv64" ]; then echo "riscv64gc-unknown-linux-musl"  >/.target; \
    else echo "Unsupported architecture $TARGETPLATFORM"; exit 1; \
    fi
RUN rustup target add "$(cat /.target)"

RUN if   [ "$TARGETPLATFORM" == "linux/amd64"  ]; then echo "-C target-cpu=x86-64-v3"      >/.target-cpu; \
    elif [ "$TARGETPLATFORM" == "linux/arm64"  ]; then echo "-C target-cpu=cortex-a53"     >/.target-cpu; \
    elif [ "$TARGETPLATFORM" == "linux/arm/v7" ]; then echo "" >/.target-cpu; \
    elif [ "$TARGETPLATFORM" == "linux/riscv64" ]; then echo ""  >/.target-cpu; \
    else echo "Unsupported architecture $TARGETPLATFORM"; exit 1; \
    fi

RUN RUSTFLAGS="$(cat /.target-cpu)" cargo zigbuild --release --locked --target "$(cat /.target)" && \
    mv ./target/$(cat /.target)/release/nailpit .

FROM gcr.io/distroless/static-debian13:latest AS runtime
WORKDIR /app
COPY ./configuration/pit.default.toml ./configuration/
COPY --from=builder /nailpit/nailpit .
VOLUME /app/configuration
VOLUME /app/input
COPY ./templates/* ./templates/
VOLUME /app/templates

STOPSIGNAL SIGINT

ENTRYPOINT ["/app/nailpit"]
