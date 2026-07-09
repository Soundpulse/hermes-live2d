#!/usr/bin/env bash
# Play a TTS recording on the ATRI Live2D desktop pet.
#
# Usage: speak.sh <audio-file> [expression-id] [bubble-text]
# Requires: ATRI_SPEAK_URL, e.g. https://my-mac.my-tailnet.ts.net
#
# Exit codes: 0 on success or pet-unreachable (graceful degrade);
#             1 on caller/setup errors (bad args, missing env, rejected payload).
set -uo pipefail

AUDIO_FILE="${1:?usage: speak.sh <audio-file> [expression-id] [bubble-text]}"
EXPRESSION="${2:-}"
TEXT="${3:-}"

: "${ATRI_SPEAK_URL:?ATRI_SPEAK_URL is not set (e.g. https://my-mac.my-tailnet.ts.net)}"

[ -f "$AUDIO_FILE" ] || { echo "speak.sh: file not found: $AUDIO_FILE" >&2; exit 1; }

exec python3 - "$AUDIO_FILE" "$EXPRESSION" "$TEXT" <<'PY'
import base64, json, os, sys, urllib.error, urllib.request

audio_file, expression, text = sys.argv[1], sys.argv[2], sys.argv[3]
url = os.environ["ATRI_SPEAK_URL"].rstrip("/") + "/speak"

ext = os.path.splitext(audio_file)[1].lstrip(".").lower()
if ext not in ("mp3", "wav", "ogg", "flac"):
    sys.exit(f"speak.sh: unsupported audio format: .{ext}")

with open(audio_file, "rb") as f:
    payload = {
        "audio_data": base64.b64encode(f.read()).decode(),
        "audio_format": ext,
    }
if text:
    payload["text"] = text
if expression:
    if not expression.isdigit():
        sys.exit(f"speak.sh: expression must be a numeric id, got: {expression}")
    payload["expression"] = int(expression)

req = urllib.request.Request(
    url,
    data=json.dumps(payload).encode(),
    headers={"Content-Type": "application/json"},
)
try:
    with urllib.request.urlopen(req, timeout=15) as resp:
        print(f"ATRI: playing {os.path.basename(audio_file)}")
except urllib.error.HTTPError as e:
    sys.exit(f"speak.sh: pet API rejected the request: {e.read().decode(errors='replace')[:200]}")
except (urllib.error.URLError, OSError):
    # Pet offline or Mac asleep — degrade silently, chat delivery already happened.
    print("ATRI: pet unreachable, skipping playback", file=sys.stderr)
PY
