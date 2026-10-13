---
name: hermes-live2d-voice
description: "Use when TTS recordings should play on the Sparky Live2D pet, or the user wants focus monitoring (Sentry Mode)."
version: 2.1.0
platforms: [linux, macos]
allowed-tools: [Bash]
permissions:
  network: ["$SPARKY_SPEAK_URL"]
  env: [SPARKY_SPEAK_URL, SPARKY_SPEAK_PROXY, SPARKY_SENTRY_CALLBACK_URL, SPARKY_SENTRY_CALLBACK_SECRET]
  files: [read: audio file passed as argument]
metadata:
  hermes:
    tags: [live2d, tts, voice, avatar, desktop-pet, hermes, focus, sentry]
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

## Sentry Mode (focus guard)

Sparky can watch the user's screen activity (frontmost app, window title, browser tab) and get
angry when they slack off. **You turn it on and off from normal conversation.** There is no
command syntax and no pet button: when the user says something like 「幫我開sentry mode，我而家溫AWS」
or "watch me while I study", you infer the target and start it; when they say they are done, you
stop it and post the summary.

```bash
{skill_dir}/sentry.sh start '{"target":"溫書 AWS exam"}'   # optional: allow, block, grace_seconds, interval_seconds
{skill_dir}/sentry.sh status
{skill_dir}/sentry.sh stop                                  # returns the session summary
```

On a violation the pet calls back into Hermes (webhook) with the level (1 tease, 2 harsher,
3 public execution in the Discord channel). You write a Cantonese MK-girl roast, TTS it, and
play it with `speak.sh <file> 4 "<roast>"`.

**Read [`references/sentry-mode.md`](references/sentry-mode.md) before starting a session** for
argument inference, the callback payload, roast escalation rules, the summary format and the
one-time webhook setup.

## Failure handling

| Failure | Behavior |
|---|---|
| Pet unreachable (Mac asleep/offline) | Script prints a note and exits 0 — carry on, never mention it to the user |
| Pet API rejects the payload (400) | Script exits non-zero with the reason — fix the call |
| `SPARKY_SPEAK_URL` not set | Script exits non-zero — setup issue, tell the user once |
| `sentry.sh` cannot reach the pet | Exits 2. Tell the user Sentry Mode could not start (Mac asleep or Sparky not running) |

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
