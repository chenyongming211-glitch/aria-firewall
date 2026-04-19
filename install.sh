#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR=""

INSTALL_BIN_DIR="/usr/local/bin"
INSTALL_LIB_DIR="/usr/local/lib"
CONFIG_DIR="/etc/aria-agent"
CONFIG_FILE="$CONFIG_DIR/config.toml"
STATE_DIR="/var/lib/aria-agent"
LOG_DIR="/var/log/aria-agent"
LOG_FILE="$LOG_DIR/aria-agent.log"
LOGROTATE_FILE="/etc/logrotate.d/aria-agent"
CONTROLLER_CONFIG_DIR="/etc/aria-controller"
CONTROLLER_ENV_FILE="$CONTROLLER_CONFIG_DIR/controller.env"
CONTROLLER_STATE_DIR="/var/lib/aria-controller"
CONTROLLER_STATE_FILE="$CONTROLLER_STATE_DIR/controller-state.json"
CONTROLLER_LOG_DIR="/var/log/aria-controller"
CONTROLLER_LOG_FILE="$CONTROLLER_LOG_DIR/aria-controller.log"
CONTROLLER_LOGROTATE_FILE="/etc/logrotate.d/aria-controller"
PIN_ROOT="/sys/fs/bpf"
PIN_DIR="$PIN_ROOT/aria"
SYSTEMD_UNIT="/etc/systemd/system/aria-agent.service"
CONTROLLER_SYSTEMD_UNIT="/etc/systemd/system/aria-controller.service"

ZIP_PATH=""
RELEASE_DIR=""
FORCE_CONFIG=0
NO_START=0
START_CONTROLLER=0
VERIFY_LAYOUT=0

REQUIRED_RELEASE_FILES=(
    aria-agent
    aria-controller
    ariactl
    libebpf_firewall.so
    libebpf_firewall_perf.so
)

cleanup() {
    if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT

log() {
    printf '[INFO] %s\n' "$*"
}

warn() {
    printf '[WARN] %s\n' "$*" >&2
}

die() {
    printf '[ERROR] %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Aria Firewall 一键安装/更新脚本

用法:
  sudo ./install.sh
  sudo ./install.sh --zip /path/to/firewall-binaries-x86_64.zip
  sudo ./install.sh --force-config
  sudo ./install.sh --start-controller
  sudo ./install.sh --no-start
  ./install.sh --verify-layout
  ./install.sh --verify-layout --release-dir release

说明:
  - 默认会在脚本同目录自动查找 firewall-binaries*.zip
  - 默认保留已有 /etc/aria-agent/config.toml
  - 默认会安装/更新 aria-agent、aria-controller、ariactl、
    libebpf_firewall.so、libebpf_firewall_perf.so，并重启 aria-agent 服务
  - 默认只安装 aria-controller.service，不启用/启动；需要时传入 --start-controller
  - --verify-layout 只做安装布局静态检查，不需要 root，不安装文件

选项:
  --zip PATH         指定 release zip 路径
  --force-config     覆盖生成默认 /etc/aria-agent/config.toml
  --start-controller  启用并启动/重启 aria-controller.service
  --no-start         只安装，不启动/重启服务
  --verify-layout    静态检查 service/env/logrotate/release 文件布局
  --release-dir DIR   配合 --verify-layout 检查 release 目录中的实际产物
  -h, --help         显示帮助
EOF
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "缺少命令: $1"
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --zip)
                [[ $# -ge 2 ]] || die "--zip 需要一个路径参数"
                ZIP_PATH="$2"
                shift 2
                ;;
            --release-dir)
                [[ $# -ge 2 ]] || die "--release-dir 需要一个路径参数"
                RELEASE_DIR="$2"
                shift 2
                ;;
            --force-config)
                FORCE_CONFIG=1
                shift
                ;;
            --start-controller)
                START_CONTROLLER=1
                shift
                ;;
            --no-start)
                NO_START=1
                shift
                ;;
            --verify-layout)
                VERIFY_LAYOUT=1
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                die "未知参数: $1"
                ;;
        esac
    done
}

find_zip() {
    if [[ -n "$ZIP_PATH" ]]; then
        [[ -f "$ZIP_PATH" ]] || die "zip 不存在: $ZIP_PATH"
        return
    fi

    local -a candidates=()
    local candidate

    shopt -s nullglob
    for candidate in "$SCRIPT_DIR"/firewall-binaries*.zip; do
        candidates+=("$candidate")
    done
    shopt -u nullglob

    if [[ ${#candidates[@]} -eq 1 ]]; then
        ZIP_PATH="${candidates[0]}"
        return
    fi

    if [[ ${#candidates[@]} -gt 1 ]]; then
        die "脚本目录下发现多个 zip，请显式传入 --zip: $SCRIPT_DIR"
    fi

    die "脚本目录下未找到 firewall-binaries*.zip，请把 zip 放到脚本同目录，或使用 --zip 指定"
}

check_root() {
    [[ "${EUID:-$(id -u)}" -eq 0 ]] || die "请使用 root 运行，例如: sudo ./install.sh"
}

parse_kernel_version() {
    local rel="$1"
    if [[ "$rel" =~ ^([0-9]+)\.([0-9]+) ]]; then
        printf '%s %s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
    else
        return 1
    fi
}

kernel_ge() {
    local cur_major="$1"
    local cur_minor="$2"
    local req_major="$3"
    local req_minor="$4"
    if (( cur_major > req_major )); then
        return 0
    fi
    if (( cur_major < req_major )); then
        return 1
    fi
    (( cur_minor >= req_minor ))
}

check_environment() {
    require_cmd unzip
    require_cmd install
    require_cmd sha256sum
    require_cmd uname
    require_cmd mountpoint
    require_cmd mount

    local kernel_release
    kernel_release="$(uname -r)"
    local major minor
    read -r major minor < <(parse_kernel_version "$kernel_release") \
        || die "无法解析内核版本: $kernel_release"

    if ! kernel_ge "$major" "$minor" 4 18; then
        die "当前内核 $kernel_release 低于最低要求 4.18"
    fi

    if kernel_ge "$major" "$minor" 5 8; then
        log "检测到内核 $kernel_release，支持 perf trace 和 EDT shaping"
    else
        warn "当前内核 $kernel_release 低于 5.8：QoS shaping 会退化，XDP link pin 能力受限"
    fi

    if [[ ! -e /sys/kernel/btf/vmlinux ]]; then
        warn "未检测到 /sys/kernel/btf/vmlinux，agent 可能无法正常加载 eBPF"
    else
        log "检测到 BTF: /sys/kernel/btf/vmlinux"
    fi

    mkdir -p "$PIN_ROOT"
    if ! mountpoint -q "$PIN_ROOT"; then
        log "挂载 bpffs 到 $PIN_ROOT"
        mount -t bpf bpffs "$PIN_ROOT"
    fi

    mkdir -p "$PIN_DIR"
}

unpack_release() {
    TMP_DIR="$(mktemp -d /tmp/aria-install.XXXXXX)"
    log "解压 release 包: $ZIP_PATH"
    unzip -q "$ZIP_PATH" -d "$TMP_DIR"

    local file
    for file in "${REQUIRED_RELEASE_FILES[@]}"; do
        [[ -f "$TMP_DIR/$file" ]] || die "release 包缺少文件: $file"
    done
}

backup_existing() {
    local backup_dir=""
    local ts
    ts="$(date +%Y%m%d-%H%M%S)"

    local -a paths=(
        "$INSTALL_BIN_DIR/aria-agent"
        "$INSTALL_BIN_DIR/aria-controller"
        "$INSTALL_BIN_DIR/ariactl"
        "$INSTALL_LIB_DIR/libebpf_firewall.so"
        "$INSTALL_LIB_DIR/libebpf_firewall_perf.so"
        "$SYSTEMD_UNIT"
        "$CONTROLLER_SYSTEMD_UNIT"
        "$CONFIG_FILE"
        "$CONTROLLER_ENV_FILE"
        "$LOGROTATE_FILE"
        "$CONTROLLER_LOGROTATE_FILE"
    )

    local path
    for path in "${paths[@]}"; do
        if [[ -e "$path" ]]; then
            backup_dir="$STATE_DIR/install-backups/$ts"
            mkdir -p "$backup_dir"
            break
        fi
    done

    [[ -n "$backup_dir" ]] || return 0

    log "备份当前安装到 $backup_dir"
    for path in "${paths[@]}"; do
        if [[ -e "$path" ]]; then
            cp -a "$path" "$backup_dir/"
        fi
    done
}

emit_systemd_unit() {
    cat <<'EOF'
[Unit]
Description=Aria Firewall Agent (multi-tap XDP firewall daemon)
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/aria-agent --config /etc/aria-agent/config.toml
Restart=on-failure
RestartSec=5
User=root
Group=root
LimitMEMLOCK=infinity
ProtectSystem=strict
ReadWritePaths=/sys/fs/bpf /var/lib/aria-agent /var/log/aria-agent
ProtectHome=yes

[Install]
WantedBy=multi-user.target
EOF
}

write_systemd_unit() {
    emit_systemd_unit >"$SYSTEMD_UNIT"
}

emit_controller_systemd_unit() {
    cat <<'EOF'
[Unit]
Description=Aria Firewall Controller
After=network.target

[Service]
Type=simple
EnvironmentFile=-/etc/aria-controller/controller.env
ExecStart=/usr/local/bin/aria-controller
Restart=on-failure
RestartSec=5
User=root
Group=root
ProtectSystem=strict
ReadWritePaths=/var/lib/aria-controller /var/log/aria-controller
ProtectHome=yes

[Install]
WantedBy=multi-user.target
EOF
}

write_controller_systemd_unit() {
    emit_controller_systemd_unit >"$CONTROLLER_SYSTEMD_UNIT"
}

emit_logrotate_config() {
    cat <<EOF
$LOG_FILE {
    daily
    rotate 14
    missingok
    notifempty
    compress
    delaycompress
    copytruncate
    create 0640 root root
}
EOF
}

write_logrotate_config() {
    emit_logrotate_config >"$LOGROTATE_FILE"
}

emit_controller_logrotate_config() {
    cat <<EOF
$CONTROLLER_LOG_FILE {
    daily
    rotate 14
    missingok
    notifempty
    compress
    delaycompress
    copytruncate
    create 0640 root root
}
EOF
}

write_controller_logrotate_config() {
    emit_controller_logrotate_config >"$CONTROLLER_LOGROTATE_FILE"
}

emit_default_config() {
    cat <<'EOF'
ebpf_path = "/usr/local/lib/libebpf_firewall.so"
trace_backend = "auto"
trace_auto_allow_ringbuf = false
pin_path = "/sys/fs/bpf/aria"
state_path = "/var/lib/aria-agent"
iface_pattern = "^(eth|tap)"
max_port_policies = 16384
listen_addr = "127.0.0.1:8080"
log_format = "text"
log_filter = "info"
log_file_path = "/var/log/aria-agent/aria-agent.log"
EOF
}

write_default_config() {
    emit_default_config >"$CONFIG_FILE"
}

emit_default_controller_env() {
    cat <<EOF
ARIA_CONTROLLER_BIND=127.0.0.1:8180
ARIA_CONTROLLER_STATE_PATH=$CONTROLLER_STATE_FILE
ARIA_CONTROLLER_LOG_FORMAT=text
ARIA_CONTROLLER_LOG_FILTER=info
ARIA_CONTROLLER_LOG_FILE_PATH=$CONTROLLER_LOG_FILE
EOF
}

write_default_controller_env() {
    emit_default_controller_env >"$CONTROLLER_ENV_FILE"
}

verify_contains() {
    local content="$1"
    local pattern="$2"
    local description="$3"

    if [[ "$content" != *"$pattern"* ]]; then
        printf '[FAIL] %s\n' "$description" >&2
        printf '       missing pattern: %s\n' "$pattern" >&2
        failures=$((failures + 1))
    else
        printf '[OK] %s\n' "$description"
    fi
}

verify_required_release_file() {
    local required="$1"
    local found=0
    local file

    for file in "${REQUIRED_RELEASE_FILES[@]}"; do
        if [[ "$file" == "$required" ]]; then
            found=1
            break
        fi
    done

    if [[ "$found" -eq 0 ]]; then
        printf '[FAIL] release package requires %s\n' "$required" >&2
        failures=$((failures + 1))
    else
        printf '[OK] release package requires %s\n' "$required"
    fi
}

verify_release_dir() {
    local release_dir="$1"
    local file

    if [[ ! -d "$release_dir" ]]; then
        printf '[FAIL] release directory exists\n' >&2
        printf '       missing directory: %s\n' "$release_dir" >&2
        failures=$((failures + 1))
        return
    fi

    printf '[OK] release directory exists: %s\n' "$release_dir"

    for file in "${REQUIRED_RELEASE_FILES[@]}"; do
        if [[ -f "$release_dir/$file" ]]; then
            printf '[OK] release directory contains %s\n' "$file"
        else
            printf '[FAIL] release directory contains %s\n' "$file" >&2
            printf '       missing file: %s/%s\n' "$release_dir" "$file" >&2
            failures=$((failures + 1))
        fi
    done
}

verify_install_layout() {
    local failures=0
    local agent_unit controller_unit agent_logrotate controller_logrotate agent_config controller_env
    local install_files_body restart_service_body

    agent_unit="$(emit_systemd_unit)"
    controller_unit="$(emit_controller_systemd_unit)"
    agent_logrotate="$(emit_logrotate_config)"
    controller_logrotate="$(emit_controller_logrotate_config)"
    agent_config="$(emit_default_config)"
    controller_env="$(emit_default_controller_env)"
    install_files_body="$(declare -f install_files)"
    restart_service_body="$(declare -f restart_service)"

    verify_required_release_file aria-agent
    verify_required_release_file aria-controller
    verify_required_release_file ariactl
    verify_required_release_file libebpf_firewall.so
    verify_required_release_file libebpf_firewall_perf.so

    verify_contains "$agent_unit" "ExecStart=/usr/local/bin/aria-agent --config /etc/aria-agent/config.toml" \
        "agent systemd unit starts aria-agent with config"
    verify_contains "$agent_unit" "ReadWritePaths=/sys/fs/bpf /var/lib/aria-agent /var/log/aria-agent" \
        "agent systemd unit can write bpffs, state, and logs"

    verify_contains "$controller_unit" "EnvironmentFile=-/etc/aria-controller/controller.env" \
        "controller systemd unit loads controller env file"
    verify_contains "$controller_unit" "ExecStart=/usr/local/bin/aria-controller" \
        "controller systemd unit starts aria-controller"
    verify_contains "$controller_unit" "ReadWritePaths=/var/lib/aria-controller /var/log/aria-controller" \
        "controller systemd unit can write state and logs"

    verify_contains "$agent_logrotate" "/var/log/aria-agent/aria-agent.log" \
        "agent logrotate targets agent file log"
    verify_contains "$controller_logrotate" "/var/log/aria-controller/aria-controller.log" \
        "controller logrotate targets controller file log"

    verify_contains "$agent_config" 'listen_addr = "127.0.0.1:8080"' \
        "agent default API bind uses port 8080"
    verify_contains "$agent_config" 'log_file_path = "/var/log/aria-agent/aria-agent.log"' \
        "agent default config points to file log"

    verify_contains "$controller_env" "ARIA_CONTROLLER_BIND=127.0.0.1:8180" \
        "controller default API bind uses port 8180"
    verify_contains "$controller_env" "ARIA_CONTROLLER_STATE_PATH=/var/lib/aria-controller/controller-state.json" \
        "controller default env points to state file"
    verify_contains "$controller_env" "ARIA_CONTROLLER_LOG_FILE_PATH=/var/log/aria-controller/aria-controller.log" \
        "controller default env points to file log"

    verify_contains "$install_files_body" "write_controller_systemd_unit" \
        "install flow writes controller systemd unit"
    verify_contains "$install_files_body" "write_controller_logrotate_config" \
        "install flow writes controller logrotate config"
    verify_contains "$install_files_body" "write_default_controller_env" \
        "install flow writes default controller env"
    verify_contains "$restart_service_body" 'if [[ "$START_CONTROLLER" -eq 1 ]]; then' \
        "controller service startup is gated by --start-controller"

    if [[ -n "$RELEASE_DIR" ]]; then
        verify_release_dir "$RELEASE_DIR"
    fi

    if (( failures > 0 )); then
        printf '\n%d install layout check(s) failed.\n' "$failures" >&2
        exit 1
    fi

    printf '\nInstall layout checks passed.\n'
}

install_files() {
    mkdir -p \
        "$INSTALL_BIN_DIR" \
        "$INSTALL_LIB_DIR" \
        "$CONFIG_DIR" \
        "$STATE_DIR" \
        "$LOG_DIR" \
        "$CONTROLLER_CONFIG_DIR" \
        "$CONTROLLER_STATE_DIR" \
        "$CONTROLLER_LOG_DIR"
    chmod 0755 "$LOG_DIR"
    chmod 0755 "$CONTROLLER_LOG_DIR"

    log "安装 aria-agent 到 $INSTALL_BIN_DIR"
    install -m 0755 "$TMP_DIR/aria-agent" "$INSTALL_BIN_DIR/aria-agent"

    log "安装 aria-controller 到 $INSTALL_BIN_DIR"
    install -m 0755 "$TMP_DIR/aria-controller" "$INSTALL_BIN_DIR/aria-controller"

    log "安装 ariactl 到 $INSTALL_BIN_DIR"
    install -m 0755 "$TMP_DIR/ariactl" "$INSTALL_BIN_DIR/ariactl"

    log "安装 libebpf_firewall.so 到 $INSTALL_LIB_DIR"
    install -m 0644 "$TMP_DIR/libebpf_firewall.so" "$INSTALL_LIB_DIR/libebpf_firewall.so"

    log "安装 libebpf_firewall_perf.so 到 $INSTALL_LIB_DIR"
    install -m 0644 "$TMP_DIR/libebpf_firewall_perf.so" "$INSTALL_LIB_DIR/libebpf_firewall_perf.so"

    log "写入/更新 systemd 单元: $SYSTEMD_UNIT"
    write_systemd_unit

    log "写入/更新 controller systemd 单元: $CONTROLLER_SYSTEMD_UNIT"
    write_controller_systemd_unit

    log "写入/更新 logrotate 配置: $LOGROTATE_FILE"
    write_logrotate_config

    log "写入/更新 controller logrotate 配置: $CONTROLLER_LOGROTATE_FILE"
    write_controller_logrotate_config

    if [[ ! -f "$CONFIG_FILE" || "$FORCE_CONFIG" -eq 1 ]]; then
        log "写入默认配置: $CONFIG_FILE"
        write_default_config
    else
        log "保留现有配置: $CONFIG_FILE"
    fi

    if [[ ! -f "$CONTROLLER_ENV_FILE" || "$FORCE_CONFIG" -eq 1 ]]; then
        log "写入默认 controller 环境配置: $CONTROLLER_ENV_FILE"
        write_default_controller_env
    else
        log "保留现有 controller 环境配置: $CONTROLLER_ENV_FILE"
    fi
}

show_installed_hashes() {
    log "安装后的文件校验:"
    sha256sum \
        "$INSTALL_BIN_DIR/aria-agent" \
        "$INSTALL_BIN_DIR/aria-controller" \
        "$INSTALL_BIN_DIR/ariactl" \
        "$INSTALL_LIB_DIR/libebpf_firewall.so" \
        "$INSTALL_LIB_DIR/libebpf_firewall_perf.so" | sed 's/^/  /'
}

restart_service() {
    if ! command -v systemctl >/dev/null 2>&1; then
        warn "系统没有 systemctl，已完成文件安装，请手动启动 aria-agent；如需 controller，请手动启动 /usr/local/bin/aria-controller"
        return 0
    fi

    log "重新加载 systemd 配置"
    systemctl daemon-reload

    log "启用 aria-agent.service"
    systemctl enable aria-agent.service >/dev/null

    if [[ "$START_CONTROLLER" -eq 1 ]]; then
        log "启用 aria-controller.service"
        systemctl enable aria-controller.service >/dev/null
    fi

    if [[ "$NO_START" -eq 1 ]]; then
        warn "按 --no-start 跳过服务启动/重启"
        return 0
    fi

    log "重启 aria-agent.service"
    if ! systemctl restart aria-agent.service; then
        warn "aria-agent 启动失败，最近日志如下:"
        journalctl -u aria-agent.service -n 50 --no-pager || true
        exit 1
    fi

    if [[ "$START_CONTROLLER" -eq 1 ]]; then
        log "重启 aria-controller.service"
        if ! systemctl restart aria-controller.service; then
            warn "aria-controller 启动失败，最近日志如下:"
            journalctl -u aria-controller.service -n 50 --no-pager || true
            exit 1
        fi
    fi

    local attempt
    for attempt in {1..15}; do
        if "$INSTALL_BIN_DIR/ariactl" health >/dev/null 2>&1; then
            log "aria-agent 已启动并通过健康检查"
            return 0
        fi
        sleep 1
    done

    warn "aria-agent 服务已启动，但健康检查未在超时时间内通过"
    systemctl --no-pager --full status aria-agent.service || true
    exit 1
}

main() {
    parse_args "$@"
    if [[ -n "$RELEASE_DIR" && "$VERIFY_LAYOUT" -ne 1 ]]; then
        die "--release-dir 只能配合 --verify-layout 使用"
    fi

    if [[ "$VERIFY_LAYOUT" -eq 1 ]]; then
        verify_install_layout
        exit 0
    fi

    check_root
    find_zip
    check_environment
    unpack_release
    backup_existing
    install_files
    show_installed_hashes
    restart_service

    log "安装/更新完成"
    printf '\n'
    printf '下一步建议:\n'
    printf '  1. 检查服务状态: systemctl status aria-agent --no-pager\n'
    printf '  2. 检查健康状态: ariactl health\n'
    printf '  3. 查看 journald 日志: journalctl -u aria-agent -n 50 --no-pager\n'
    printf '  4. 查看文件日志: tail -n 50 %s\n' "$LOG_FILE"
    printf '  5. 查看 controller 文件日志: tail -n 50 %s\n' "$CONTROLLER_LOG_FILE"
    printf '  6. 如需启动 controller: systemctl enable --now aria-controller.service\n'
    printf '  7. 如需首次部署，确认 /etc/aria-agent/config.toml 中的 iface_pattern\n'
    printf '\n'
}

main "$@"
