#!/usr/bin/env bash
# Control Sentry Mode on the Sparky Live2D desktop pet.
#
# Usage:
#   sentry.sh start '<json>'   # e.g. '{"target":"溫書 AWS exam","block":["discord"]}'
#   sentry.sh stop             # prints the session summary JSON
#   sentry.sh status
#
# Requires: SPARKY_SPEAK_URL (same pet URL that speak.sh uses).
# Optional: SPARKY_SPEAK_PROXY, SPARKY_SENTRY_CALLBACK_URL, SPARKY_SENTRY_CALLBACK_SECRET.
# The callback URL and secret are injected from the environment on `start`
# (unless the JSON already sets callback_url), so the secret never has to
# appear on the command line.
#
# Exit codes: 0 success; 1 caller/setup error or rejected request; 2 pet unreachable.
set -uo pipefail

ACTION="${1:?usage: sentry.sh start '<json>' | stop | status}"
BODY="${2:-}"

: "${SPARKY_SPEAK_URL:?SPARKY_SPEAK_URL is not set (e.g. https://my-mac.my-tailnet.ts.net)}"

exec python3 - "$ACTION" "$BODY" <<'PY'
import json, os, sys, urllib.error, urllib.request

action, body = sys.argv[1], sys.argv[2]
base = os.environ["SPARKY_SPEAK_URL"].rstrip("/")

if action == "start":
    try:
        payload = json.loads(body) if body else {}
    except json.JSONDecodeError as e:
        sys.exit(f"sentry.sh: start body is not valid JSON: {e}")
    if not isinstance(payload, dict) or not str(payload.get("target", "")).strip():
        sys.exit("sentry.sh: start needs a JSON object with a non-empty \"target\"")
    cb_url = os.environ.get("SPARKY_SENTRY_CALLBACK_URL")
    cb_secret = os.environ.get("SPARKY_SENTRY_CALLBACK_SECRET")
    if cb_url and "callback_url" not in payload:
        payload["callback_url"] = cb_url
        if cb_secret:
            payload["callback_secret"] = cb_secret
    req = urllib.request.Request(
        base + "/sentry/start",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
elif action == "stop":
    req = urllib.request.Request(base + "/sentry/stop", data=b"", method="POST")
elif action == "status":
    req = urllib.request.Request(base + "/sentry/status")
else:
    sys.exit(f"sentry.sh: unknown action: {action} (use start, stop or status)")

proxy = os.environ.get("SPARKY_SPEAK_PROXY")
if proxy:
    open_url = urllib.request.build_opener(
        urllib.request.ProxyHandler({"http": proxy, "https": proxy})
    ).open
else:
    open_url = urllib.request.urlopen

try:
    with open_url(req, timeout=15) as resp:
        data = json.loads(resp.read().decode() or "{}")
except urllib.error.HTTPError as e:
    sys.exit(f"sentry.sh: pet API rejected the request: {e.read().decode(errors='replace')[:300]}")
except (urllib.error.URLError, OSError) as e:
    print(f"sentry.sh: pet unreachable ({e}). Is the Mac awake and Sparky running?", file=sys.stderr)
    sys.exit(2)

print(json.dumps(data, ensure_ascii=False, indent=2))
PY
