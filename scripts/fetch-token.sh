#!/usr/bin/env bash
# Fetch a JWT from the local Keycloak instance and write it to the path
# configured in auth.token_file (or ~/.dd40/token.jwt by default).
#
# Usage:
#   ./scripts/fetch-token.sh                        # uses defaults
#   ./scripts/fetch-token.sh --user bob --pass s3cr3t
#   ./scripts/fetch-token.sh --user bob             # prompts for password
#
# Options:
#   --user <username>    Keycloak username (default: testuser)
#   --pass <password>    Password (default: prompted if not given)
#   --out  <path>        Override output path (default: from config or ~/.dd40/token.jwt)
#   --host <url>         Keycloak base URL (default: http://localhost:8080)
#   --realm <realm>      Keycloak realm (default: dd40)
#   --client <client_id> Client ID (default: dd40)

set -euo pipefail

# ── defaults ─────────────────────────────────────────────────────────────────

HOST="http://localhost:8080"
REALM="dd40"
CLIENT_ID="dd40"
USERNAME="testuser"
PASSWORD=""
OUT_PATH=""

# ── helpers ──────────────────────────────────────────────────────────────────

log() { printf '\033[1;34m[fetch-token]\033[0m %s\n' "$*"; }
ok()  { printf '\033[1;32m[fetch-token]\033[0m %s\n' "$*"; }
err() { printf '\033[1;31m[fetch-token]\033[0m %s\n' "$*" >&2; }

# Read token_file path from the first config.toml we can find.
config_token_file() {
    local config_candidates=(
        "${DD40_CONFIG:-}"
        "./config.toml"
        "${HOME}/.config/dd40/config.toml"
    )
    for f in "${config_candidates[@]}"; do
        [ -z "$f" ] && continue
        [ -f "$f" ] || continue
        # Extract auth.token_file = "..." from a TOML file (simple grep, no parser needed).
        local val
        val=$(grep -A20 '^\[auth\]' "$f" 2>/dev/null \
            | grep '^token_file' \
            | head -1 \
            | sed 's/.*=[ ]*"\(.*\)"/\1/')
        [ -n "$val" ] && echo "$val" && return
    done
}

# ── argument parsing ──────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user)   USERNAME="$2"; shift 2 ;;
        --pass)   PASSWORD="$2"; shift 2 ;;
        --out)    OUT_PATH="$2"; shift 2 ;;
        --host)   HOST="$2"; shift 2 ;;
        --realm)  REALM="$2"; shift 2 ;;
        --client) CLIENT_ID="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,20p' "$0" | sed 's/^# \?//'
            exit 0 ;;
        *)
            err "Unknown argument: $1"
            exit 1 ;;
    esac
done

# ── resolve output path ───────────────────────────────────────────────────────

if [ -z "${OUT_PATH}" ]; then
    OUT_PATH=$(config_token_file)
fi
if [ -z "${OUT_PATH}" ]; then
    OUT_PATH="${HOME}/.dd40/token.jwt"
fi

# Expand ~ manually (not all shells/contexts expand it in variables).
OUT_PATH="${OUT_PATH/#\~/$HOME}"

# ── prompt for password if not provided ──────────────────────────────────────

if [ -z "${PASSWORD}" ]; then
    printf "Password for '%s': " "${USERNAME}"
    read -rs PASSWORD
    echo
fi

# ── fetch token ──────────────────────────────────────────────────────────────

TOKEN_URL="${HOST}/realms/${REALM}/protocol/openid-connect/token"

log "Fetching token from ${TOKEN_URL} ..."

RESPONSE=$(curl -sf \
    -X POST "${TOKEN_URL}" \
    -d "client_id=${CLIENT_ID}" \
    -d "grant_type=password" \
    -d "username=${USERNAME}" \
    -d "password=${PASSWORD}" \
    2>&1) || {
    err "curl failed — is Keycloak running? (./scripts/keycloak-start.sh)"
    exit 1
}

# Extract the access_token field.
if command -v jq >/dev/null 2>&1; then
    TOKEN=$(echo "${RESPONSE}" | jq -r '.access_token // empty')
else
    # Fallback without jq: naive grep.
    TOKEN=$(echo "${RESPONSE}" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
fi

if [ -z "${TOKEN}" ] || [ "${TOKEN}" = "null" ]; then
    # Try to surface the Keycloak error message.
    if command -v jq >/dev/null 2>&1; then
        MSG=$(echo "${RESPONSE}" | jq -r '.error_description // .error // "unknown error"')
    else
        MSG="${RESPONSE}"
    fi
    err "Failed to obtain token: ${MSG}"
    exit 1
fi

# ── write token file ──────────────────────────────────────────────────────────

mkdir -p "$(dirname "${OUT_PATH}")"
printf '%s' "${TOKEN}" > "${OUT_PATH}"
chmod 600 "${OUT_PATH}"

ok "Token written to ${OUT_PATH}"

# Show expiry without jq if possible.
if command -v jq >/dev/null 2>&1; then
    EXP=$(echo "${RESPONSE}" | jq -r '.expires_in // empty')
    if [ -n "${EXP}" ]; then
        ok "Expires in ${EXP} seconds"
    fi
fi

ok "Add to your config:"
ok "  [auth]"
ok "  token_file = \"${OUT_PATH}\""
