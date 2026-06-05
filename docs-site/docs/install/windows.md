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
- **Git Bash** — bundled inside Operon for HPC-style shell command compatibility.
  You don't have to install anything; Operon points Claude Code at the bundled
  bash automatically.

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
2. **Claude Code** — installed via the bundled Git Bash:
   ```bash
   curl -fsSL https://claude.ai/install.sh | bash
   ```
   Falls back to `npm install -g @anthropic-ai/claude-code` if Node.js is
   available and curl fails.
3. **Authentication** — log in with your Anthropic account or paste an API
   key. See [Providers](../ai/providers.md).

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
- **WSL** — not required. Operon runs natively on Windows; the bundled Git
  Bash already gives you a POSIX shell for Claude Code.

## Uninstall

- **NSIS:** Settings → Apps → Operon → Uninstall.
- **MSI:** `msiexec /x Operon_<version>_x64_en-US.msi` or the same Settings → Apps path.
- **User data** (optional): delete `%USERPROFILE%\.operon\` and `%APPDATA%\com.operon.app\`.
