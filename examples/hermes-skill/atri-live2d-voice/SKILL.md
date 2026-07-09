---
name: atri-live2d-voice
description: "Play generated TTS recordings on the ATRI Live2D desktop pet (lip sync + expression + speech bubble)."
version: 1.0.0
platforms: [linux, macos]
metadata:
  hermes:
    tags: [live2d, tts, voice, avatar, desktop-pet, atri]
    related_skills: []
---

# ATRI Live2D Voice

Route every TTS recording you generate to the ATRI Live2D desktop pet, which plays
it with volume-driven lip sync, an optional expression, and a speech bubble.

## The one rule

**Whenever you generate a voice recording with the `text_to_speech` tool, immediately
run the bundled script with the generated file:**

```bash
{skill_dir}/speak.sh <audio-file> [expression-id] [bubble-text]
```

- `audio-file` — the exact file `text_to_speech` produced (mp3/wav/ogg/flac)
- `expression-id` — optional, pick from the table below based on the mood of what is spoken
- `bubble-text` — optional, the spoken text (or a short rendition of it) shown in the pet's speech bubble

Example:

```bash
{skill_dir}/speak.sh /tmp/hermes_voice/tts_abc123.mp3 13 "High performance, desu kara!"
```

## What this skill does NOT change

- **Normal chat replies are never spoken.** Do not call `text_to_speech` just to make
  the pet talk — only recordings you were already generating (voice notes, "say this",
  sing/read requests, etc.) are routed to the pet.
- **Keep delivering the audio file to chat as usual** (e.g. the mp3 posted in Discord).
  The pet playback is additional, not a replacement.
- **Do not delete the audio file** after running the script — the gateway may still
  need it to deliver the chat attachment.

## Expression guide

| Mood of the recording | Expression | ID |
|---|---|---|
| Proud, confident, task done | YES | 13 |
| Shy, being praised | Shy | 1 |
| Surprised, stunned | Stunned | 7 |
| Serious analysis | Shadow | 15 |
| Refusal, annoyed | NO | 12 |
| Sad, regretful | Lost highlight | 2 |
| Happy, casual chat | Bird | 10 |
| Crab/food mentioned | Crab | 11 |
| Neutral / default | _(omit expression)_ | — |

Expressions 3, 4, 5, 6, 9, 14 change the outfit — only use them if the user explicitly asks.

## Failure handling

| Failure | Behavior |
|---|---|
| Pet unreachable (Mac asleep/offline) | Script prints a note and exits 0 — carry on, never mention it to the user |
| Pet API rejects the payload (400) | Script exits non-zero with the reason — fix the call |
| `ATRI_SPEAK_URL` not set | Script exits non-zero — setup issue, tell the user once |

## Setup (one-time)

**On the Mac** (where the ATRI Live2D app runs):

```bash
tailscale serve --bg 3210
```

**On the hermes host** — set the pet's tailnet URL in the gateway environment:

```bash
export ATRI_SPEAK_URL="https://<mac-name>.<tailnet>.ts.net"
```

Verify from the hermes host:

```bash
curl -s "$ATRI_SPEAK_URL/status"
# {"ok":true,"message":"ATRI Live2D API is running"}
```
