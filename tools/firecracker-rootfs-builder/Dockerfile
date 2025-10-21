# Stage 1: Build the faas-guest-agent using a Rust toolchain
FROM rust:1.78-bookworm AS agent-build

ENV CARGO_TERM_COLOR=always

RUN apt-get update && apt-get install -y --no-install-recommends \
    musl-tools pkg-config libssl-dev clang llvm make g++ cmake linux-libc-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

RUN rustup target add x86_64-unknown-linux-musl

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY faas-bin ./faas-bin
COPY faas-lib ./faas-lib
COPY faas-guest-agent ./faas-guest-agent 2>/dev/null || true
COPY tools ./tools
COPY resources ./resources

RUN cargo build --release --package faas-guest-agent --target x86_64-unknown-linux-musl

# Stage 2: Build the rootfs using Buildroot
FROM debian:bookworm-slim AS builder

ARG BUILDROOT_VERSION
ENV BUILDROOT_VERSION=${BUILDROOT_VERSION:-2024.02.3}

RUN apt-get update && apt-get install -y --no-install-recommends \
    sed make binutils build-essential gcc g++ bash patch \
    gzip bzip2 perl tar cpio unzip rsync file bc wget \
    libncurses-dev python3 git pkg-config cpio ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy Buildroot sources and overlay assets
COPY tools/firecracker-rootfs-builder/buildroot-${BUILDROOT_VERSION} /build/buildroot-src
COPY tools/firecracker-rootfs-builder/buildroot_package /build/buildroot_package
COPY tools/firecracker-rootfs-builder/buildroot_config.base /build/buildroot_config.base
COPY tools/firecracker-rootfs-builder/overlay /build/overlay
RUN chmod +x /build/overlay/init

# Provide the agent binary in the location expected by the entrypoint
RUN mkdir -p /build/project-src/target/x86_64-unknown-linux-musl/release
COPY --from=agent-build /workspace/target/x86_64-unknown-linux-musl/release/faas-guest-agent \
     /build/project-src/target/x86_64-unknown-linux-musl/release/faas-guest-agent

VOLUME /build/output

COPY tools/firecracker-rootfs-builder/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
