#!/usr/bin/env bash
# Sample Cronicle-compatible rusty-sched plugin.
# Reads a JSON envelope on stdin: {"id":"<run_id>","params":{...}}
# Emits progress + final complete event on stdout.

set -euo pipefail

# Read the envelope (single line) and acknowledge.
read -r envelope || envelope='{}'

printf '{"progress":0.0,"description":"starting","raw":%s}\n' "$envelope"
sleep 0.05
printf '{"progress":0.5,"description":"halfway","perf":{"counter":42}}\n'
sleep 0.05
printf '{"complete":1,"code":0,"description":"echo plugin done"}\n'
