# HPC gotchas

What we learned running this on real clusters. None of these are theoretical.

## `/tmp` is node-local

Writing output to `/tmp` on a compute node means the login node can't tail
it. Operon writes to your **shared working directory** so both sides see
the same file.

In your scripts: use `/scratch/$USER/...` or your home directory, never
`/tmp/...` for files that need to be visible across nodes.

## `claude` is often an alias

On most HPC clusters, `claude` resolves to `npx @anthropic-ai/claude-code`
(or a wrapper script in `~/.local/bin/`). That alias only exists in your
**interactive shell** — piping to `bash -c "claude ..."` loses it.

Operon runs commands **in your shell directly** (`bash -l -c` with the
profile sourced) to preserve aliases. If you script things outside Operon,
remember to either:

- Use the full path: `~/.local/bin/claude ...`
- Source your aliases: `shopt -s expand_aliases && source ~/.bashrc && claude ...`

## SSH 9.x post-quantum warnings

Newer OpenSSH (9.x+) emits warnings about `sntrup761x25519-sha512` and
`mlkem-x25519` key exchange algorithms:

```
sntrup761x25519-sha512 is experimental and may be incompatible with non-OpenSSH peers
```

These are **benign** — your connection works fine. Operon filters them
from stderr so they don't appear as false errors in the chat. If you see
them in a manual SSH session, ignore them.

## Quoting across SSH chains

Local shell → SSH → remote shell → `bash -c` is a nightmare of layered
quoting. A single quote that "should" work locally gets re-interpreted at
every layer.

Operon **base64-encodes** complex remote scripts before sending them over
SSH, then decodes on the remote side. If you write your own SSH-wrapped
commands, do the same:

```bash
local_script='cd "/path with spaces" && complex command'
b64=$(echo -n "$local_script" | base64)
ssh remote "echo $b64 | base64 -d | bash"
```

## Conda activation in scripts

`conda activate` doesn't work inside `bash -c` because it needs the
shell-function override that conda sources from your profile. Two fixes:

```bash
# Option 1: use `source activate` (the legacy command, still works)
source activate myenv

# Option 2: explicitly source conda's init script
source ~/miniconda3/etc/profile.d/conda.sh
conda activate myenv
```

Operon's protocols use option 2 by default.

## SLURM `srun` inside a job script

If you submit a SLURM batch job that needs to use `srun` to launch
parallel tasks, the inner `srun` inherits the job's allocation. Don't
re-specify `--cpus-per-task` etc. — let `srun` inherit. Operon's
SLURM-aware protocols handle this; double-specification is the most
common foot-gun.

## Duo / MFA on every connect

Some clusters require Duo on every fresh SSH connection. Operon uses
**ControlMaster** to multiplex multiple operations over one socket — meaning
you Duo once per session, not once per command.

If you see repeated Duo prompts, check that ControlMaster is enabled in
your SSH profile (it should be by default in Operon-managed profiles).

## Login node abuse

Don't run analyses on the login node. The cluster admins **will** kill your
process, possibly your account. Always:

1. SSH to the login node
2. Get an interactive compute node (`srun --pty bash` or equivalent)
3. Start tmux on the compute node
4. Open Operon's chat there

Sysadmins can usually tell — they watch CPU usage on the login node.

## `module load` in non-interactive shells

`module load` requires the `module` shell function, which is sourced from
`/etc/profile.d/modules.sh` (or similar). In non-interactive shells, this
isn't always sourced.

Operon uses `bash -l -c` (login shell) so the profile is loaded, but if you
write your own scripts, prepend:

```bash
source /etc/profile.d/modules.sh   # or wherever your cluster puts it
module load python/3.11
```

## Scratch quota

Cluster scratch directories often have time-based purge policies (7 days,
30 days). Don't leave AI session output files (`.operon-*.jsonl`) on
scratch you care about archiving. Either:

- Set a working directory on a persistent filesystem
- Or copy the JSONLs out of scratch into your project before purge day

Operon doesn't auto-archive these files — they're yours to manage.

## Network filesystems and locking

If your home directory is on NFS / Lustre / GPFS, file locking might be
flaky. Operon doesn't rely on lock files for anything critical (sessions
are append-only), but third-party tools (R's `.libPaths`, conda) can
occasionally choke. If you see "file is locked" errors with conda, try:

```bash
conda config --set use_lockfiles no
```
