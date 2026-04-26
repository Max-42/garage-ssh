#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ENGINE=""
IMAGE_TAG="garage-ssh-gate:local"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --engine)
      ENGINE="$2"
      shift 2
      ;;
    --image)
      IMAGE_TAG="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "${ENGINE}" ]]; then
  if command -v podman >/dev/null 2>&1; then
    ENGINE="podman"
  elif command -v docker >/dev/null 2>&1; then
    ENGINE="docker"
  else
    echo "Neither podman nor docker found" >&2
    exit 1
  fi
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi
if ! command -v ssh >/dev/null 2>&1 || ! command -v ssh-keygen >/dev/null 2>&1; then
  echo "openssh client tools are required" >&2
  exit 1
fi

TEST_ID="garage-e2e-$(date +%s)-$RANDOM"
NETWORK="${TEST_ID}-net"
APP_NAME="${TEST_ID}-app"
HOOK_NAME="${TEST_ID}-hook"
TMP_DIR="$(mktemp -d)"
DATA_DIR="${TMP_DIR}/data"
mkdir -p "${DATA_DIR}"

APP_PORT_SSH="12242"
APP_PORT_WEB="18099"
HOOK_PORT="18080"
TEST_PASSED="false"

log() {
  echo "[e2e] $*"
}

fail() {
  echo "[e2e][FAIL] $*" >&2
  exit 1
}

assert_eq() {
  local expected="$1"
  local got="$2"
  local msg="$3"
  if [[ "${expected}" != "${got}" ]]; then
    fail "${msg} (expected='${expected}' got='${got}')"
  fi
}

cleanup() {
  set +e
  if [[ "${TEST_PASSED}" != "true" ]]; then
    ${ENGINE} logs "${APP_NAME}" >/dev/null 2>&1 && ${ENGINE} logs "${APP_NAME}" | tail -n 120 || true
  fi
  ${ENGINE} rm -f "${APP_NAME}" >/dev/null 2>&1 || true
  ${ENGINE} rm -f "${HOOK_NAME}" >/dev/null 2>&1 || true
  ${ENGINE} network rm "${NETWORK}" >/dev/null 2>&1 || true
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

log "Using container engine: ${ENGINE}"
log "Using image: ${IMAGE_TAG}"

${ENGINE} image inspect "${IMAGE_TAG}" >/dev/null 2>&1 || fail "Image '${IMAGE_TAG}' does not exist"

log "Creating isolated test network"
${ENGINE} network create "${NETWORK}" >/dev/null

log "Starting webhook listener container"
${ENGINE} run -d --name "${HOOK_NAME}" --network "${NETWORK}" \
  -p "${HOOK_PORT}:${HOOK_PORT}" \
  -v "${SCRIPT_DIR}/webhook_listener.py:/app/webhook_listener.py:ro" \
  -e PORT="${HOOK_PORT}" \
  "python:3.12-alpine" \
  python /app/webhook_listener.py >/dev/null

for _ in $(seq 1 30); do
  if ${ENGINE} logs "${HOOK_NAME}" 2>/dev/null | grep -q "LISTENING"; then
    break
  fi
  sleep 1
done
${ENGINE} logs "${HOOK_NAME}" | grep -q "LISTENING" || fail "Webhook listener did not start"

cat > "${DATA_DIR}/options.json" <<JSON
{
  "ssh_port": 2242,
  "webhook_url": "http://${HOOK_NAME}:${HOOK_PORT}/hook",
  "home_latitude": 0.0,
  "home_longitude": 0.0,
  "geofence_radius_km": 15,
  "geofence_override_timeout_sec": 45,
  "tofu_timeout_sec": 60,
  "untrusted_key_retention_days": 21,
  "expected_json_version": "1.0.1",
  "log_level": "info",
  "host_key_pem": ""
}
JSON

log "Starting app container"
${ENGINE} run -d --name "${APP_NAME}" --network "${NETWORK}" \
  -p "${APP_PORT_SSH}:2242" -p "${APP_PORT_WEB}:8099" \
  -v "${DATA_DIR}:/data" \
  "${IMAGE_TAG}" \
  /usr/bin/garage-ssh-gate >/dev/null

for _ in $(seq 1 60); do
  if curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/state" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

STATE_JSON="$(curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/state")"
echo "${STATE_JSON}" | jq -e '.trusted_keys_count == 0 and .untrusted_keys_count == 0 and .tofu_active == false' >/dev/null \
  || fail "Initial /api/state did not return expected structure/content"
log "Web server on port ${APP_PORT_WEB} responds with useful API state"

cat > "${TMP_DIR}/payload.json" <<'JSON'
{
  "time": { "value": "2026-04-26T12:34:56Z", "format": "ISO 8601" },
  "device": {
    "version": "17.2",
    "model": "iPhone",
    "hostname": "shortcut-test",
    "name": "E2E Device",
    "os": "iOS",
    "build": "21D61"
  },
  "position": {
    "longitude": 0.0,
    "latitude": 0.0,
    "altitude": 0.0
  },
  "version": "1.0.1"
}
JSON

log "Generating client keys"
ssh-keygen -t ed25519 -N "" -f "${TMP_DIR}/trusted_key" -C "trusted@test" >/dev/null
ssh-keygen -t ed25519 -N "" -f "${TMP_DIR}/bad_key" -C "bad@test" >/dev/null

SSH_COMMON=(
  -F /dev/null
  -p "${APP_PORT_SSH}"
  -o StrictHostKeyChecking=no
  -o UserKnownHostsFile="${TMP_DIR}/known_hosts"
  -o IdentitiesOnly=yes
  -o IdentityAgent=none
  -o BatchMode=yes
  -o ConnectTimeout=8
)

log "Checking TOFU starts disabled"
TOFU_STATUS="$(curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/tofu/status")"
echo "${TOFU_STATUS}" | jq -e '.active == false' >/dev/null || fail "TOFU should be inactive initially"

log "Enable TOFU via API"
curl -fsS -X POST "http://127.0.0.1:${APP_PORT_WEB}/api/tofu/activate" >/dev/null
TOFU_STATUS="$(curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/tofu/status")"
echo "${TOFU_STATUS}" | jq -e '.active == true' >/dev/null || fail "TOFU activation failed"

log "Connect with new random key while TOFU active (should succeed + trigger webhook)"
SSH_OUT1="$(ssh "${SSH_COMMON[@]}" -i "${TMP_DIR}/trusted_key" test@127.0.0.1 < "${TMP_DIR}/payload.json" 2>&1 || true)"
echo "${SSH_OUT1}" | grep -q "SUCCESS: Garage door opening!" \
  || fail "TOFU onboarding connection did not succeed"

TRUSTED_KEYS="$(curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/trusted-keys")"
echo "${TRUSTED_KEYS}" | jq -e 'length == 1 and .[0].username == "test"' >/dev/null \
  || fail "TOFU did not move new key into trusted keys"

STATE_AFTER_TOFU="$(curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/state")"
echo "${STATE_AFTER_TOFU}" | jq -e '.trusted_keys_count == 1 and .untrusted_keys_count == 0' >/dev/null \
  || fail "State after TOFU success is unexpected"

HOOK_COUNT="$(curl -fsS "http://127.0.0.1:${HOOK_PORT}/count" | jq -r '.count')"
assert_eq "1" "${HOOK_COUNT}" "Webhook should be triggered exactly once after TOFU success"

log "Disable TOFU"
curl -fsS -X POST "http://127.0.0.1:${APP_PORT_WEB}/api/tofu/deactivate" >/dev/null
TOFU_STATUS="$(curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/tofu/status")"
echo "${TOFU_STATUS}" | jq -e '.active == false' >/dev/null || fail "TOFU deactivation failed"

log "Reconnect with authenticated key after TOFU disabled (should still succeed + webhook)"
SSH_OUT2="$(ssh "${SSH_COMMON[@]}" -i "${TMP_DIR}/trusted_key" test@127.0.0.1 < "${TMP_DIR}/payload.json" 2>&1 || true)"
echo "${SSH_OUT2}" | grep -q "SUCCESS: Garage door opening!" \
  || fail "Trusted key did not succeed after TOFU disabled"
HOOK_COUNT="$(curl -fsS "http://127.0.0.1:${HOOK_PORT}/count" | jq -r '.count')"
assert_eq "2" "${HOOK_COUNT}" "Webhook should be triggered again for trusted key success"

log "Connect with unauthenticated key while TOFU disabled (should fail + no webhook)"
SSH_OUT3="$(ssh "${SSH_COMMON[@]}" -i "${TMP_DIR}/bad_key" test@127.0.0.1 < "${TMP_DIR}/payload.json" 2>&1 || true)"
echo "${SSH_OUT3}" | grep -q "FAIL: Key not trusted" \
  || fail "Untrusted key did not fail as expected"

UNTRUSTED_KEYS="$(curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/untrusted-keys")"
echo "${UNTRUSTED_KEYS}" | jq -e 'length >= 1' >/dev/null \
  || fail "Untrusted key was not recorded in untrusted key list"

RECENT_LOGS="$(curl -fsS "http://127.0.0.1:${APP_PORT_WEB}/api/logs")"
echo "${RECENT_LOGS}" | jq -e 'map(select(.result | contains("UNTRUSTED_KEY"))) | length >= 1' >/dev/null \
  || fail "Expected UNTRUSTED_KEY log entry was not found"

HOOK_COUNT="$(curl -fsS "http://127.0.0.1:${HOOK_PORT}/count" | jq -r '.count')"
assert_eq "2" "${HOOK_COUNT}" "Webhook must NOT trigger for untrusted key"

log "Checking that SSH port ${APP_PORT_SSH} remains reachable"
timeout 5 bash -c "</dev/tcp/127.0.0.1/${APP_PORT_SSH}" >/dev/null 2>&1 \
  || fail "SSH port ${APP_PORT_SSH} is not reachable"

log "E2E integration tests passed"
TEST_PASSED="true"
