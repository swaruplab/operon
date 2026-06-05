# Build from source

For contributors, or anyone wanting to build a debug version.

## Prerequisites

All platforms need:

| Tool | Why |
|---|---|
| [Rust](https://rustup.rs/) (latest stable) | Tauri / backend |
| [Node.js](https://nodejs.org/) 18+ + npm | Frontend (React + Vite) |
| [Git](https://git-scm.com/) | Cloning the repo |

Then per OS:

=== "macOS"

    ```bash
    xcode-select --install
    ```

=== "Windows"

    1. Install [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the
       **Desktop development with C++** workload.
    2. Install the [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (pre-installed on Win 11).

=== "Linux — Debian/Ubuntu"

    ```bash
    sudo apt install libwebkit2gtk-4.1-dev build-essential \
        curl wget file libxdo-dev libssl-dev \
        libayatana-appindicator3-dev librsvg2-dev
    ```

=== "Linux — Fedora"

    ```bash
    sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
        libappindicator-gtk3-devel librsvg2-devel \
        @"C Development Tools and Libraries"
    ```

## Clone & install

```bash
git clone https://github.com/swaruplab/operon.git
cd operon
git checkout cross-platform        # default branch — active development
npm install
```

## Run in dev mode

```bash
npm run tauri dev
```

This launches Vite for the frontend (with HMR) and `cargo run` for the
backend. Edits to React/TS reload instantly; edits to Rust trigger a
backend rebuild.

## Build production installers

```bash
# Native build for your current OS
npm run tauri build

# Or target-specific:
npm run build:mac:arm
npm run build:mac:intel
npm run build:mac:universal
npm run build:win              # MSI + NSIS
npm run build:win:msi
npm run build:win:nsis
npm run build:linux            # deb + rpm + AppImage
npm run build:linux:deb
npm run build:linux:appimage
```

Output lands in `src-tauri/target/release/bundle/`.

## Signed builds (macOS)

The repo includes signing-script templates that you copy and fill in with your
Apple Developer credentials:

```bash
cp build-signed.example.sh build-signed.sh
$EDITOR build-signed.sh     # add APPLE_ID, TEAM_ID, app-specific password
bash build-signed.sh
```

Templates:

- `build-signed.example.sh` → Apple Silicon DMG
- `build-intel.example.sh` → Intel DMG
- `build-universal.example.sh` → Universal binary

## CI / multi-platform release

`.github/workflows/release.yml` builds installers for all four targets
(macOS arm64, macOS x64, Windows x64, Linux x64) on every `v*` tag push and
uploads them as GitHub Release assets.

## Project layout

See the [architecture](../architecture.md) page for the full tree, but
high-level:

```
operon/
├── src/             # React + TypeScript frontend
├── src-tauri/       # Rust backend (Tauri 2)
│   ├── src/         # IPC commands, state managers
│   └── binaries/    # Bundled sidecars (anthropic-proxy translation layer)
├── protocols/       # 665 bundled bioinformatics protocols
├── presets/         # Hot-fetchable Portkey gateway presets
├── docs/            # The legacy HTML landing pages (img/ shared with the MkDocs site)
└── docs-site/       # This documentation site (MkDocs Material)
```

## Contribute

See [Contributing](../contributing.md).
