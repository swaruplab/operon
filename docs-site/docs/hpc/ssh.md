# SSH connections

Connect to remote servers and HPC clusters directly from Operon. Browse
files, edit code, and run analyses on remote machines without leaving the
app.

![SSH connection dialog](../img/ssh-connect.png){ width=500 }

## Setting up a connection

1. Click the **SSH** icon in the activity bar.
2. Click **New connection**.
3. Fill in your server details:

    | Field | Example |
    |---|---|
    | **Name** | A label — "Lab HPC", "Sherlock", anything |
    | **Hostname** | `login.hpc.school.edu` |
    | **Port** | `22` by default |
    | **Username** | Your cluster username |
    | **Auth method** | Password, SSH key, or *use ~/.ssh/config* |
    | **Working directory** | `/home/username/projects` (optional) |

4. Click **Save**, then **Connect**.

## Auth methods

### SSH key (recommended)

Pick the path to your private key (default `~/.ssh/id_ed25519`). If your
key is passphrase-protected and you have `ssh-agent` running, Operon picks
up the unlocked key automatically. Otherwise it prompts.

You can set up SSH keys directly from Operon — there's a **Set up SSH keys**
helper that runs `ssh-keygen` and `ssh-copy-id` for you, no terminal
required.

### Password

Operon prompts on every connect. Passwords are stored in the OS keychain
only if you opt in via "Remember password" — never in plain-text config.

### Duo / MFA

For university clusters that require two-factor auth: Operon hands the Duo
prompt through to the terminal. Type your push / passcode / call response
when prompted, same as you would over the command line.

### ProxyJump (bastion hosts)

If your cluster sits behind a bastion / login-jumpbox, configure ProxyJump
in `~/.ssh/config`:

```ssh-config
Host hpc-bastion
    HostName bastion.hpc.school.edu
    User myname
    IdentityFile ~/.ssh/id_ed25519

Host hpc
    HostName compute.hpc.school.edu
    User myname
    ProxyJump hpc-bastion
    IdentityFile ~/.ssh/id_ed25519
```

In Operon, tick **Use ~/.ssh/config** and just enter `hpc` as the
hostname — Operon spawns the system OpenSSH binary, which resolves
ProxyJump natively.

### Agent forwarding

Tick **Forward SSH agent** in the profile to enable `ssh -A`. Lets remote
Claude push to GitHub on your behalf using your local SSH key.

## Connection multiplexing

Operon uses SSH **ControlMaster** under the hood, so multiple file-explorer
operations, terminal commands, and AI sessions share a single TCP connection.
That means:

- File browsing feels instant (no per-action handshake)
- Duo prompts trigger once per session, not per command
- Fewer auth events for your IT team's logs

The ControlMaster socket lives at `~/.operon/ssh-sockets/`.

## Working remotely

Once connected, the file explorer switches to the remote filesystem.
Everything that works locally works remotely:

- Edit files (Monaco talks to the file over SSH)
- Run commands in the integrated terminal (an SSH-backed PTY)
- Diff AI-generated edits before applying
- Use Git on the remote checkout
- Start AI sessions on the remote machine (see [HPC architecture](architecture.md))

## Disconnecting

Click the SSH icon and select the profile → **Disconnect**. Operon tears
down the ControlMaster socket and any active AI sessions. (Sessions
running inside tmux on the remote machine **keep running** — see
[tmux](tmux.md).)
