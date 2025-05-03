#!/bin/bash

# Basic Environment Check Script for FaaS Platform Prerequisites

echo \"Running Environment Checks...\"

HAS_ERROR=0

# Check CPU VMX (Virtualization Technology)
if ! grep -q -E \"(vmx|svm)\" /proc/cpuinfo; then
    echo \"ERROR: CPU Virtualization (VMX or SVM) not found or not enabled in BIOS.\" >&2
    HAS_ERROR=1
else
    echo \"[OK] CPU Virtualization (VMX/SVM) found.\"
fi

# Check TDX Support (Basic check for TDX flag)
# Note: A more robust check might involve specific MSRs or kernel messages
if ! grep -q \"tdx_guest\" /proc/cpuinfo && ! grep -q \"tdx_host\" /proc/cpuinfo ; then
    echo \"WARN: TDX CPU flag ('tdx_guest' or 'tdx_host') not found in /proc/cpuinfo. TDX might not be supported or enabled.\" >&2
    # Consider making this an error if TDX is strictly required from the start
    # HAS_ERROR=1
else
    echo \"[OK] TDX CPU flag found.\"
fi

# Check KVM Modules
if ! lsmod | grep -q kvm; then
    echo \"ERROR: KVM modules not loaded. Is KVM installed and enabled?\" >&2
    HAS_ERROR=1
else
    echo \"[OK] KVM modules appear to be loaded.\"
fi

# Check KVM Device Permissions
if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
    echo \"ERROR: Current user does not have read/write permissions for /dev/kvm.\" >&2
    echo \"Hint: Add your user to the 'kvm' group (e.g., sudo usermod -aG kvm \${USER}) and reboot or log out/in.\" >&2
    HAS_ERROR=1
else
    echo \"[OK] /dev/kvm permissions seem correct.\"
fi

# Check if Docker command exists
if ! command -v docker &> /dev/null; then
    echo \"ERROR: Docker command not found. Is Docker installed?\" >&2
    HAS_ERROR=1
else
    echo \"[OK] Docker command found.\"
    # Check Docker Daemon Status/Permissions
    if ! docker info > /dev/null 2>&1; then
        echo \"ERROR: Could not connect to Docker daemon. Is it running? Does the user have permission?\" >&2
        echo \"Hint: Add your user to the 'docker' group (e.g., sudo usermod -aG docker \${USER}) and reboot or log out/in.\" >&2
        HAS_ERROR=1
    else
        echo \"[OK] Docker daemon connection successful.\"
    fi
fi

# Check for QEMU (optional, depending on Firecracker/VM strategy)
if ! command -v qemu-system-x86_64 &> /dev/null; then
    echo \"WARN: qemu-system-x86_64 command not found. Required if using QEMU directly.\" >&2
else
    echo \"[OK] qemu-system-x86_64 found.\"
fi

# Check for Firecracker (optional)
# Assuming firecracker binary is in PATH or known location
# if ! command -v firecracker &> /dev/null; then
#     echo \"WARN: firecracker command not found. Required if using Firecracker directly.\" >&2
# else
#     echo \"[OK] firecracker found.\"
# fi

echo \"--------------------\"
if [ $HAS_ERROR -ne 0 ]; then
    echo \"Environment checks failed. Please address the errors above.\" >&2
    exit 1
else
    echo \"Environment checks passed (basic verification).\"
    exit 0
fi 