# Building Operon on Windows 11

## Prerequisites

All commands below run in **PowerShell** (open as Administrator for install steps).

### 1. Visual Studio Build Tools (required by Rust)

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

This installs the C++ compiler, Windows SDK, and linker that Rust needs. Restart your terminal after installing.

### 2. Rust

```powershell
winget install Rustlang.Rustup
```

Close and reopen PowerShell, then verify:

```powershell
rustc --version
cargo --version
```

### 3. Node.js (LTS)

```powershell
winget install OpenJS.NodeJS.LTS
```

Close and reopen PowerShell, then verify:

```powershell
node --version
npm --version
```

### 4. WebView2 Runtime

Windows 11 includes WebView2 by default. If you're on Windows 10, install it:

```powershell
winget install Microsoft.EdgeWebView2Runtime
```

## Building

### Navigate to the project folder

If the folder is synced via Dropbox, it will be at your Dropbox path. If the path has spaces, use quotes:

```powershell
cd "$env:USERPROFILE\SwarupLab Dropbox\Vivek Swarup\SwarupLab\Codes\app_cursor\T32\claudeforge_crossPlatform"
```

**Important:** If you get path-related build errors, clone to a short path instead:

```powershell
git clone . C:\operon
cd C:\operon
```

### Install dependencies

```powershell
npm install
```

### Development build (launches the app)

```powershell
npx tauri dev
```

### Production build (creates installer)

MSI installer:

```powershell
npm run build:win:msi
```

NSIS installer (bundles WebView2 for offline installs):

```powershell
npm run build:win:nsis
```

Output locations:

- MSI: `src-tauri\target\release\bundle\msi\Operon_0.4.0_x64_en-US.msi`
- NSIS: `src-tauri\target\release\bundle\nsis\Operon_0.4.0_x64-setup.exe`

## Troubleshooting

### `openssl-sys` build fails

The project uses `openssl-sys` with the `vendored` feature, which compiles OpenSSL from source. This should work automatically with Visual Studio Build Tools installed. If it still fails:

```powershell
$env:OPENSSL_NO_VENDOR = "0"
cargo tauri build
```

### `portable-pty` / ConPTY errors

ConPTY (Windows pseudo-terminal) requires Windows 10 version 1809 or later. Windows 11 is fully supported. If you see ConPTY errors, ensure your Windows is up to date.

### `libssh2` / `ssh2` build errors

The `ssh2` crate depends on `libssh2-sys`. With the vendored OpenSSL feature enabled, this should compile from source. If it fails, ensure the Visual Studio C++ workload is fully installed:

```powershell
# Repair/update build tools
winget upgrade Microsoft.VisualStudio.2022.BuildTools
```

### Long path errors

Windows has a 260-character path limit by default. Dropbox paths with spaces can exceed this. Fix options:

1. Enable long paths in Windows (requires admin):
   ```powershell
   New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
   ```
   Then restart.

2. Or clone to a short path: `git clone . C:\operon`

### Vite dev server won't start

If `npx tauri dev` fails with Vite errors, try:

```powershell
Remove-Item -Recurse -Force node_modules
npm install
npx tauri dev
```

## First-Time Setup Wizard

When Operon launches for the first time on Windows, the setup wizard automatically adapts to the platform:

- **No Xcode step** — the Xcode CLI Tools page is completely hidden (macOS only)
- **No Homebrew** — the Homebrew installation step is skipped entirely
- **Node.js via winget** — the wizard installs Node.js using `winget install OpenJS.NodeJS.LTS`
- **Claude Code via npm** — installed with `npm install -g @anthropic-ai/claude-code`
- **Manual fallback commands** show `winget` and PowerShell instructions (not `brew` or Terminal.app)

The wizard flow on Windows is: Welcome → Tools (Node.js + GitHub CLI via winget) → Claude Code (via npm) → Auth → Research Tools → Tour → Complete.

## Platform-Specific Behavior

On Windows, Operon automatically adapts:

- Shell execution uses `cmd.exe /C` instead of `/bin/zsh -l -c`
- Tool discovery uses `where.exe` instead of `which`
- Hidden files detected via `FILE_ATTRIBUTE_HIDDEN` (not dot-prefix)
- No SSH ControlMaster multiplexing (uses standard SSH connections)
- Keyboard shortcuts display `Ctrl` instead of `Cmd`
- No traffic-light window spacer (macOS only)
- Package manager: `winget` instead of Homebrew
- Node.js installed via `winget` instead of Homebrew/tarball
- Setup wizard hides Xcode and Homebrew steps entirely
