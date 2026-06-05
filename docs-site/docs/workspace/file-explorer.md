# File explorer

The leftmost icon in the activity bar opens the file explorer.

![File explorer](../img/file-explorer.png){ width=400 }

## What it does

- Tree view with lazy directory loading — works even on directories with
  100k+ files
- Create, rename, delete files and folders via right-click
- **Search** across the project (++cmd+shift+f++ / ++ctrl+shift+f++)
- **Go-to-folder path bar** (++cmd+g++ / ++ctrl+g++) — type or paste a path
  to jump
- **Symlink-aware** — resolves through symlinks for both local files and
  remote files over SSH

## Opening a project

`File → Open Folder` (or ++cmd+o++ / ++ctrl+o++) sets the working directory.
Claude operates within this folder — its file-read / file-write tools are
sandboxed to it (with explicit approval prompts for anything outside).

## Right-click actions

| Action | Effect |
|---|---|
| New file | Creates a file in the current folder; opens it in the editor |
| New folder | Creates a folder |
| Rename | Inline rename — same as F2 in most file managers |
| Delete | Sends to system Trash (macOS), Recycle Bin (Windows), or `~/.local/share/Trash` (Linux) |
| Reveal in Finder / Explorer / file manager | Opens the OS file manager at this path |
| Copy path | Absolute path → clipboard |
| Copy relative path | Path relative to the project root → clipboard |

## Remote file explorer

When you connect to an SSH host, the file explorer switches to the remote
filesystem. All the same actions apply — they execute over SSH against the
remote shell. See [SSH connections](../hpc/ssh.md) for setup.

## Search

++cmd+shift+f++ / ++ctrl+shift+f++ opens project-wide search. Matches are
grouped by file and clickable. Results are streamed as the index runs, so
large projects feel responsive even before indexing finishes.

## Performance notes

- Trees collapse by default. Directory contents are loaded on first expansion
  and cached.
- The explorer ignores `node_modules`, `.git`, `.venv`, `__pycache__`, and
  similar by default — configurable in [Settings](settings.md).
- On HPC clusters, large `scratch/` directories with millions of files may
  still take a few seconds to enumerate on first open.
