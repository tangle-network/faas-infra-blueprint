#!/usr/bin/env bash
# Prepare Firecracker kernel/rootfs assets for CI or local runs.
# - If FC_ROOTFS_URL is set, download the prebuilt rootfs (optionally verifying the SHA256).
# - Otherwise, build the rootfs via tools/firecracker-rootfs-builder (slow, but deterministic).

set -euo pipefail

PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
ASSET_DIR="/var/lib/faas"
KERNEL_SOURCE="${PROJECT_ROOT}/resources/kernel/hello-vmlinux.bin"
TARGET_KERNEL="${ASSET_DIR}/kernel"
TARGET_ROOTFS="${ASSET_DIR}/rootfs.ext4"
DEFAULT_ROOTFS_URL="https://github.com/tangle-network/faas-infra-assets/releases/latest/download/rootfs.ext4"
DEFAULT_ROOTFS_SHA256_URL="https://github.com/tangle-network/faas-infra-assets/releases/latest/download/rootfs.sha256"

ROOTFS_URL="${FC_ROOTFS_URL:-${DEFAULT_ROOTFS_URL}}"
ROOTFS_SHA256="${FC_ROOTFS_SHA256:-}"

sudo mkdir -p "${ASSET_DIR}"

echo "::group::Installing Firecracker kernel"
sudo install -Dm644 "${KERNEL_SOURCE}" "${TARGET_KERNEL}"
echo "::endgroup::"

downloaded=false
if [[ -n "${ROOTFS_URL}" ]]; then
  echo "::group::Downloading prebuilt rootfs from ${ROOTFS_URL}"
  tmp_rootfs="$(mktemp)"
  if curl --fail --location --show-error --output "${tmp_rootfs}" "${ROOTFS_URL}"; then
    if [[ -z "${FC_ROOTFS_SHA256:-}" ]]; then
      echo "::group::Fetching checksum from ${DEFAULT_ROOTFS_SHA256_URL}"
      if ROOTFS_SHA256_CONTENT="$(curl --fail --location --show-error --silent "${DEFAULT_ROOTFS_SHA256_URL}" 2>/dev/null)"; then
        ROOTFS_SHA256="$(echo "${ROOTFS_SHA256_CONTENT}" | awk '{print $1}')"
      else
        ROOTFS_SHA256=""
        echo "Checksum file not available; continuing without verification."
      fi
      echo "::endgroup::"
    fi
    if [[ -n "${ROOTFS_SHA256}" ]]; then
      echo "${ROOTFS_SHA256}  ${tmp_rootfs}" | sha256sum --check -
    fi
    sudo install -Dm644 "${tmp_rootfs}" "${TARGET_ROOTFS}"
    rm -f "${tmp_rootfs}"
    echo "::endgroup::"
    downloaded=true
  else
    echo "::warning::Failed to download rootfs from ${ROOTFS_URL}. Falling back to local Buildroot build."
    rm -f "${tmp_rootfs}"
    echo "::endgroup::"
  fi
fi

if [[ "${downloaded}" != "true" ]]; then
  LOCAL_ROOTFS="${PROJECT_ROOT}/tools/firecracker-rootfs-builder/output/rootfs.ext4"
  if [[ -f "${LOCAL_ROOTFS}" ]]; then
    echo "::group::Using existing locally built rootfs (${LOCAL_ROOTFS})"
    sudo install -Dm644 "${LOCAL_ROOTFS}" "${TARGET_ROOTFS}"
    echo "::endgroup::"
  else
    echo "::group::Prebuilt rootfs not available; building via Buildroot (this may take a while)"
    (
      cd "${PROJECT_ROOT}/tools/firecracker-rootfs-builder"
      sudo ./build_rootfs.sh
    )
    sudo install -Dm644 "${LOCAL_ROOTFS}" "${TARGET_ROOTFS}"
    echo "::endgroup::"
  fi
fi

echo "Firecracker assets staged at ${ASSET_DIR}"
