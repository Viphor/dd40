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
#   --debug              Enable bash -x tracing for debugging

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
#
# Uses awk instead of a grep pipeline to avoid set -euo pipefail triggering
# on a "no match" exit code from grep, which happens even when || true is
# appended because pipefail raises on intermediate stages.
config_token_file() {
    # Platform-specific config dir (mirrors what the dirs crate returns).
    local platform_config_dir
    if [[ "$(uname)" == "Darwin" ]]; then
        platform_config_dir="${HOME}/Library/Application Support/dd40"
    else
        platform_config_dir="${HOME}/.config/dd40"
    fi

    local config_candidates=(
        "${DD40_CONFIG:-}"
        "./config.toml"
        "${platform_config_dir}/config.toml"
    )

    local f val
    for f in "${config_candidates[@]}"; do
        # Use if-then rather than [[ ]] && continue: a failing [[ ]] in a
        # standalone && expression returns non-zero and triggers set -e.
        if [[ -z "$f" || ! -f "$f" ]]; then continue; fi
        # awk always exits 0 regardless of whether it matched, so it is safe
        # under set -e.  It finds the [auth] section, reads token_file, and
        # strips the surrounding quotes.
        val=$(awk '
            /^\[auth\]/         { in_auth=1; next }
            in_auth && /^\[/    { in_auth=0; next }
            in_auth && /^token_file[[:space:]]*=/ {
                sub(/^[^"]*"/, "")
                sub(/".*/, "")
                print
                exit
            }
        ' "$f")
        # Same reason: use if-then, not [[ ]] && ...
        if [[ -n "$val" ]]; then echo "$val"; return; fi
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
        --debug)  set -x; shift ;;
        -h|--help)
            sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
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
    # Fallback without jq: awk is used instead of grep|cut to avoid
    # set -e triggering on a non-matching grep exit code.
    TOKEN=$(echo "${RESPONSE}" | awk -F'"' '
        {
            for (i=1; i<=NF; i++) {
                if ($i == "access_token") { print $(i+2); exit }
            }
        }
    ')
fi

if [ -z "${TOKEN}" ] || [ "${TOKEN}" = "null" ]; then
    # Try to surface the Keycloak error message.
    if command -v jq >/dev/null 2>&1; then
        MSG=$(echo "${RESPONSE}" | jq -r '.error_description // .error // "unknown error"')
    else
        MSG=$(echo "${RESPONSE}" | awk -F'"' '
            {
                for (i=1; i<=NF; i++) {
                    if ($i == "error_description" || $i == "error") { print $(i+2); exit }
                }
            }
        ')
        MSG="${MSG:-${RESPONSE}}"
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
