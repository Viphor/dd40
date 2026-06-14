#!/usr/bin/env bash
# Start a local Keycloak instance for dd40 development.
#
# First run: starts Keycloak and then bootstraps the dd40 realm, client, and
# a test user via the Admin REST API. Subsequent runs just start the container
# if it isn't already running.
#
# Requirements: docker
#
# Keycloak will be available at http://localhost:8080
# Admin console: http://localhost:8080/admin  (admin / admin)

set -euo pipefail

CONTAINER_NAME="dd40-keycloak"
IMAGE="quay.io/keycloak/keycloak:latest"
PORT=8080
ADMIN_USER="admin"
ADMIN_PASS="admin"
REALM="dd40"
CLIENT_ID="dd40"
TEST_USER="testuser"
TEST_PASS="testpass"

# ── helpers ──────────────────────────────────────────────────────────────────

log() { printf '\033[1;34m[keycloak]\033[0m %s\n' "$*"; }
ok()  { printf '\033[1;32m[keycloak]\033[0m %s\n' "$*"; }
err() { printf '\033[1;31m[keycloak]\033[0m %s\n' "$*" >&2; }

# Detect container runtime: prefer podman if available, fall back to docker.
if command -v podman >/dev/null 2>&1; then
    RUNTIME="podman"
elif command -v docker >/dev/null 2>&1; then
    RUNTIME="docker"
else
    err "Neither podman nor docker found in PATH"
    exit 1
fi
log "Using container runtime: ${RUNTIME}"

wait_for_keycloak() {
    log "Waiting for Keycloak to become ready..."
    local tries=0
    until curl -sf "http://localhost:${PORT}/health/ready" >/dev/null 2>&1; do
        tries=$((tries + 1))
        if [ "$tries" -ge 120 ]; then
            err "Keycloak did not become ready after 120 seconds"
            exit 1
        fi
        sleep 1
    done
    ok "Keycloak is ready"
}

admin_token() {
    local response
    response=$(curl -sf \
        -d "client_id=admin-cli" \
        -d "username=${ADMIN_USER}" \
        -d "password=${ADMIN_PASS}" \
        -d "grant_type=password" \
        "http://localhost:${PORT}/realms/master/protocol/openid-connect/token")
    # awk instead of grep|cut: avoids set -e triggering on a non-matching grep.
    echo "$response" | awk -F'"' '
        { for (i=1; i<=NF; i++) { if ($i == "access_token") { print $(i+2); exit } } }
    '
}

kcadm() {
    local token="$1"; shift
    curl -sf \
        -H "Authorization: Bearer ${token}" \
        -H "Content-Type: application/json" \
        "$@"
}

# ── main ─────────────────────────────────────────────────────────────────────

# Check if the container already exists.
if "${RUNTIME}" inspect "${CONTAINER_NAME}" >/dev/null 2>&1; then
    STATUS=$("${RUNTIME}" inspect -f '{{.State.Status}}' "${CONTAINER_NAME}")
    if [ "${STATUS}" = "running" ]; then
        ok "Container '${CONTAINER_NAME}' is already running"
        ok "Admin console: http://localhost:${PORT}/admin"
        exit 0
    fi
    log "Starting existing container '${CONTAINER_NAME}'"
    "${RUNTIME}" start "${CONTAINER_NAME}"
    wait_for_keycloak
    ok "Admin console: http://localhost:${PORT}/admin"
    exit 0
fi

# Fresh start.
log "Pulling ${IMAGE} ..."
"${RUNTIME}" pull "${IMAGE}"

log "Starting Keycloak on port ${PORT} ..."
"${RUNTIME}" run -d \
    --name "${CONTAINER_NAME}" \
    -p "${PORT}:8080" \
    -e KC_BOOTSTRAP_ADMIN_USERNAME="${ADMIN_USER}" \
    -e KC_BOOTSTRAP_ADMIN_PASSWORD="${ADMIN_PASS}" \
    "${IMAGE}" start-dev

wait_for_keycloak

# Bootstrap realm, client, and test user.
log "Bootstrapping realm '${REALM}' ..."
TOKEN=$(admin_token)

# Create realm.
kcadm "$TOKEN" -X POST \
    "http://localhost:${PORT}/admin/realms" \
    -d "{\"realm\":\"${REALM}\",\"enabled\":true}" \
    && log "  realm '${REALM}' created" || log "  realm '${REALM}' may already exist"

# Create public client (no secret needed for password-grant testing).
kcadm "$TOKEN" -X POST \
    "http://localhost:${PORT}/admin/realms/${REALM}/clients" \
    -d "{
        \"clientId\":\"${CLIENT_ID}\",
        \"publicClient\":true,
        \"directAccessGrantsEnabled\":true,
        \"enabled\":true
    }" \
    && log "  client '${CLIENT_ID}' created" || log "  client '${CLIENT_ID}' may already exist"

# Create test user.
kcadm "$TOKEN" -X POST \
    "http://localhost:${PORT}/admin/realms/${REALM}/users" \
    -d "{
        \"username\":\"${TEST_USER}\",
        \"enabled\":true,
        \"firstName\":\"Test\",
        \"lastName\":\"User\",
        \"attributes\":{\"preferred_username\":[\"${TEST_USER}\"]},
        \"credentials\":[{
            \"type\":\"password\",
            \"value\":\"${TEST_PASS}\",
            \"temporary\":false
        }]
    }" \
    && log "  user '${TEST_USER}' created" || log "  user '${TEST_USER}' may already exist"

echo
ok "Keycloak is ready"
ok "  Admin console : http://localhost:${PORT}/admin  (${ADMIN_USER} / ${ADMIN_PASS})"
ok "  Realm         : ${REALM}"
ok "  Client        : ${CLIENT_ID}"
ok "  Test user     : ${TEST_USER} / ${TEST_PASS}"
echo
ok "Fetch a token with:"
ok "  ./scripts/fetch-token.sh"
