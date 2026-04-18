#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
install_script="$repo_root/install.sh"
workflow="$repo_root/.github/workflows/build.yml"

failures=0

check_contains() {
    local file="$1"
    local pattern="$2"
    local description="$3"

    if ! grep -Fq -- "$pattern" "$file"; then
        printf '[FAIL] %s\n' "$description" >&2
        printf '       missing pattern: %s\n' "$pattern" >&2
        failures=$((failures + 1))
    else
        printf '[OK] %s\n' "$description"
    fi
}

check_contains "$workflow" "cp target/x86_64-unknown-linux-musl/release/aria-controller release/" \
    "release archive includes aria-controller"

check_contains "$install_script" "for file in aria-agent aria-controller ariactl libebpf_firewall.so libebpf_firewall_perf.so; do" \
    "release package validation requires controller and both eBPF objects"
check_contains "$install_script" 'CONTROLLER_ENV_FILE="$CONTROLLER_CONFIG_DIR/controller.env"' \
    "controller env file path is declared"
check_contains "$install_script" 'CONTROLLER_STATE_DIR="/var/lib/aria-controller"' \
    "controller state directory is declared"
check_contains "$install_script" 'CONTROLLER_LOG_DIR="/var/log/aria-controller"' \
    "controller log directory is declared"
check_contains "$install_script" 'CONTROLLER_LOGROTATE_FILE="/etc/logrotate.d/aria-controller"' \
    "controller logrotate file path is declared"

check_contains "$install_script" "EnvironmentFile=-/etc/aria-controller/controller.env" \
    "controller systemd unit loads controller env file"
check_contains "$install_script" "ExecStart=/usr/local/bin/aria-controller" \
    "controller systemd unit starts aria-controller"
check_contains "$install_script" "ReadWritePaths=/var/lib/aria-controller /var/log/aria-controller" \
    "controller systemd unit can write state and logs"

check_contains "$install_script" "ARIA_CONTROLLER_BIND=127.0.0.1:8180" \
    "controller default bind uses port 8180"
check_contains "$install_script" 'ARIA_CONTROLLER_STATE_PATH=$CONTROLLER_STATE_FILE' \
    "controller default env points to controller state file"
check_contains "$install_script" 'ARIA_CONTROLLER_LOG_FILE_PATH=$CONTROLLER_LOG_FILE' \
    "controller default env points to controller log file"

check_contains "$install_script" "write_controller_systemd_unit" \
    "controller systemd unit is written during install"
check_contains "$install_script" "write_controller_logrotate_config" \
    "controller logrotate config is written during install"
check_contains "$install_script" 'if [[ "$START_CONTROLLER" -eq 1 ]]; then' \
    "controller service startup is gated by --start-controller"

if (( failures > 0 )); then
    printf '\n%d install layout check(s) failed.\n' "$failures" >&2
    exit 1
fi

printf '\nInstall layout checks passed.\n'
