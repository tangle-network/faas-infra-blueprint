# Use Tangle base image for Blueprint builds if available, otherwise standard Rust
# FROM ghcr.io/tangle-network/tangle-runtime-dev:latest AS builder
FROM rust:1.75 AS builder

WORKDIR /usr/src/app

# Copy necessary source code
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
# Copy all crates, including faas-lib
COPY crates crates
# Copy the binary crate source
COPY faas-bin faas-bin

# Build the specific Blueprint binary using the correct package name
RUN cargo build --release --package faas-blueprint-bin --bin faas-blueprint


# --- Runtime Stage ---
# Use a minimal base image
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Install necessary runtime dependencies (Docker client, ca-certificates)
RUN apt-get update && apt-get install -y docker.io ca-certificates --no-install-recommends && \
    rm -rf /var/lib/apt/lists/*

# Copy the built binary from the builder stage
COPY --from=builder /usr/src/app/target/release/faas-blueprint /usr/local/bin/faas-blueprint

# Set user (needs access to docker.sock)
# Running as root simplifies Docker socket access for now.
# Consider using user mapping or socket forwarding in production.
# USER root

# Set the entrypoint to the Blueprint binary
ENTRYPOINT ["/usr/local/bin/faas-blueprint"] 