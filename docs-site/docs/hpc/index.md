# HPC mode

> Run on your cluster, not your laptop.

Operon's terminal mode runs Claude inside a persistent **tmux** session on
your HPC compute node. Sessions survive dropped Wi-Fi, closed laptops, and
overnight jobs. Your data never leaves the cluster — only terminal I/O
travels over SSH.

![Operon connected to an HPC cluster](../img/tour-hpc.png){ width=600 }

## Why HPC mode

| | |
|---|---|
| :material-tmux: **Persistent tmux sessions** | Every chat runs inside a named tmux session. Close your laptop, switch machines, reconnect next week — the session's still there. |
| :material-server: **Claude on the compute node** | The agent runs where the data lives. No round-tripping TB-scale files back to your laptop. SLURM jobs, GPU nodes, anywhere. |
| :material-console: **Respects your shell** | Conda envs, lmod modules, `.bashrc` aliases — all work. Operon injects commands into your actual shell, not `bash -c`. |
| :material-restore: **Session resume** | Reopen Operon on any Mac, Windows, or Linux machine. Active and completed sessions are waiting, hydrated from metadata on disk. |
| :material-folder-network: **Shared-filesystem aware** | Operon writes output files to the shared working directory, not node-local `/tmp`. Login and compute nodes see the same state. |
| :material-shield-key: **Duo, MFA, ProxyJump** | Native OpenSSH under the hood. Whatever auth dance your institution requires — including SSH agent forwarding. |
| :material-calendar-clock: **SLURM · PBS · SGE** | Schedule jobs from chat. Operon recognizes the big three schedulers and picks the right one based on the host's toolchain. |
| :material-broadcast: **Live output streaming** | A second SSH connection tails the agent's NDJSON log from the login node, so you can watch a compute-node job in real time. |

## Pages in this section

- [SSH connections](ssh.md) — setting up profiles, key auth, Duo/MFA, ProxyJump
- [tmux sessions](tmux.md) — why tmux is non-negotiable for remote work
- [SLURM / PBS / SGE](slurm.md) — interactive nodes, batch jobs, scheduler templates
- [Architecture](architecture.md) — how the three SSH connections fit together
- [HPC gotchas](gotchas.md) — `/tmp` is node-local, `claude` is an alias, etc.
