#!/usr/bin/env bash
set -euo pipefail

CONTROLLER_URL="${CONTROLLER_URL:-http://127.0.0.1:8180}"
PREFIX=""
TENANT_ID=""
NODE_ID=""
NETWORK_ID=""
CLEANUP=0
DRY_RUN=0

CREATED_TENANT=0
CREATED_NODE=0
CREATED_NETWORK=0
CLEANUP_RUNNING=0

usage() {
    cat <<'EOF'
Aria Controller API smoke check

Usage:
  scripts/controller-smoke.sh
  scripts/controller-smoke.sh --controller-url http://127.0.0.1:8180
  scripts/controller-smoke.sh --prefix smoke-001 --cleanup
  scripts/controller-smoke.sh --dry-run

Options:
  --controller-url URL  Controller base URL, default http://127.0.0.1:8180
  --prefix PREFIX      Resource suffix prefix, default smoke-<unix timestamp>
  --tenant-id ID       Tenant ID to create, default tenant-<prefix>
  --node-id ID         Node ID to create/register, default node-<prefix>
  --network-id ID      Network ID to create, default network-<prefix>
  --cleanup            Delete created network/node/tenant before exit
  --dry-run            Generate and validate JSON payloads without HTTP calls
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
            --cleanup)
                CLEANUP=1
                shift
                ;;
            --dry-run)
                DRY_RUN=1
                shift
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

tenant_payload() {
    jq -nc --arg id "$TENANT_ID" '{
        metadata: {id: $id},
        spec: {
            name: $id,
            description: "controller smoke test",
            quotas: {}
        }
    }'
}

node_payload() {
    jq -nc --arg id "$NODE_ID" '{
        metadata: {id: $id, labels: {role: "smoke"}},
        spec: {
            name: $id,
            mgmt_address: "127.0.0.1",
            az: "local"
        }
    }'
}

network_payload() {
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
}

registration_payload() {
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
}

apply_status_payload() {
    local generation="$1"
    local applied_at="$2"
    local compiled_objects="$3"

    jq -nc \
        --arg generation "$generation" \
        --arg applied_at "$applied_at" \
        --argjson compiled_objects "$compiled_objects" \
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
}

heartbeat_payload() {
    local now="$1"

    jq -nc --arg now "$now" '{
        agent_uptime: 1,
        datapath_ready: true,
        attached_ports: 0,
        event_queue_depth: 0,
        wal_health: "ok",
        last_reconcile_at: $now,
        last_error: null
    }'
}

validate_payload() {
    local name="$1"
    local payload="$2"
    local filter="$3"

    if jq -e "$filter" >/dev/null <<<"$payload"; then
        printf '[OK] %s payload validated\n' "$name"
    else
        printf '[ERROR] %s payload failed validation\n' "$name" >&2
        jq . <<<"$payload" >&2
        return 1
    fi
}

validate_payloads() {
    local sample_generation="dry-run-generation"
    local sample_now="0"
    local sample_counts='{"tenants":1,"networks":1,"ports":0,"security_groups":0,"route_tables":0,"ip_groups":0,"network_policies":0,"qos_policies":0,"mirror_policies":0,"service_chains":0,"health_checks":0,"backend_sets":0,"services":0,"node_configs":0,"deletes":0}'

    validate_payload "tenant" "$(tenant_payload)" \
        ".metadata.id == \"$TENANT_ID\" and .spec.name == \"$TENANT_ID\" and (.spec.quotas | type) == \"object\""
    validate_payload "node" "$(node_payload)" \
        ".metadata.id == \"$NODE_ID\" and .spec.name == \"$NODE_ID\" and .spec.mgmt_address == \"127.0.0.1\""
    validate_payload "network" "$(network_payload)" \
        ".metadata.id == \"$NETWORK_ID\" and .spec.tenant_id == \"$TENANT_ID\" and .spec.route_mode == \"native\""
    validate_payload "registration" "$(registration_payload)" \
        ".info.node_id == \"$NODE_ID\" and .capability.supports_xdp == true and .capability.supports_tc == true"
    validate_payload "apply-status" "$(apply_status_payload "$sample_generation" "$sample_now" "$sample_counts")" \
        ".generation == \"$sample_generation\" and .status == \"applied\" and (.compiled_objects | type) == \"object\""
    validate_payload "heartbeat" "$(heartbeat_payload "$sample_now")" \
        ".datapath_ready == true and .wal_health == \"ok\" and .last_error == null"
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

delete_resource() {
    local kind="$1"
    local path="$2"
    local status

    status="$(curl -sS -o /dev/null -w '%{http_code}' -X DELETE "$CONTROLLER_URL$path" || printf '000')"
    case "$status" in
        200|204)
            printf '[OK] %s deleted\n' "$kind"
            ;;
        404)
            printf '[OK] %s already absent\n' "$kind"
            ;;
        *)
            printf '[WARN] failed to delete %s: http_status=%s\n' "$kind" "$status" >&2
            return 1
            ;;
    esac
}

cleanup_resources() {
    local failures=0

    [[ "$CLEANUP_RUNNING" -eq 0 ]] || return 0
    CLEANUP_RUNNING=1

    if [[ "$CREATED_NETWORK" -eq 1 ]]; then
        delete_resource "network $NETWORK_ID" "/api/v1/networks/$NETWORK_ID" || failures=$((failures + 1))
    fi
    if [[ "$CREATED_NODE" -eq 1 ]]; then
        delete_resource "node $NODE_ID" "/api/v1/nodes/$NODE_ID" || failures=$((failures + 1))
    fi
    if [[ "$CREATED_TENANT" -eq 1 ]]; then
        delete_resource "tenant $TENANT_ID" "/api/v1/tenants/$TENANT_ID" || failures=$((failures + 1))
    fi

    CLEANUP_RUNNING=0
    [[ "$failures" -eq 0 ]]
}

cleanup_on_exit() {
    local status=$?
    if [[ "$CLEANUP" -eq 1 && "$DRY_RUN" -eq 0 ]]; then
        cleanup_resources || {
            [[ "$status" -eq 0 ]] && status=1
        }
    fi
    exit "$status"
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

    if [[ "$DRY_RUN" -eq 1 ]]; then
        validate_payloads
        printf '[OK] dry-run payload validation complete\n'
        return 0
    fi

    trap cleanup_on_exit EXIT

    get_json "/api/v1/health" >/dev/null
    printf '[OK] controller health endpoint responded\n'

    post_json "/api/v1/tenants" "$(tenant_payload)" >/dev/null
    CREATED_TENANT=1
    printf '[OK] tenant created\n'

    post_json "/api/v1/nodes" "$(node_payload)" >/dev/null
    CREATED_NODE=1
    printf '[OK] node created\n'

    post_json "/api/v1/networks" "$(network_payload)" >/dev/null
    CREATED_NETWORK=1
    printf '[OK] network created\n'

    post_json "/api/v1/southbound/nodes/$NODE_ID/register" "$(registration_payload)" >/dev/null
    printf '[OK] southbound registration accepted\n'

    desired_state="$(get_json "/api/v1/southbound/nodes/$NODE_ID/desired-state")"
    generation="$(jq -r '.generation' <<<"$desired_state")"
    object_counts="$(jq -c '.object_counts' <<<"$desired_state")"
    now="$(date +%s)"
    printf '[OK] desired state fetched: generation=%s\n' "$generation"

    post_json "/api/v1/southbound/nodes/$NODE_ID/apply-status" \
        "$(apply_status_payload "$generation" "$now" "$object_counts")" >/dev/null
    printf '[OK] apply status accepted\n'

    post_json "/api/v1/southbound/nodes/$NODE_ID/heartbeat" "$(heartbeat_payload "$now")" >/dev/null
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
