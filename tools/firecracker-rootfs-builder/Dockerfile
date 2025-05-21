# Stage 1: Asset Preparation
FROM debian:bookworm-slim AS asset-prep

WORKDIR /assets

RUN echo "Stage 1: Starting asset preparation..."

RUN echo "Stage 1: Attempting to COPY Buildroot source (2024.02.3)..."
COPY tools/firecracker-rootfs-builder/buildroot-2024.02.3 /assets/buildroot-src
RUN echo "Stage 1: Successfully COPIED Buildroot source."

RUN echo "Stage 1: Attempting to COPY project source (./)..."
COPY . /assets/project-src
RUN echo "Stage 1: Successfully COPIED project source."
RUN echo "Stage 1: Listing /assets/project-src/target after COPY ./" && \
    ls -la /assets/project-src/target/ || echo "Target dir not found or ls failed in asset-prep"
RUN echo "Stage 1: Listing /assets/project-src/target/x86_64-unknown-linux-musl/release/ after COPY ./" && \
    ls -la /assets/project-src/target/x86_64-unknown-linux-musl/release/ || echo "Release dir not found or ls failed in asset-prep"
RUN echo "Stage 1: Checking for agent binary specifically in asset-prep stage:" && \
    ls -la /assets/project-src/target/x86_64-unknown-linux-musl/release/faas-guest-agent || echo "Agent binary /assets/project-src/target/x86_64-unknown-linux-musl/release/faas-guest-agent not found in asset-prep"

RUN echo "Stage 1: Asset preparation complete."

# Stage 2: Main Builder Environment
FROM debian:bookworm-slim AS builder

ARG BUILDROOT_VERSION
ENV BUILDROOT_VERSION=${BUILDROOT_VERSION:-2024.02.3}

RUN apt-get update && apt-get install -y --no-install-recommends \
    sed make binutils build-essential gcc g++ bash patch \
    gzip bzip2 perl tar cpio unzip rsync file bc wget \
    libncurses-dev python3 git pkg-config cpio ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

RUN echo "Stage 2: Starting main builder setup..."

RUN echo "Stage 2: Attempting to COPY --from=asset-prep buildroot-src..."
COPY --from=asset-prep /assets/buildroot-src /build/buildroot-src
RUN echo "Stage 2: Successfully COPIED --from=asset-prep buildroot-src."

RUN echo "Stage 2: Attempting to COPY --from=asset-prep project-src..."
COPY --from=asset-prep /assets/project-src /build/project-src
RUN echo "Stage 2: Successfully COPIED --from=asset-prep project-src."
RUN echo "Stage 2: Listing /build/project-src/target after COPY --from=asset-prep" && \
    ls -la /build/project-src/target/ || echo "Target dir not found or ls failed in builder stage"
RUN echo "Stage 2: Listing /build/project-src/target/x86_64-unknown-linux-musl/release/ after COPY --from=asset-prep" && \
    ls -la /build/project-src/target/x86_64-unknown-linux-musl/release/ || echo "Release dir not found or ls failed in builder stage"
RUN echo "Stage 2: Checking for agent binary specifically in builder stage:" && \
    ls -la /build/project-src/target/x86_64-unknown-linux-musl/release/faas-guest-agent || echo "Agent binary /build/project-src/target/x86_64-unknown-linux-musl/release/faas-guest-agent not found in builder stage"

COPY tools/firecracker-rootfs-builder/buildroot_package /build/buildroot_package
COPY tools/firecracker-rootfs-builder/buildroot_config.base /build/buildroot_config.base
COPY tools/firecracker-rootfs-builder/overlay /build/overlay
RUN chmod +x /build/overlay/init

VOLUME /build/output

COPY tools/firecracker-rootfs-builder/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

RUN echo "Stage 2: Main builder setup complete."

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["make"] 