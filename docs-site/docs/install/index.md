# Install Operon

Operon ships as a signed installer for every major platform. Pick yours:

[:fontawesome-brands-apple: macOS](macos.md){ .md-button .md-button--primary }
[:fontawesome-brands-windows: Windows](windows.md){ .md-button }
[:fontawesome-brands-linux: Linux](linux.md){ .md-button }
[:material-cog: Build from source](build-from-source.md){ .md-button }

## System requirements

| Platform | Minimum |
|---|---|
| **macOS** | macOS 12 (Monterey) or later · Apple Silicon (M1+) or Intel |
| **Windows** | Windows 10 (1809+) or Windows 11 · x64 · WebView2 (pre-installed on Win 11; auto-installed on Win 10) |
| **Linux** | x64 · glibc 2.31+ · `webkit2gtk-4.1` (Ubuntu 22.04+ / Fedora 36+ / Debian 12+) |
| **Disk** | ~500 MB including dependencies |
| **RAM** | 4 GB minimum, 8 GB recommended |
| **Internet** | Required for AI features and initial setup |

## What gets installed

Operon itself is a small (~40 MB) native bundle. On first launch, the
**setup wizard** detects and installs any missing dependencies:

| Dependency | Why | How |
|---|---|---|
| Native toolchain | C/C++ build tools for any local Python wheels and Claude Code's npm prebuilds | Xcode CLT (macOS) · auto-installed; WebView2 (Windows) · system check; webkit2gtk (Linux) · package manager hint |
| **Claude Code** | The AI engine Operon talks to | `curl -fsSL https://claude.ai/install.sh \| bash` (or npm fallback) |
| Optional: GitHub CLI, conda, common bioinformatics CLIs | Convenience | Per-OS package manager |

You can decline any optional step — Operon will still launch, but some features
(e.g. AI chat) require Claude Code to be present.

## Updates

Operon checks GitHub Releases at launch. When a newer version is available,
a banner appears with a one-click upgrade. You can also grab the latest
installer manually from the
[Releases page](https://github.com/swaruplab/operon/releases/latest) and
overwrite the existing install:

- **macOS:** drag the new `.app` into Applications — overwrite when prompted.
- **Windows:** run the new `.exe` / `.msi`.
- **Linux:** re-run `apt`/`dnf`, or replace the AppImage.

Your settings, sessions, and custom protocols live in `~/.operon/` and are
preserved across upgrades.
