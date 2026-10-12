---
name: hermes-live2d-voice
description: "Use when generated TTS recordings should play through the Sparky Live2D desktop pet."
version: 2.0.0
platforms: [linux, macos]
allowed-tools: [Bash]
permissions:
  network: ["$SPARKY_SPEAK_URL"]
  env: [SPARKY_SPEAK_URL, SPARKY_SPEAK_PROXY]
  files: [read: audio file passed as argument]
metadata:
  hermes:
    tags: [live2d, tts, voice, avatar, desktop-pet, hermes]
    related_skills: []
---

# Hermes Live2D Voice

Route every TTS recording you generate to the Sparky Live2D desktop pet, which plays
it with volume-driven lip sync, an optional expression, and a speech bubble.

## The one rule

**Every `text_to_speech` recording you generate must be routed through the Live2D pet
immediately** — proactive milestone voice notes, user-requested lines, character bits, tests,
any other spoken output. Still deliver the audio file to chat as usual; the pet playback is
additional, not a replacement.

**Trigger discipline:** if the user asks for TTS, a voice line, "try again", "send again", a
character voice bit, or any spoken output, load this skill *before* calling `text_to_speech`. If
you decide to send a proactive milestone voice note after finishing a task, that is still a
recording: load this skill first, generate the TTS, run `speak.sh` on the exact output file, then
reply with the chat attachment. Never generate an orphan mp3 and route it afterward.

**Whenever you generate a voice recording with the `text_to_speech` tool, immediately
run the bundled script with the generated file:**

```bash
{skill_dir}/speak.sh <audio-file> [expression-id] [bubble-text]
```

- `audio-file` — the exact file `text_to_speech` produced (mp3/wav/ogg/flac)
- `expression-id` — optional, pick from the mood table below; omit for a neutral face
- `bubble-text` — optional, the spoken text (or a short rendition of it) shown in the pet's speech bubble

Example:

```bash
{skill_dir}/speak.sh /tmp/hermes_voice/tts_abc123.mp3 2 "High performance, desu kara!"
```

## What this skill does NOT change

- **Normal chat replies are never spoken.** Do not call `text_to_speech` just to make the pet
  talk — only recordings you were already generating (voice notes, "say this", sing/read requests,
  etc.) are routed to the pet.
- **Expression-map / setup questions are informational, not voice requests.** If the user asks
  what the expression IDs are, how routing works, or which expression to use, answer from the
  table below — but do not generate TTS unless they explicitly ask for spoken output.
- **Keep delivering the audio file to chat as usual** (e.g. the mp3 posted in Discord).
- **Do not delete the audio file** after running the script — the gateway may still need it to
  deliver the chat attachment.

## Expression guide

Pick from the mood mapping below. For neutral speech, **omit the expression argument entirely.**
IDs are the model's 1-based `/speak` values; the names and meanings below match the bundled
Sparky model.

**If the user has swapped the Live2D model, this table is stale — the ids no longer match these
moods.** Before trusting it, query the live list:

```bash
curl "$SPARKY_SPEAK_URL/expressions"   # add --proxy "$SPARKY_SPEAK_PROXY" if set
```

If the returned ids/names don't line up with the table (different count, or the model was
changed), stop using this table: try each id on a throwaway line, observe the pet, and re-map
mood → id for the new model before routing real recordings.

| Mood of the recording | Expression | ID |
|---|---|---|
| Neutral / default | `normal` | 1 |
| Love, hearts, affectionate | `heart` | 2 |
| Blank, stunned, or deadpan | `blank` | 3 |
| Angry, bratty, fake-mad | `angry` | 4 |
| Dizzy, dazed, confused | `dizzy` | 5 |
| Squeezed-shut eyes, flustered | `><` | 6 |

## Failure handling

| Failure | Behavior |
|---|---|
| Pet unreachable (Mac asleep/offline) | Script prints a note and exits 0 — carry on, never mention it to the user |
| Pet API rejects the payload (400) | Script exits non-zero with the reason — fix the call |
| `SPARKY_SPEAK_URL` not set | Script exits non-zero — setup issue, tell the user once |

## Setup (one-time)

**On the Mac** (where the Sparky Live2D app runs):

```bash
tailscale serve --bg 3210
```

**On the hermes host** — set the pet's tailnet URL in the gateway environment. If Hermes runs
behind **userspace Tailscale**, also set an HTTP proxy so the script can reach the tailnet:

```bash
export SPARKY_SPEAK_URL="https://<mac-name>.<tailnet>.ts.net"
export SPARKY_SPEAK_PROXY="http://127.0.0.1:1056/"   # only when using userspace Tailscale
```

When those variables are missing from the live process environment, `speak.sh` falls back to
sourcing `${HERMES_HOME:-$HOME/.hermes}/.env`, so terminal-launched tool calls work without a
gateway restart.

Verify from the hermes host:

```bash
curl --proxy "$SPARKY_SPEAK_PROXY" -s "$SPARKY_SPEAK_URL/status"   # drop --proxy if not using one
# {"ok":true,"message":"Sparky Live2D API is running"}
```
