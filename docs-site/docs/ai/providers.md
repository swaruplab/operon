# AI providers

Operon supports three provider backends — pick one from
**Settings → Auth → Provider**.

![Provider selection](../img/auth-selection.png){ width=500 }

## Anthropic (default)

Direct connection to `api.anthropic.com`. Authenticate with either:

| Method | When |
|---|---|
| **OAuth login** | Recommended for individuals. Click *Log in with Claude*, authorize in browser, paste the code back. Token refreshes automatically. |
| **API key** | Paste an `sk-ant-...` key. Stored securely in the OS keychain. Useful for shared or institutional setups. |

That's it — pick a model and chat. Operon hits Claude direct, no proxy.

## Portkey

[Portkey](https://portkey.ai/) is an Anthropic + OpenAI-compatible AI
gateway. Use this if:

- Your **institution** runs a Portkey deployment (e.g. UCI's ZotGPT)
- You want **cost tracking, prompt logs, and rate limits** centrally
- You want to use **Bedrock-backed Claude** via Portkey routing
- You want to mix **Anthropic and non-Anthropic** models (Moonshot Kimi,
  GPT, Gemini) under one virtual key

### Setup

1. Paste your **Portkey virtual key** into Settings → Auth.
2. Paste the **base URL** (e.g. `https://api.portkey.ai/v1` or your
   institution's gateway).
3. The model catalog auto-loads. Pick one.

### How Operon routes Portkey models

- **Anthropic-family models** (slug contains `claude` or `anthropic`) →
  direct pass-through to Portkey's `/v1/messages`. Claude Code talks
  Anthropic format end-to-end.
- **Non-Anthropic models** (Moonshot Kimi, GPT, Gemini, …) → routed
  through Operon's bundled `anthropic-proxy` sidecar, which translates
  Anthropic-format requests into OpenAI Chat Completions. Transparent to
  Claude Code.

This selection is automatic — Operon checks the slug and picks the path.

!!! warning "Bedrock-backed routes — `requestMetadata` issue"

    Portkey routes that fan out to AWS Bedrock impose a regex constraint
    on the `requestMetadata` field that Claude Code's default `user_id`
    JSON blob violates. Operon routes Bedrock-backed models through the
    proxy automatically (which drops the offending metadata) to avoid
    the 400 error.

## Custom (Ollama, vLLM, LM Studio, …)

For any OpenAI-compatible local backend. Use this for:

- **Privacy / NDA / clinical data** that can't leave your network
- **Offline / air-gapped** environments
- **Lab-hosted models** (70B on an A100 over SSH tunnel)
- **Cost-sensitive** workloads

### Setup

1. Set Provider to **Custom**.
2. Enter the **base URL**, e.g. `http://localhost:11434/v1` for Ollama.
3. Add an **API key** if your backend requires one. Most local servers
   don't.

Operon routes Anthropic-format requests through the bundled
`anthropic-proxy` sidecar, which translates them to
`/v1/chat/completions`. Streaming, tool calls, and system prompts all
survive the round-trip.

See [Private LLM stack](private-llm.md) for the full Ollama / vLLM /
LM Studio walkthrough.

## Comparison

| | Anthropic | Portkey | Custom |
|---|---|---|---|
| **Best for** | Individual users | Institutional / regulated | Private data / local compute |
| **Internet required** | Yes | Yes | No (with local backend) |
| **Cost** | Direct billing | Per your institution / Portkey plan | Free (compute aside) |
| **Anthropic models** | All | All (incl. Bedrock) | No (use Claude-compatible local model) |
| **Non-Anthropic models** | No | Yes (via Portkey routing) | Yes (any OpenAI-compatible) |
| **Data leaves your network** | Yes (to Anthropic) | Yes (to Portkey + upstream) | No (with local backend) |
| **MCP support** | Full | Full | Full |
| **Setup difficulty** | One paste | Two pastes + pick model | Spin up a local server first |

## Switching providers

Operon stores credentials per-provider in the OS keychain, so you can flip
between them without re-pasting. Open Settings → Auth, pick the provider,
the rest of the form fills from saved values.

## Troubleshooting

### "Either x-portkey-config or x-portkey-provider header is required"

Your `ANTHROPIC_BASE_URL` is pointing at Portkey from your shell profile
(`~/.zshrc`, `~/.bash_profile`). Fixed automatically in v0.7.3+ — Operon
now clears stale provider env vars before each spawn. If you're on an
older version, `unset ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN` in your
shell profile or upgrade.

### Custom backend returns 401 / anonymous

A stale `ANTHROPIC_API_KEY` in your shell profile is being preferred over
the bearer token Operon sends. Fixed in v0.7.3+ — Operon clears it for
the Custom path. On older versions, `unset ANTHROPIC_API_KEY` or upgrade.

### Anthropic subscription session says "Invalid API key"

Same cause, opposite direction: on a Max/Pro subscription Operon supplies
no credential at all (the CLI owns the login), so a stale
`ANTHROPIC_API_KEY` in your shell profile takes precedence over your
claude.ai session. Fixed in v1.0.1+ — the Anthropic path now clears it too
whenever Operon isn't supplying a key of its own. See
[Troubleshooting](../troubleshooting.md) for the full message and the
manual workaround on older versions.

### Ollama / vLLM streaming feels slower than Claude

The translation proxy adds ~10ms of overhead per token chunk. For most
local models this is dwarfed by inference latency. If it really bites,
some users disable streaming via the proxy and accept full-response
delivery instead.
