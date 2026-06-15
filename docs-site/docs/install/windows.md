# Install on Windows

Operon ships two installer flavours:

| Installer | When to use |
|---|---|
| `Operon_<version>_x64-setup.exe` (NSIS) | The default for individuals. Per-user install, no admin needed. |
| `Operon_<version>_x64_en-US.msi` | For IT departments doing GPO / Intune rollouts. System-wide install, machine-level. |

Download either from
[the latest release](https://github.com/swaruplab/operon/releases/latest).

## Requirements

- **Windows 10** (build 1809+) or **Windows 11**, x64
- **WebView2 runtime** — pre-installed on Windows 11. On Windows 10, the
  installer auto-downloads the runtime if missing.
- **Git for Windows (Git Bash)** — a **required runtime dependency** for Claude
  Code and SSH features. Operon runs POSIX-style shell commands through Git Bash;
  it is **not bundled**. The setup wizard detects an existing Git Bash install
  (or offers to install Git for Windows) and points Claude Code at it
  automatically. Without it, Claude/SSH features are degraded — `cmd.exe` is only
  a fallback. Install from [git-scm.com](https://git-scm.com/download/win) if the
  wizard doesn't.
- **OpenSSH client (`ssh.exe`)** — required for remote/SSH features. This is the
  system OpenSSH client; it ships with Windows 10 (1809+) and Windows 11. If it's
  missing, enable it via **Settings → Apps → Optional features → OpenSSH Client**.

## Install — NSIS (.exe)

1. Double-click `Operon_<version>_x64-setup.exe`.
2. SmartScreen may pop "Windows protected your PC — Unrecognized app". Click
   **More info → Run anyway**. (We're not yet code-signed with a Microsoft EV
   cert; this clears once Operon is whitelisted.)
3. Follow the installer. The default install location is
   `%LOCALAPPDATA%\Programs\Operon`.

## Install — MSI

```powershell
# Per-machine, silent install
msiexec /i Operon_<version>_x64_en-US.msi /quiet

# Per-user, with logging
msiexec /i Operon_<version>_x64_en-US.msi /quiet ALLUSERS="" /l*v operon-install.log
```

## First launch — setup wizard

The wizard checks for:

1. **WebView2 runtime** — already there on Win 11; auto-installed on Win 10
   if the installer didn't already handle it.
2. **Git for Windows (Git Bash)** — required for Claude Code and SSH. If it
   isn't found, the wizard offers to install Git for Windows (it is not bundled).
3. **Claude Code** — installed via Git Bash:
   ```bash
   curl -fsSL https://claude.ai/install.sh | bash
   ```
   Falls back to `npm install -g @anthropic-ai/claude-code` if Node.js is
   available and curl fails.
4. **Authentication** — log in with your Anthropic account or paste an API
   key. See [Providers](../ai/providers.md).

## Windows runtime contract

Operon runs natively on Windows, but a few host components must be present for
the Claude and remote features to work. These are **runtime dependencies** —
Operon does not bundle them.

| Component | Required for | Notes |
|---|---|---|
| **Git for Windows (Git Bash)** | Claude Code, all POSIX command execution | POSIX command strings run through Git Bash. `cmd.exe` is only a degraded fallback. The setup wizard detects it or offers to install it; not bundled. |
| **OpenSSH client (`ssh.exe`)** | SSH / remote compute features | The system OpenSSH client. Ships with Windows 10 (1809+) and Windows 11; otherwise enable via Settings → Apps → Optional features → OpenSSH Client. |
| **SSH keys** | Authenticating to remote hosts | Standard `~/.ssh` (`%USERPROFILE%\.ssh`) keys. Use `ssh-agent` (the Windows OpenSSH Authentication Agent service) for key caching — ControlMaster multiplexing is not used on Windows. |

### Provider limitations

The bundled Anthropic→OpenAI translation proxy is **not supported on Windows**.
Non-Anthropic local providers that rely on that translation layer are therefore
unavailable here. To use a non-Anthropic model on Windows, point Operon at a
**remote Anthropic-compatible endpoint** (for example a hosted LiteLLM or
OpenRouter endpoint that speaks the Anthropic Messages API). Native Anthropic
(Claude) access works without the proxy. See [Providers](../ai/providers.md).

## Where Operon lives

| Path | What |
|---|---|
| `%LOCALAPPDATA%\Programs\Operon\` | The app binary (NSIS install) |
| `C:\Program Files\Operon\` | The app binary (MSI install) |
| `%USERPROFILE%\.operon\` | Sessions, custom protocols, logs, settings |
| `%APPDATA%\com.operon.app\` | Tauri state, window position |

## Recommended

- **Windows Terminal** — better than the old `cmd.exe`. Optional but Operon's
  integrated terminal feels more at home in it when launched externally.
- **WSL** — not required. Operon runs natively on Windows; Git Bash (Git for
  Windows) provides the POSIX shell Claude Code runs commands through.

## Uninstall

- **NSIS:** Settings → Apps → Operon → Uninstall.
- **MSI:** `msiexec /x Operon_<version>_x64_en-US.msi` or the same Settings → Apps path.
- **User data** (optional): delete `%USERPROFILE%\.operon\` and `%APPDATA%\com.operon.app\`.
