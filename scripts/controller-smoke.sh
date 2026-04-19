#!/usr/bin/env bash
set -euo pipefail

CONTROLLER_URL="${CONTROLLER_URL:-http://127.0.0.1:8180}"
PREFIX=""
TENANT_ID=""
NODE_ID=""
NETWORK_ID=""

usage() {
    cat <<'EOF'
Aria Controller API smoke check

Usage:
  scripts/controller-smoke.sh
  scripts/controller-smoke.sh --controller-url http://127.0.0.1:8180
  scripts/controller-smoke.sh --prefix smoke-001

Options:
  --controller-url URL  Controller base URL, default http://127.0.0.1:8180
  --prefix PREFIX      Resource suffix prefix, default smoke-<unix timestamp>
  --tenant-id ID       Tenant ID to create, default tenant-<prefix>
  --node-id ID         Node ID to create/register, default node-<prefix>
  --network-id ID      Network ID to create, default network-<prefix>
  -h, --help           Show this help

This script only exercises controller northbound/southbound APIs. It does not
load eBPF programs and does not change the local agent datapath.
EOF
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        printf '[ERROR] missing command: %s\n' "$1" >&2
        exit 1
    }
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --controller-url)
                [[ $# -ge 2 ]] || {
                    printf '[ERROR] --controller-url requires a value\n' >&2
                    exit 1
                }
                CONTROLLER_URL="$2"
                shift 2
                ;;
            --prefix)
                [[ $# -ge 2 ]] || {
                    printf '[ERROR] --prefix requires a value\n' >&2
                    exit 1
                }
                PREFIX="$2"
                shift 2
                ;;
            --tenant-id)
                [[ $# -ge 2 ]] || {
                    printf '[ERROR] --tenant-id requires a value\n' >&2
                    exit 1
                }
                TENANT_ID="$2"
                shift 2
                ;;
            --node-id)
                [[ $# -ge 2 ]] || {
                    printf '[ERROR] --node-id requires a value\n' >&2
                    exit 1
                }
                NODE_ID="$2"
                shift 2
                ;;
            --network-id)
                [[ $# -ge 2 ]] || {
                    printf '[ERROR] --network-id requires a value\n' >&2
                    exit 1
                }
                NETWORK_ID="$2"
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                printf '[ERROR] unknown argument: %s\n' "$1" >&2
                exit 1
                ;;
        esac
    done
}

post_json() {
    local path="$1"
    local payload="$2"
    curl -fsS -X POST "$CONTROLLER_URL$path" \
        -H 'content-type: application/json' \
        -d "$payload"
}

get_json() {
    local path="$1"
    curl -fsS "$CONTROLLER_URL$path"
}

main() {
    parse_args "$@"
    require_cmd curl
    require_cmd jq
    require_cmd date

    local desired_state generation object_counts now status sync_state

    CONTROLLER_URL="${CONTROLLER_URL%/}"
    PREFIX="${PREFIX:-smoke-$(date +%s)}"
    TENANT_ID="${TENANT_ID:-tenant-$PREFIX}"
    NODE_ID="${NODE_ID:-node-$PREFIX}"
    NETWORK_ID="${NETWORK_ID:-network-$PREFIX}"

    printf '[INFO] controller: %s\n' "$CONTROLLER_URL"
    printf '[INFO] tenant: %s\n' "$TENANT_ID"
    printf '[INFO] node: %s\n' "$NODE_ID"
    printf '[INFO] network: %s\n' "$NETWORK_ID"

    get_json "/api/v1/health" >/dev/null
    printf '[OK] controller health endpoint responded\n'

    post_json "/api/v1/tenants" "$(
        jq -nc --arg id "$TENANT_ID" '{
            metadata: {id: $id},
            spec: {
                name: $id,
                description: "controller smoke test",
                quotas: {}
            }
        }'
    )" >/dev/null
    printf '[OK] tenant created\n'

    post_json "/api/v1/nodes" "$(
        jq -nc --arg id "$NODE_ID" '{
            metadata: {id: $id, labels: {role: "smoke"}},
            spec: {
                name: $id,
                mgmt_address: "127.0.0.1",
                az: "local"
            }
        }'
    )" >/dev/null
    printf '[OK] node created\n'

    post_json "/api/v1/networks" "$(
        jq -nc --arg id "$NETWORK_ID" --arg tenant "$TENANT_ID" '{
            metadata: {id: $id},
            spec: {
                tenant_id: $tenant,
                name: $id,
                network_type: "l3",
                ipv4_enabled: true,
                ipv6_enabled: false,
                route_mode: "native"
            }
        }'
    )" >/dev/null
    printf '[OK] network created\n'

    post_json "/api/v1/southbound/nodes/$NODE_ID/register" "$(
        jq -nc --arg id "$NODE_ID" '{
            info: {
                node_id: $id,
                hostname: $id,
                agent_version: "0.10.0",
                kernel_version: "smoke",
                addresses: [{kind: "management", value: "127.0.0.1"}],
                labels: {role: "smoke"}
            },
            capability: {
                supported_hooks: ["xdp", "tc"],
                supports_xdp: true,
                supports_tc: true,
                supports_socket_lb: false,
                supports_trace_ringbuf: true,
                supports_nat: true,
                supports_lb: true,
                supports_encap: false,
                supports_qos_shaping: true,
                limits: {},
                observability_profile: "smoke"
            }
        }'
    )" >/dev/null
    printf '[OK] southbound registration accepted\n'

    desired_state="$(get_json "/api/v1/southbound/nodes/$NODE_ID/desired-state")"
    generation="$(jq -r '.generation' <<<"$desired_state")"
    object_counts="$(jq -c '.object_counts' <<<"$desired_state")"
    now="$(date +%s)"
    printf '[OK] desired state fetched: generation=%s\n' "$generation"

    post_json "/api/v1/southbound/nodes/$NODE_ID/apply-status" "$(
        jq -nc \
            --arg generation "$generation" \
            --arg applied_at "$now" \
            --argjson compiled_objects "$object_counts" \
            '{
                generation: $generation,
                status: "applied",
                applied_at: $applied_at,
                compiled_objects: $compiled_objects,
                domain_statuses: [],
                failed_objects: [],
                warnings: [],
                degraded_reasons: []
            }'
    )" >/dev/null
    printf '[OK] apply status accepted\n'

    post_json "/api/v1/southbound/nodes/$NODE_ID/heartbeat" "$(
        jq -nc --arg now "$now" '{
            agent_uptime: 1,
            datapath_ready: true,
            attached_ports: 0,
            event_queue_depth: 0,
            wal_health: "ok",
            last_reconcile_at: $now,
            last_error: null
        }'
    )" >/dev/null
    printf '[OK] heartbeat accepted\n'

    status="$(get_json "/api/v1/southbound/nodes/$NODE_ID/status")"
    sync_state="$(jq -r '.sync_status.state' <<<"$status")"
    if [[ "$sync_state" != "in_sync" ]]; then
        printf '[ERROR] expected sync_status.state=in_sync, got %s\n' "$sync_state" >&2
        jq . <<<"$status" >&2
        exit 1
    fi

    printf '[OK] southbound status is in_sync\n'
    jq '{node_id, desired_generation, sync_status, pending_object_counts}' <<<"$status"
}

main "$@"
