# SLURM / PBS / SGE

Operon recognizes the big three HPC schedulers and helps Claude generate
appropriate job scripts.

## Interactive vs. batch

There are two ways to use HPC compute from Operon. Pick based on workload.

### Interactive node — for exploration and Agent mode

When you want Claude to **run commands and react to their output**
(Agent mode), you need an interactive shell on a node with enough
resources:

=== "SLURM"

    ```bash
    srun --pty --nodes=1 --cpus-per-task=8 \
         --mem=64G --time=04:00:00 \
         --gres=gpu:1 --partition=gpu bash
    ```

=== "PBS / Torque"

    ```bash
    qsub -I -l nodes=1:ppn=8,mem=64gb,walltime=04:00:00
    ```

=== "SGE"

    ```bash
    qlogin -pe smp 8 -l h_vmem=8G,h_rt=04:00:00
    ```

Once you land on the compute node, **start (or attach to) a tmux session**
([why tmux](tmux.md)), then chat with Claude as normal.

### Batch job — for very long runs

For runs that take many hours (genome alignment, model training), don't
sit on an interactive node — submit a batch job. Ask Claude (in Plan or
Agent mode) to generate the script:

> *"Write a SLURM script that aligns these FASTQs with STAR, requesting
> 8 cores, 64GB, gpu:1, walltime 8h. Save as `align.sh` and submit it."*

Claude can also poll the queue and report when the job finishes.

## Scheduler templates

### SLURM

```bash
#!/bin/bash
#SBATCH --job-name=operon
#SBATCH --cpus-per-task=8
#SBATCH --mem=64G
#SBATCH --time=04:00:00
#SBATCH --gres=gpu:1
#SBATCH --partition=gpu
#SBATCH --output=logs/%j.out
#SBATCH --error=logs/%j.err

set -euo pipefail
module load conda
source activate myenv

# ... your commands ...
```

### PBS / Torque

```bash
#!/bin/bash
#PBS -N operon
#PBS -l nodes=1:ppn=8:gpus=1
#PBS -l mem=64gb
#PBS -l walltime=04:00:00
#PBS -q gpu
#PBS -o logs/operon.out
#PBS -e logs/operon.err

cd $PBS_O_WORKDIR
module load conda
source activate myenv

# ... your commands ...
```

### SGE

```bash
#!/bin/bash
#$ -N operon
#$ -pe smp 8
#$ -l h_vmem=8G
#$ -l h_rt=04:00:00
#$ -l gpu=1
#$ -o logs/operon.out
#$ -e logs/operon.err

source ~/.bashrc
conda activate myenv

# ... your commands ...
```

## Telling Claude about your cluster

When you're in Plan or Agent mode on an HPC host, **tell Claude about the
environment**:

- Available modules (paste `module avail` output)
- Conda environments worth using
- Scratch vs. home directory conventions (`/scratch/$USER` for big files)
- Any lab-specific submission patterns

Operon detects which scheduler is available (`sbatch` / `qsub`) and uses
that in its protocol templates, but it doesn't know your specific
partition names, account codes, or QoS settings. A two-sentence brief at
the start of the session goes a long way.

## Monitoring jobs from chat

You can ask Claude to check on a job:

> *"Is job 12345 still running? Show me the latest log lines."*

Claude will run `squeue -u $USER` (or the PBS/SGE equivalent), parse the
output, and tail the log file.

## Common partitions / accounts

If your cluster has named partitions like `gpu-a100`, `cpu-bigmem`, or
account codes you must charge against, mention them upfront:

> *"Submit to partition `gpu-a100` charging account `lab_swarup`. Walltime
> max 24h."*

Claude will include those flags in any script it generates.
