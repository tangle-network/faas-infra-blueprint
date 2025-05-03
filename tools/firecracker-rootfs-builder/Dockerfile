# Dockerfile for building the Firecracker RootFS using Buildroot
FROM debian:bookworm-slim AS builder

ARG BUILDROOT_VERSION
ENV BUILDROOT_VERSION=${BUILDROOT_VERSION:-2025.02.1}

RUN apt-get update && apt-get install -y --no-install-recommends \
    sed make binutils build-essential gcc g++ bash patch \
    gzip bzip2 perl tar cpio unzip rsync file bc wget \
    libncurses-dev python3 git pkg-config cpio \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy required assets from host build context
COPY buildroot_package ./buildroot_package
COPY buildroot_config.base ./buildroot_config.base
COPY overlay ./overlay
RUN chmod +x /build/overlay/init

# Mount points for volumes from host
VOLUME /build/buildroot-src
VOLUME /build/project-src
VOLUME /build/output

# Entrypoint script
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["make"] 