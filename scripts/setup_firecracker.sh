#!/bin/bash
# Firecracker setup script for Linux production environments

set -e

FC_VERSION="1.5.0"
ARCH=$(uname -m)
OS=$(uname -s)

if [ "$OS" != "Linux" ]; then
    echo "⚠️  Firecracker only runs on Linux. Detected: $OS"
    echo "Use Docker on macOS for development."
    exit 1
fi

echo "🚀 Setting up Firecracker v$FC_VERSION for $ARCH"

# Create directories
sudo mkdir -p /usr/local/bin
sudo mkdir -p /var/lib/firecracker/{kernel,rootfs,snapshots}

# Download Firecracker binary
echo "📥 Downloading Firecracker..."
wget -q "https://github.com/firecracker-microvm/firecracker/releases/download/v${FC_VERSION}/firecracker-v${FC_VERSION}-${ARCH}.tgz"
tar -xzf "firecracker-v${FC_VERSION}-${ARCH}.tgz"
sudo mv "release-v${FC_VERSION}-${ARCH}/firecracker-v${FC_VERSION}-${ARCH}" /usr/local/bin/firecracker
sudo chmod +x /usr/local/bin/firecracker
rm -rf "firecracker-v${FC_VERSION}-${ARCH}.tgz" "release-v${FC_VERSION}-${ARCH}"

# Use our kernel
echo "📥 Setting up kernel..."
KERNEL_PATH="/var/lib/firecracker/kernel/vmlinux"
if [ -f "resources/kernel/vmlinux-x86_64-5.10.186.bin" ]; then
    echo "✅ Found local kernel"
    sudo cp resources/kernel/vmlinux-x86_64-5.10.186.bin "$KERNEL_PATH"
elif [ -f "../resources/kernel/vmlinux-x86_64-5.10.186.bin" ]; then
    sudo cp ../resources/kernel/vmlinux-x86_64-5.10.186.bin "$KERNEL_PATH"
else
    echo "⚠️  No local kernel. Downloading..."
    KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.5/x86_64/vmlinux-5.10.186"
    sudo wget -q -O "$KERNEL_PATH" "$KERNEL_URL"
fi

# Use our custom-built rootfs
echo "📦 Using custom FaaS rootfs..."
ROOTFS_PATH="/var/lib/firecracker/rootfs/rootfs.ext4"
if [ -f "tools/firecracker-rootfs-builder/output/images/rootfs.ext2" ]; then
    echo "✅ Found locally built rootfs"
    sudo cp tools/firecracker-rootfs-builder/output/images/rootfs.ext2 "$ROOTFS_PATH"
elif [ -f "../tools/firecracker-rootfs-builder/output/images/rootfs.ext2" ]; then
    sudo cp ../tools/firecracker-rootfs-builder/output/images/rootfs.ext2 "$ROOTFS_PATH"
else
    echo "⚠️  No local rootfs found. Building now..."
    cd tools/firecracker-rootfs-builder && ./build_rootfs.sh
    sudo cp output/images/rootfs.ext2 "$ROOTFS_PATH"
    cd -
fi

# Set up networking (TAP device)
echo "🌐 Setting up networking..."
sudo ip tuntap add tap0 mode tap
sudo ip addr add 172.16.0.1/24 dev tap0
sudo ip link set tap0 up

# Enable IP forwarding
sudo sh -c "echo 1 > /proc/sys/net/ipv4/ip_forward"
sudo iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
sudo iptables -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
sudo iptables -A FORWARD -i tap0 -o eth0 -j ACCEPT

# Install KVM if not present
if [ ! -e /dev/kvm ]; then
    echo "📦 Installing KVM..."

    # Detect distribution
    if [ -f /etc/debian_version ]; then
        # Debian/Ubuntu
        sudo apt-get update
        sudo apt-get install -y qemu-kvm libvirt-daemon-system libvirt-clients bridge-utils cpu-checker
    elif [ -f /etc/redhat-release ]; then
        # RHEL/CentOS/Fedora
        sudo yum install -y qemu-kvm libvirt libvirt-python libguestfs-tools virt-install
    elif [ -f /etc/arch-release ]; then
        # Arch Linux
        sudo pacman -S --noconfirm qemu libvirt virt-manager
    else
        echo "⚠️  Unsupported distribution. Please install KVM manually:"
        echo "   Ubuntu/Debian: apt-get install qemu-kvm"
        echo "   RHEL/Fedora: yum install qemu-kvm"
        echo "   Arch: pacman -S qemu"
    fi

    # Load KVM modules
    sudo modprobe kvm
    sudo modprobe kvm_intel 2>/dev/null || sudo modprobe kvm_amd 2>/dev/null

    # Verify KVM is available
    if [ ! -e /dev/kvm ]; then
        echo "❌ KVM installation failed. Check if virtualization is enabled in BIOS."
        echo "   Run: egrep -c '(vmx|svm)' /proc/cpuinfo"
        echo "   If output is 0, virtualization is disabled in BIOS."
        exit 1
    fi
fi

# Check KVM acceleration
if command -v kvm-ok &> /dev/null; then
    kvm-ok || echo "⚠️  KVM acceleration may not be fully functional"
fi

sudo chmod 666 /dev/kvm
echo "✅ KVM is available and configured"

# Verify installation
if /usr/local/bin/firecracker --version; then
    echo "✅ Firecracker installed successfully!"
    echo "📁 Kernel: /var/lib/firecracker/kernel/vmlinux"
    echo "📁 RootFS: /var/lib/firecracker/rootfs/ubuntu.ext4"
    echo "🌐 Network: tap0 (172.16.0.1/24)"
else
    echo "❌ Installation failed"
    exit 1
fi

# Create systemd service (optional)
cat << 'EOF' | sudo tee /etc/systemd/system/firecracker-setup.service
[Unit]
Description=Firecracker Network Setup
After=network.target

[Service]
Type=oneshot
ExecStart=/bin/bash -c '
    ip tuntap add tap0 mode tap || true
    ip addr add 172.16.0.1/24 dev tap0 || true
    ip link set tap0 up || true
    echo 1 > /proc/sys/net/ipv4/ip_forward
    iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE || true
'
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable firecracker-setup

echo "🎉 Firecracker setup complete!"