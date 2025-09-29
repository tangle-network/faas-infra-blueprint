#!/bin/bash

# FaaS Platform Security Audit Script
# Performs comprehensive security checks for production deployment

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging
LOG_FILE="/var/log/faas-security-audit-$(date +%Y%m%d-%H%M%S).log"
ISSUES_FOUND=0

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') $1" | tee -a "$LOG_FILE"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$LOG_FILE"
    ((ISSUES_FOUND++))
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1" | tee -a "$LOG_FILE"
    ((ISSUES_FOUND++))
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1" | tee -a "$LOG_FILE"
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1" | tee -a "$LOG_FILE"
}

check_command() {
    if command -v "$1" >/dev/null 2>&1; then
        return 0
    else
        log_error "Required command '$1' not found"
        return 1
    fi
}

check_file_permissions() {
    local file="$1"
    local expected_perms="$2"
    local description="$3"

    if [[ -f "$file" ]]; then
        local actual_perms=$(stat -c "%a" "$file" 2>/dev/null || stat -f "%A" "$file" 2>/dev/null)
        if [[ "$actual_perms" == "$expected_perms" ]]; then
            log_success "$description: $file has correct permissions ($expected_perms)"
        else
            log_error "$description: $file has incorrect permissions ($actual_perms, expected $expected_perms)"
        fi
    else
        log_warning "$description: $file does not exist"
    fi
}

check_service_status() {
    local service="$1"
    if systemctl is-active --quiet "$service"; then
        log_success "Service $service is running"
    else
        log_error "Service $service is not running"
    fi
}

check_port_binding() {
    local port="$1"
    local description="$2"

    if netstat -tuln | grep -q ":$port "; then
        log_success "$description: Port $port is listening"
    else
        log_error "$description: Port $port is not listening"
    fi
}

main() {
    log_info "Starting FaaS Platform Security Audit"
    log_info "Audit log: $LOG_FILE"
    echo

    # 1. System Prerequisites
    log_info "=== Checking System Prerequisites ==="

    check_command "docker" || exit 1
    check_command "systemctl" || exit 1
    check_command "netstat" || check_command "ss" || exit 1
    check_command "openssl" || exit 1

    # Check if running as appropriate user
    if [[ $EUID -eq 0 ]]; then
        log_warning "Running as root - consider using dedicated faas user"
    fi

    # 2. File Permissions and Configuration
    log_info "=== Checking File Permissions ==="

    # Check critical configuration files
    check_file_permissions "/etc/faas/gateway-config.toml" "600" "Gateway configuration"
    check_file_permissions "/etc/faas/executor-config.toml" "600" "Executor configuration"
    check_file_permissions "/etc/faas/api-key.txt" "600" "API key file"

    # Check SSL certificates
    check_file_permissions "/etc/ssl/certs/faas.crt" "644" "SSL certificate"
    check_file_permissions "/etc/ssl/private/faas.key" "600" "SSL private key"

    # Check systemd service files
    check_file_permissions "/etc/systemd/system/faas-gateway.service" "644" "Gateway service file"
    check_file_permissions "/etc/systemd/system/faas-executor.service" "644" "Executor service file"

    # 3. Service Status
    log_info "=== Checking Service Status ==="

    check_service_status "faas-gateway"
    check_service_status "faas-executor"
    check_service_status "docker"

    # 4. Network Security
    log_info "=== Checking Network Configuration ==="

    check_port_binding "8080" "FaaS Gateway HTTP"
    check_port_binding "8443" "FaaS Gateway HTTPS"

    # Check if unnecessary ports are exposed
    EXPOSED_PORTS=$(netstat -tuln | grep LISTEN | awk '{print $4}' | cut -d: -f2 | sort -u)
    ALLOWED_PORTS=("22" "8080" "8443" "9090")

    for port in $EXPOSED_PORTS; do
        if [[ ! " ${ALLOWED_PORTS[@]} " =~ " ${port} " ]]; then
            log_warning "Unexpected port $port is listening"
        fi
    done

    # 5. SSL/TLS Configuration
    log_info "=== Checking SSL/TLS Configuration ==="

    if [[ -f "/etc/ssl/certs/faas.crt" ]]; then
        # Check certificate expiration
        CERT_EXPIRY=$(openssl x509 -in /etc/ssl/certs/faas.crt -noout -enddate | cut -d= -f2)
        CERT_EXPIRY_EPOCH=$(date -d "$CERT_EXPIRY" +%s)
        CURRENT_EPOCH=$(date +%s)
        DAYS_UNTIL_EXPIRY=$(( (CERT_EXPIRY_EPOCH - CURRENT_EPOCH) / 86400 ))

        if [[ $DAYS_UNTIL_EXPIRY -lt 30 ]]; then
            log_error "SSL certificate expires in $DAYS_UNTIL_EXPIRY days"
        elif [[ $DAYS_UNTIL_EXPIRY -lt 60 ]]; then
            log_warning "SSL certificate expires in $DAYS_UNTIL_EXPIRY days"
        else
            log_success "SSL certificate expires in $DAYS_UNTIL_EXPIRY days"
        fi

        # Check certificate strength
        KEY_SIZE=$(openssl x509 -in /etc/ssl/certs/faas.crt -noout -text | grep "Public-Key:" | grep -o "[0-9]\+")
        if [[ $KEY_SIZE -ge 2048 ]]; then
            log_success "SSL certificate key size is adequate ($KEY_SIZE bits)"
        else
            log_error "SSL certificate key size is weak ($KEY_SIZE bits, minimum 2048)"
        fi
    else
        log_error "SSL certificate not found"
    fi

    # 6. Docker Security
    log_info "=== Checking Docker Security ==="

    # Check Docker daemon configuration
    if [[ -f "/etc/docker/daemon.json" ]]; then
        log_success "Docker daemon configuration file exists"

        # Check for security-related settings
        if grep -q "no-new-privileges" /etc/docker/daemon.json; then
            log_success "Docker configured with no-new-privileges"
        else
            log_warning "Docker not configured with no-new-privileges"
        fi

        if grep -q "userland-proxy.*false" /etc/docker/daemon.json; then
            log_success "Docker userland proxy disabled"
        else
            log_warning "Docker userland proxy not disabled"
        fi
    else
        log_warning "Docker daemon configuration file not found"
    fi

    # Check for privileged containers
    PRIVILEGED_CONTAINERS=$(docker ps --filter "label=privileged=true" -q)
    if [[ -n "$PRIVILEGED_CONTAINERS" ]]; then
        log_error "Privileged containers detected: $PRIVILEGED_CONTAINERS"
    else
        log_success "No privileged containers running"
    fi

    # 7. Container Image Security
    log_info "=== Checking Container Image Security ==="

    # Check for images with security vulnerabilities (if trivy is available)
    if command -v trivy >/dev/null 2>&1; then
        log_info "Running container image vulnerability scan..."

        # Get list of images used by FaaS platform
        IMAGES=$(docker images --format "{{.Repository}}:{{.Tag}}" | grep -E "(faas|alpine|python|node)" | head -5)

        for image in $IMAGES; do
            HIGH_VULNS=$(trivy image --severity HIGH,CRITICAL --quiet --format json "$image" | jq '.Results[]?.Vulnerabilities | length // 0' | awk '{sum+=$1} END {print sum+0}')

            if [[ $HIGH_VULNS -gt 0 ]]; then
                log_error "Image $image has $HIGH_VULNS high/critical vulnerabilities"
            else
                log_success "Image $image has no high/critical vulnerabilities"
            fi
        done
    else
        log_warning "Trivy not installed - skipping image vulnerability scan"
    fi

    # 8. AppArmor/SELinux Status
    log_info "=== Checking Mandatory Access Controls ==="

    # Check AppArmor
    if command -v aa-status >/dev/null 2>&1; then
        if aa-status --enabled 2>/dev/null; then
            log_success "AppArmor is enabled"

            # Check for FaaS-specific profile
            if aa-status | grep -q "faas-container"; then
                log_success "FaaS AppArmor profile is loaded"
            else
                log_warning "FaaS AppArmor profile not found"
            fi
        else
            log_warning "AppArmor is not enabled"
        fi
    fi

    # Check SELinux
    if command -v getenforce >/dev/null 2>&1; then
        SELINUX_STATUS=$(getenforce)
        if [[ "$SELINUX_STATUS" == "Enforcing" ]]; then
            log_success "SELinux is enforcing"
        elif [[ "$SELINUX_STATUS" == "Permissive" ]]; then
            log_warning "SELinux is in permissive mode"
        else
            log_warning "SELinux is disabled"
        fi
    fi

    # 9. Firewall Configuration
    log_info "=== Checking Firewall Configuration ==="

    # Check UFW (Ubuntu)
    if command -v ufw >/dev/null 2>&1; then
        UFW_STATUS=$(ufw status | head -1)
        if echo "$UFW_STATUS" | grep -q "active"; then
            log_success "UFW firewall is active"
        else
            log_warning "UFW firewall is not active"
        fi
    fi

    # Check iptables
    if command -v iptables >/dev/null 2>&1; then
        INPUT_POLICY=$(iptables -L INPUT | head -1 | grep -o "policy [A-Z]*" | cut -d' ' -f2)
        if [[ "$INPUT_POLICY" == "DROP" ]] || [[ "$INPUT_POLICY" == "REJECT" ]]; then
            log_success "iptables INPUT policy is restrictive ($INPUT_POLICY)"
        else
            log_warning "iptables INPUT policy is permissive ($INPUT_POLICY)"
        fi
    fi

    # 10. Log Security
    log_info "=== Checking Log Security ==="

    LOG_DIRS=("/var/log/faas" "/var/log/docker")
    for dir in "${LOG_DIRS[@]}"; do
        if [[ -d "$dir" ]]; then
            DIR_PERMS=$(stat -c "%a" "$dir" 2>/dev/null || stat -f "%A" "$dir" 2>/dev/null)
            if [[ "$DIR_PERMS" == "750" ]] || [[ "$DIR_PERMS" == "755" ]]; then
                log_success "Log directory $dir has appropriate permissions ($DIR_PERMS)"
            else
                log_warning "Log directory $dir permissions may be too permissive ($DIR_PERMS)"
            fi
        else
            log_warning "Log directory $dir does not exist"
        fi
    done

    # 11. System Updates
    log_info "=== Checking System Updates ==="

    # Check for available security updates (Ubuntu/Debian)
    if command -v apt >/dev/null 2>&1; then
        SECURITY_UPDATES=$(apt list --upgradable 2>/dev/null | grep -i security | wc -l)
        if [[ $SECURITY_UPDATES -gt 0 ]]; then
            log_warning "$SECURITY_UPDATES security updates available"
        else
            log_success "No security updates pending"
        fi
    fi

    # Check for available updates (CentOS/RHEL)
    if command -v yum >/dev/null 2>&1; then
        SECURITY_UPDATES=$(yum --security check-update 2>/dev/null | grep -c "security" || echo "0")
        if [[ $SECURITY_UPDATES -gt 0 ]]; then
            log_warning "$SECURITY_UPDATES security updates available"
        else
            log_success "No security updates pending"
        fi
    fi

    # 12. Resource Limits
    log_info "=== Checking Resource Limits ==="

    # Check ulimits for faas user
    if id faas >/dev/null 2>&1; then
        NOFILE_LIMIT=$(su - faas -c 'ulimit -n' 2>/dev/null || echo "unknown")
        if [[ "$NOFILE_LIMIT" -ge 65536 ]]; then
            log_success "File descriptor limit is adequate ($NOFILE_LIMIT)"
        else
            log_warning "File descriptor limit may be too low ($NOFILE_LIMIT)"
        fi
    fi

    # 13. API Security
    log_info "=== Checking API Security ==="

    # Test if API is accessible without authentication
    if command -v curl >/dev/null 2>&1; then
        HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/api/v1/metrics || echo "000")
        if [[ "$HTTP_CODE" == "401" ]] || [[ "$HTTP_CODE" == "403" ]]; then
            log_success "API properly requires authentication"
        elif [[ "$HTTP_CODE" == "200" ]]; then
            log_error "API is accessible without authentication"
        else
            log_warning "Unable to test API authentication (HTTP $HTTP_CODE)"
        fi
    fi

    # Summary
    echo
    log_info "=== Security Audit Summary ==="

    if [[ $ISSUES_FOUND -eq 0 ]]; then
        log_success "Security audit completed with no issues found"
        exit 0
    else
        log_error "Security audit completed with $ISSUES_FOUND issues found"
        log_info "Review the audit log for details: $LOG_FILE"
        exit 1
    fi
}

# Check if script is run with appropriate privileges
if [[ $EUID -ne 0 ]] && [[ ! -r /etc/faas/gateway-config.toml ]]; then
    echo "Warning: Some checks may require root privileges or access to FaaS configuration files"
fi

main "$@"