#!/usr/bin/env bash
# chaos/scenarios.sh
#
# Loops through chaos scenarios against rusty-sched + Toxiproxy + Postgres
# (see docker-compose.chaos.yml). For each scenario we inject a fault,
# observe rusty-sched's /readyz behavior, then clean up and confirm recovery.
#
# Exit status:
#   0  every scenario PASS
#   1  one or more scenarios FAIL
#
# Required env (defaults match the compose file):
#   TOXIPROXY_URL   default http://toxiproxy:8474
#   RSCHED_URL      default http://rusty-sched:8080
#   POSTGRES_HOST   default postgres

set -uo pipefail

TOXIPROXY_URL="${TOXIPROXY_URL:-http://toxiproxy:8474}"
RSCHED_URL="${RSCHED_URL:-http://rusty-sched:8080}"
POSTGRES_HOST="${POSTGRES_HOST:-postgres}"

PASS=0
FAIL=0
RESULTS=()

log()  { printf '[chaos] %s\n' "$*"; }
record() {
  local name="$1" status="$2" detail="$3"
  RESULTS+=("$(printf '%-32s  %-4s  %s' "$name" "$status" "$detail")")
  if [ "$status" = "PASS" ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi
}

# Wait for /readyz to return 200 within $1 seconds.
wait_ready() {
  local deadline=$(( $(date +%s) + ${1:-60} ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if curl -fsS -o /dev/null -w '%{http_code}' "$RSCHED_URL/readyz" 2>/dev/null | grep -q '^200$'; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# Add a toxic to the `postgres` proxy. $1=toxic JSON.
add_toxic() {
  curl -fsS -X POST -H 'Content-Type: application/json' \
    -d "$1" "$TOXIPROXY_URL/proxies/postgres/toxics" >/dev/null
}

clear_toxics() {
  # Toxiproxy has no bulk-delete; list and delete each toxic by name.
  local names
  names=$(curl -fsS "$TOXIPROXY_URL/proxies/postgres/toxics" | jq -r '.[].name' 2>/dev/null || true)
  for n in $names; do
    curl -fsS -X DELETE "$TOXIPROXY_URL/proxies/postgres/toxics/$n" >/dev/null || true
  done
}

set_proxy_enabled() {
  local enabled="$1"
  curl -fsS -X POST -H 'Content-Type: application/json' \
    -d "{\"enabled\": $enabled}" "$TOXIPROXY_URL/proxies/postgres" >/dev/null
}

# Initial sanity: rusty-sched must be ready before we start.
log "waiting for rusty-sched /readyz baseline..."
if ! wait_ready 60; then
  log "rusty-sched never reached /readyz"
  record "baseline" "FAIL" "rusty-sched not ready"
  printf '\n=== Summary ===\n'; printf '%s\n' "${RESULTS[@]}"
  exit 1
fi
log "baseline OK"

# ---------- scenario 1: 500ms postgres latency -------------------------------
log "scenario 1: inject 500ms latency on postgres"
clear_toxics
add_toxic '{"name":"lat500","type":"latency","attributes":{"latency":500}}'
sleep 10
if wait_ready 30; then
  record "latency-500ms"  "PASS" "ready under 500ms postgres latency"
else
  record "latency-500ms"  "FAIL" "/readyz unhealthy under latency"
fi
clear_toxics

# ---------- scenario 2: 10% packet loss --------------------------------------
log "scenario 2: inject 10% bandwidth corruption"
# Toxiproxy doesn't model packet loss directly; `timeout` with toxicity 0.1
# is the standard proxy for lossy links (closes a fraction of streams).
add_toxic '{"name":"loss10","type":"timeout","toxicity":0.1,"attributes":{"timeout":1000}}'
sleep 10
if wait_ready 30; then
  record "loss-10pct"     "PASS" "ready under 10% stream timeouts"
else
  record "loss-10pct"     "FAIL" "/readyz unhealthy under loss"
fi
clear_toxics

# ---------- scenario 3: full partition for 30s -------------------------------
log "scenario 3: partition postgres for 30s"
set_proxy_enabled false
sleep 30
set_proxy_enabled true
if wait_ready 60; then
  record "partition-30s"  "PASS" "recovered after 30s partition"
else
  record "partition-30s"  "FAIL" "did not recover after partition"
fi

# ---------- scenario 4: postgres restart -------------------------------------
log "scenario 4: restart postgres"
# We can't `docker restart` from inside the chaos-runner container without the
# docker socket mounted (intentionally avoided). Instead simulate a restart by
# disabling the proxy long enough that Postgres' idle connections drop, then
# re-enabling — from rusty-sched's POV this is equivalent.
set_proxy_enabled false
sleep 15
set_proxy_enabled true
if wait_ready 60; then
  record "postgres-restart" "PASS" "recovered after simulated restart"
else
  record "postgres-restart" "FAIL" "did not recover after restart"
fi

# ---------- summary ----------------------------------------------------------
printf '\n=== Summary ===\n'
printf '%s\n' "${RESULTS[@]}"
printf '\n%d pass, %d fail\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
