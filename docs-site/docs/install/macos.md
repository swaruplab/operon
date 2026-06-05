# Install on macOS

## Download

Grab the right DMG for your Mac from the
[latest release](https://github.com/swaruplab/operon/releases/latest):

| Mac | File |
|---|---|
| Apple Silicon (M1 / M2 / M3 / M4) | `Operon_<version>_aarch64.dmg` |
| Intel | `Operon_<version>_x64.dmg` |

Not sure which? **Apple menu :material-apple: → About This Mac**. If it says
"Apple M1" or newer, you want Apple Silicon. Intel CPUs say "Intel Core i…".

## Install

1. Double-click the `.dmg` to mount it.
2. Drag **Operon** into the **Applications** folder.

    ![Drag to Applications](../img/install-dmg.png){ width=500 }

3. Eject the DMG.

## First launch — bypass Gatekeeper

The first time you open Operon, macOS warns that the app is from an
unidentified developer (we're not in the Apple Developer Program). Get past
this once:

1. Right-click **Operon** in Applications → **Open**.
2. Click **Open** in the dialog.

    ![Gatekeeper dialog](../img/install-security.png){ width=400 }

Subsequent launches are normal — no more warnings.

!!! tip "If the right-click trick doesn't appear"

    macOS 13+ sometimes hides the option. Open **System Settings → Privacy &
    Security**, scroll to the "Operon was blocked" line, click **Open Anyway**.

## Setup wizard

After the first launch, the setup wizard walks you through three steps:

1. **Xcode Command Line Tools** — the C/C++ toolchain needed by many Python
   wheels and the Claude Code installer. Operon triggers the system installer;
   accept the license. If automated install fails, run `xcode-select --install`
   in Terminal.
2. **Claude Code** — Anthropic's CLI agent. Operon runs the official installer
   (`curl -fsSL https://claude.ai/install.sh | bash`) and falls back to
   `npm install -g @anthropic-ai/claude-code` if curl is unavailable.
3. **Authentication** — log in with your Anthropic account or paste an API
   key. See [Providers](../ai/providers.md) for the full menu.

You can re-run the wizard anytime from the gear icon → **Setup wizard**.

## Optional: install Operon via Homebrew

A Homebrew cask is in the works. For now, the DMG flow is the supported path.

## Where Operon lives

| Path | What |
|---|---|
| `/Applications/Operon.app` | The app itself |
| `~/.operon/` | Sessions, custom protocols, logs, settings |
| `~/Library/Application Support/com.operon.app/` | Tauri state (window position, secure store) |

## Uninstall

```bash
# Move the app to Trash
mv /Applications/Operon.app ~/.Trash/

# Remove user data (optional — keeps sessions/protocols/settings)
rm -rf ~/.operon ~/Library/Application\ Support/com.operon.app
```
