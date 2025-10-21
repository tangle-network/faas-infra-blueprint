#!/usr/bin/env bash
# Publish the pre-built Firecracker rootfs to the faas-infra-assets repo.
#
# Usage:
#   scripts/publish_firecracker_rootfs.sh [--repo owner/name] [--tag TAG]
#
# Requirements:
#   - GitHub CLI (`gh`) authenticated with repo push access.
#   - `rootfs.ext4` present (defaults to tools/firecracker-rootfs-builder/output/rootfs.ext4).

set -euo pipefail

PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

REPO="tangle-network/faas-infra-assets"
TAG="fc-rootfs-$(date +%Y%m%d-%H%M)"
ROOTFS_PATH="${PROJECT_ROOT}/tools/firecracker-rootfs-builder/output/rootfs.ext4"

usage() {
  cat <<EOF
Usage: $0 [--repo owner/name] [--tag TAG] [--rootfs PATH]

Options:
  --repo    GitHub repository to upload to (default: ${REPO})
  --tag     Release tag to use/create (default: ${TAG})
  --rootfs  Path to rootfs.ext4 (default: ${ROOTFS_PATH})

Examples:
  $0 --tag fc-rootfs-20250115
  $0 --repo my-org/assets --rootfs /tmp/rootfs.ext4

Requires: GitHub CLI (gh) authenticated with repo access.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      REPO="$2"; shift 2;;
    --tag)
      TAG="$2"; shift 2;;
    --rootfs)
      ROOTFS_PATH="$2"; shift 2;;
    -h|--help)
      usage; exit 0;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 1;;
  esac
done

if ! command -v gh >/dev/null; then
  echo "[ERROR] GitHub CLI (gh) is not installed or not in PATH" >&2
  exit 1
fi

if [[ ! -f "$ROOTFS_PATH" ]]; then
  echo "[ERROR] rootfs file not found: $ROOTFS_PATH" >&2
  exit 1
fi

ROOTFS_PATH_ABS="$(realpath "$ROOTFS_PATH")"
ROOTFS_DIR="$(dirname "$ROOTFS_PATH_ABS")"
SHA_FILE="${ROOTFS_DIR}/$(basename "$ROOTFS_PATH_ABS").sha256"
sha256sum "$ROOTFS_PATH_ABS" > "$SHA_FILE"

echo "[INFO] Uploading rootfs to $REPO (tag: $TAG)"

if ! gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  echo "[INFO] Creating new release $TAG"
  gh release create "$TAG" --repo "$REPO" --title "$TAG" --notes "Automated rootfs upload" >/dev/null
else
  echo "[INFO] Release $TAG already exists; updating assets"
fi

gh release upload "$TAG" "$ROOTFS_PATH_ABS" "$SHA_FILE" --repo "$REPO" --clobber >/dev/null

echo "[INFO] Upload complete. Assets available at https://github.com/$REPO/releases/tag/$TAG"
