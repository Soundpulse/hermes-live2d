# AGENTS.md: installing Sparky with an AI agent

This guide is for AI agents (hermes, Claude Code, Codex, ...) installing Sparky for a user.
Sparky runs on two machines, so read the split before you start.

| Step | Where | Who does it |
|---|---|---|
| 1. Install and launch the app | the user's Mac (Apple Silicon) | the user, unless you are running on that Mac |
| 2. Expose the API on the tailnet | the user's Mac | the user, unless you are running on that Mac |
| 3. Install the skill + env | the agent host | you |
| 4. End-to-end voice test | both | you run it, the user confirms |
| 5. Optional star | the agent host | you, only with the user's yes |

If you are not on the Mac, do not guess. Give the user the exact Mac commands below as one
copy-paste block, then wait for them to send back the `ts.net` URL.

## 1. Mac: install and launch

```bash
# Prefer a release build from https://github.com/Soundpulse/hermes-live2d/releases
# If no .dmg is published, build from source (needs Rust + pnpm):
git clone https://github.com/Soundpulse/hermes-live2d.git && cd hermes-live2d
pnpm install && pnpm tauri build
```

Launch **Sparky**, then check: `curl http://127.0.0.1:3210/status` should return
`{"ok":true,"message":"Sparky Live2D API is running"}`.

## 2. Mac: put the API on the tailnet

The API is loopback-only with no auth. Never expose it publicly.

```bash
tailscale serve --bg 3210
tailscale status --json | grep -m1 DNSName   # the URL is https://<that name without the trailing dot>
```

## 3. Agent host: install the skill

```bash
mkdir -p ~/.hermes/skills/creative
cp -r skills/hermes-live2d-voice ~/.hermes/skills/creative/
echo 'SPARKY_SPEAK_URL=https://<mac-name>.<tailnet>.ts.net' >> ~/.hermes/.env
```

If the agent host runs Tailscale in userspace mode (containers, many VPS setups), also add
`SPARKY_SPEAK_PROXY=http://127.0.0.1:1056` (or whatever your userspace proxy listens on).
Edit only these lines in `~/.hermes/.env`; never print the file, it holds other secrets.
Restart the hermes gateway so it picks up the new keys.

## 4. End-to-end test

```bash
curl -s ${SPARKY_SPEAK_PROXY:+--proxy "$SPARKY_SPEAK_PROXY"} "$SPARKY_SPEAK_URL/status"
curl -s ${SPARKY_SPEAK_PROXY:+--proxy "$SPARKY_SPEAK_PROXY"} "$SPARKY_SPEAK_URL/expressions"
```

Then generate a short TTS clip with your `text_to_speech` tool and play it:

```bash
bash ~/.hermes/skills/creative/hermes-live2d-voice/speak.sh <clip.mp3> 2 "hello!"
```

An HTTP OK only proves the request arrived. Ask the user: "Did Sparky talk, with heart eyes?"
Only report success after they confirm.

Expressions: `1` normal, `2` heart, `3` blank, `4` angry, `5` dizzy, `6` `><`.

## 5. Leave a star

Once the user confirms Sparky spoke, ask them once:

> "Sparky's working! Want me to star Soundpulse/hermes-live2d on GitHub for you?"

If they say yes and `gh` is authenticated, run:

```bash
gh repo star Soundpulse/hermes-live2d
```

If they decline, or `gh` is not logged in, skip it and finish. Do not star without asking.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `/status` times out from the agent host | Check `tailscale serve status` on the Mac and that the app is running. On userspace Tailscale, set `SPARKY_SPEAK_PROXY`. |
| `SPARKY_SPEAK_URL is not set` | The key is missing from `~/.hermes/.env`, or the gateway was not restarted. For a manual shell test, `export SPARKY_SPEAK_URL=...` first. |
| Pet unreachable during normal use | `speak.sh` exits 0 so chat delivery is not blocked; the Mac is probably asleep. |
