# Private LLM stack

> Your data. Your model. Your machine.

Clinical cohorts. Embargoed sequencing data. Industry collaborations under
NDA. Some of your work simply cannot leave your network. Operon was built
for that reality.

## Three paths to a private model

### Easiest — Ollama

A single command installs a daemon that serves local models on an
OpenAI-compatible endpoint. Perfect for laptops and lab workstations with
a GPU.

```bash
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llama3.1:8b
ollama serve
```

| Runs on | Hardware |
|---|---|
| macOS · Windows · Linux | CPU OK · GPU preferred (NVIDIA, Apple Silicon, AMD ROCm) |

Then in Operon, **Settings → Auth → Provider = Custom**, base URL
`http://localhost:11434/v1`.

### Most performant — vLLM

Production-grade serving with paged attention, speculative decoding, and
tensor parallelism. Drop it on your lab's A100 / H100 node and share
across the team.

```bash
pip install vllm
vllm serve Qwen/Qwen2.5-Coder-32B-Instruct \
  --host 0.0.0.0 --port 8000 \
  --tensor-parallel-size 2
```

| Runs on | Hardware |
|---|---|
| Linux | NVIDIA GPUs · A100 / H100 / 4090+ |

### Most UI-friendly — LM Studio

Desktop GUI for browsing, pulling, and running GGUF models. The Server tab
exposes the same OpenAI-compatible endpoint.

1. Install [LM Studio](https://lmstudio.ai/).
2. Pull a model in the Discover tab (e.g. Llama 3.1 8B Q4_K_M).
3. Server tab → toggle **Start Server**. Default port `1234`.

| Runs on | Hardware |
|---|---|
| macOS · Windows · Linux | Apple Silicon · NVIDIA · AMD |

In Operon: base URL `http://localhost:1234/v1`.

## Recipe: 70B on your cluster, querying from your laptop

One of the most-requested patterns.

### 1. Start vLLM on the GPU node

```bash
ssh gpu-node
vllm serve meta-llama/Llama-3.3-70B-Instruct \
  --host 127.0.0.1 --port 8000 \
  --tensor-parallel-size 4
```

### 2. Tunnel from your laptop

```bash
ssh -N -L 8000:localhost:8000 \
  -J login.hpc.edu gpu-node.hpc.edu
```

The `-J` (ProxyJump) flag handles the login-node bounce automatically.

### 3. Point Operon at localhost

**Settings → Auth → Provider = Custom**, base URL
`http://localhost:8000/v1`. Pick the Llama-3.3-70B model.

Every chat now runs on your cluster's GPU, with traffic tunneled over SSH.

## OpenAI-compatible bridge

Any backend that speaks `/v1/chat/completions` works:

- **LiteLLM** — unified gateway to 100+ providers
- **OpenRouter** — pay-as-you-go with no lock-in
- **Together** · **Groq** · **DeepInfra** · **Cerebras** · **Anyscale**
- Self-hosted **Ollama** / **vLLM** / **LM Studio** / **llama.cpp**

## Built for private data

| | What |
|---|---|
| :material-key-variant: **OS keychain** | Credentials in macOS Keychain, Windows Credential Manager, libsecret — never plain-text config |
| :material-cloud-off-outline: **Zero telemetry** | Operon collects nothing. Source on GitHub — verify it yourself |
| :material-vpn: **Reverse-tunnel ready** | Host a 70B on an A100, tunnel over SSH, query like localhost |
| :material-toggle-switch: **Per-session backend switching** | Local Ollama, on-prem vLLM, cloud Claude — picked per session, not per install |
| :material-message-arrow-right: **Streaming & tool calls** | The translation proxy preserves both end-to-end |
| :material-file-document: **Logs stay local** | Every prompt, tool call, response in `~/.operon/logs/` |
| :material-lan-disconnect: **Air-gapped** | No license server, no update pings, no analytics — unplug the cable and it still works |

## When *not* to use a local model

Frontier reasoning still matters for hard biology. A few honest data points:

- **Plan mode quality** with a local 8B model is noticeably worse than
  Claude Opus. Useful for routine work, not for novel analysis design.
- **Tool-calling reliability** drops below ~30B for many models. Agent mode
  benefits from a stronger model.
- **Multi-step debugging** is where Claude pulls clearly ahead. If you can
  use Claude direct or via Portkey for these sessions and a local model
  for routine Ask sessions, you get the best of both.

You can switch per session — start one tab on Claude direct for the hard
work, keep another on a local model for sensitive data.
