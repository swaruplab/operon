# Building Operon on Linux

## Prerequisites

Tested on **Ubuntu 22.04 LTS** and **Fedora 38+**. Commands below are for Ubuntu/Debian; Fedora equivalents are noted where different.

### 1. System Libraries (required by Tauri)

**Ubuntu/Debian:**

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  pkg-config \
  curl \
  wget \
  file \
  patchelf
```

**Fedora:**

```bash
sudo dnf groupinstall -y "Development Tools"
sudo dnf install -y \
  webkit2gtk4.1-devel \
  gtk3-devel \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  openssl-devel \
  pkg-config \
  curl \
  wget \
  file \
  patchelf
```

### 2. Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the prompts (default installation is fine). Then reload your shell:

```bash
source "$HOME/.cargo/env"
```

Verify:

```bash
rustc --version
cargo --version
```

### 3. Node.js (LTS)

Using NodeSource:

```bash
curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
sudo apt install -y nodejs
```

Or using `nvm` (recommended for development):

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
source ~/.bashrc
nvm install --lts
```

Verify:

```bash
node --version
npm --version
```

## Building

### Navigate to the project folder

```bash
cd /path/to/claudeforge_crossPlatform
```

### Install dependencies

```bash
npm install
```

### Development build (launches the app)

```bash
npx tauri dev
```

### Production build (creates installers)

DEB package:

```bash
npm run build:linux:deb
```

AppImage (portable, no installation needed):

```bash
npm run build:linux:appimage
```

Both:

```bash
npm run build:linux
```

Output locations:

- DEB: `src-tauri/target/release/bundle/deb/operon_0.4.0_amd64.deb`
- AppImage: `src-tauri/target/release/bundle/appimage/operon_0.4.0_amd64.AppImage`

## Installing the DEB package

```bash
sudo dpkg -i src-tauri/target/release/bundle/deb/operon_0.4.0_amd64.deb
```

## Running the AppImage

```bash
chmod +x src-tauri/target/release/bundle/appimage/operon_0.4.0_amd64.AppImage
./operon_0.4.0_amd64.AppImage
```

## Troubleshooting

### Missing `webkit2gtk-4.1`

Tauri 2 requires webkit2gtk **4.1** (not 4.0). On older Ubuntu versions (20.04), this package may not be available. Upgrade to Ubuntu 22.04 or later.

If you see:

```
Package libwebkit2gtk-4.1-dev is not available
```

Check your Ubuntu version with `lsb_release -a` and upgrade if needed.

### `openssl-sys` build fails

The project uses vendored OpenSSL. If it still fails, install the development headers:

```bash
# Ubuntu/Debian
sudo apt install -y libssl-dev pkg-config

# Fedora
sudo dnf install -y openssl-devel pkg-config
```

### `libayatana-appindicator` not found

On some distributions the package name differs:

```bash
# Try the older package name
sudo apt install -y libappindicator3-dev
```

### `patchelf` not found during AppImage build

```bash
sudo apt install -y patchelf
```

### Vite dev server won't start

If `npx tauri dev` fails with Vite errors:

```bash
rm -rf node_modules
npm install
npx tauri dev
```

### Blank window or WebKit crash

Ensure your graphics drivers are up to date. On VMs or headless systems, you may need:

```bash
export WEBKIT_DISABLE_COMPOSITING_MODE=1
npx tauri dev
```

### Wayland issues

If running Wayland and experiencing rendering issues, try forcing X11:

```bash
GDK_BACKEND=x11 npx tauri dev
```

## First-Time Setup Wizard

When Operon launches for the first time on Linux, the setup wizard automatically adapts to the platform:

- **No Xcode step** — the Xcode CLI Tools page is completely hidden (macOS only)
- **No Homebrew** — the Homebrew installation step is skipped entirely
- **Node.js via apt** — the wizard installs Node.js using `sudo apt install -y nodejs`
- **Claude Code via npm** — installed with `npm install -g @anthropic-ai/claude-code` or `curl -fsSL https://claude.ai/install.sh | bash`
- **Manual fallback commands** show `apt` and terminal instructions (not `brew` or Terminal.app)

The wizard flow on Linux is: Welcome → Tools (Node.js + GitHub CLI via apt) → Claude Code (via npm/curl) → Auth → Research Tools → Tour → Complete.

## Platform-Specific Behavior

On Linux, Operon automatically adapts:

- Shell execution uses `/bin/bash -l -c` instead of `/bin/zsh -l -c`
- Tool discovery uses `which` (same as macOS)
- Hidden files detected via dot-prefix (same as macOS)
- SSH ControlMaster multiplexing is supported (same as macOS)
- Keyboard shortcuts display `Ctrl` instead of `Cmd`
- No traffic-light window spacer (macOS only)
- Package manager: `apt` (or system package manager) instead of Homebrew
- URLs opened via `xdg-open`
- Terminal opened via detected terminal emulator (gnome-terminal, konsole, xfce4-terminal, or xterm)
- Setup wizard hides Xcode and Homebrew steps entirely
