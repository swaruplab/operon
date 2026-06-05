# Install on Linux

Operon ships three Linux formats. Pick the one your distro prefers.

| Format | Best for |
|---|---|
| `.deb` | Debian 12+, Ubuntu 22.04+, Pop!_OS, Mint |
| `.rpm` | Fedora 39+, RHEL 9, Rocky Linux, AlmaLinux |
| `.AppImage` | Any glibc 2.31+ distro — Arch, openSUSE, NixOS, anything not on the list above |

Download from the
[latest release](https://github.com/swaruplab/operon/releases/latest).

## Requirements

- **x86_64** CPU (no aarch64 builds yet for Linux)
- **glibc 2.31+** — `ldd --version` to check
- **webkit2gtk-4.1** — Tauri 2 uses this for the embedded webview
- **OpenSSH** — for SSH/HPC mode

## Install — Debian / Ubuntu (.deb)

```bash
sudo apt install ./Operon_<version>_amd64.deb
# or, if apt is older:
sudo dpkg -i Operon_<version>_amd64.deb && sudo apt install -f
```

Tested on Ubuntu 22.04, 24.04, Debian 12. Earlier Ubuntu versions ship
`webkit2gtk-4.0` (incompatible) — upgrade or use the AppImage.

## Install — Fedora / RHEL (.rpm)

```bash
sudo dnf install ./Operon-<version>-1.x86_64.rpm
# or
sudo rpm -i Operon-<version>-1.x86_64.rpm
```

Tested on Fedora 39+, RHEL 9, Rocky Linux 9, AlmaLinux 9.

## Install — AppImage

```bash
chmod +x Operon_<version>_amd64.AppImage
./Operon_<version>_amd64.AppImage
```

No install step required. To integrate the AppImage with your application
menu, use [AppImageLauncher](https://github.com/TheAssassin/AppImageLauncher).

## Setup wizard

On first launch:

1. **System libs** check — if `webkit2gtk-4.1` is missing the wizard tells
   you the exact `apt`/`dnf` command for your distro.
2. **Claude Code** install via the official script:
   ```bash
   curl -fsSL https://claude.ai/install.sh | bash
   ```
   Falls back to npm (`npm install -g @anthropic-ai/claude-code`) if curl
   or the script fails.
3. **Authentication** — log in with your Anthropic account or paste an API
   key. See [Providers](../ai/providers.md).

## Where Operon lives

| Path | What |
|---|---|
| `/usr/bin/operon` (apt/dnf) or the AppImage path | The binary |
| `~/.operon/` | Sessions, custom protocols, logs, settings |
| `~/.config/com.operon.app/` | Tauri state, window position |
| `~/.local/share/com.operon.app/` | Bundled assets |

## Common errors

- **`error while loading shared libraries: libwebkit2gtk-4.1.so`** —
  install `webkit2gtk-4.1`:
  ```bash
  # Debian/Ubuntu
  sudo apt install libwebkit2gtk-4.1-0
  # Fedora
  sudo dnf install webkit2gtk4.1
  ```
- **Wayland glitches** — set `WEBKIT_DISABLE_DMABUF_RENDERER=1` in your env
  if you see rendering artefacts on NVIDIA + Wayland.
- **AppImage refuses to launch on Wayland** — `--no-sandbox` is a common
  fallback, though we don't ship sandbox by default.

## Uninstall

```bash
# apt
sudo apt remove operon

# dnf
sudo dnf remove operon

# AppImage — just delete it

# user data (optional)
rm -rf ~/.operon ~/.config/com.operon.app ~/.local/share/com.operon.app
```
