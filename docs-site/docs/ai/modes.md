# Four AI modes

Switch modes with the dropdown at the top of the chat panel. Each mode
changes how Claude approaches your request.

![AI modes dropdown](../img/ai-modes-dropdown.png){ width=400 }

## Agent

> Executes multi-step tasks autonomously.

The default mode and the most powerful. Claude can:

- Read and write files in your project
- Run terminal commands
- Install packages (pip, conda, brew, apt, dnf, npm)
- Execute scripts, iterate on errors
- Submit SLURM / PBS / SGE jobs over SSH
- Track its progress against an `implementation_plan.md` if one exists

Use it for **doing work**: "Run a DESeq2 analysis on the count matrix in
`data/`", "Set up a Nextflow pipeline for variant calling", "Debug this
Scanpy error".

**Stop button:** the chat panel always has a Stop button while Agent is
running. ++esc++ also stops the session.

## Plan

> Thinks before it acts.

Plan mode produces an `implementation_plan.md` in your project root — a
checklist of steps, tools, expected outputs, and assumptions. **It does
not execute.** You iterate on the plan in chat (request changes, ask
questions, push back) until it's right, then switch to Agent to run it.

Agent mode picks up the plan automatically: when it sees
`implementation_plan.md` in the working directory, it loads it as context
and marks each step `[x]` as it completes.

**When to use Plan first:**

- The analysis is complex enough that you want to review the approach
  before any commands run
- You're on a remote cluster where mistakes waste queued compute
- You're not sure which protocol fits and want Claude to think it through
- You need a record of the decision-making for a methods section

Plan mode caps at 3 turns by default — it's intentionally brief.

## Ask

> Pure Q&A. No file writes, no commands.

Use Ask for:

- Understanding outputs ("What does this volcano plot mean?")
- Explaining statistics ("Why did DESeq2 shrink these LFCs?")
- Architectural decisions ("Should I use Seurat or Scanpy for this study?")
- **Literature review** with the [PubMed toggle](pubmed.md) on

Ask doesn't touch your filesystem — it's safe to leave running while you're
working in another panel.

## Report

> Produces a structured scientific writeup.

The fourth mode, added in v0.7.x. Report:

- Reads project files (figures, results tables, logs)
- Produces a methods + results section ready to drop into a manuscript
- **Restricts itself** to read-only tools — no `Bash`, no file edits —
  so the writeup is grounded in what's actually in your project rather
  than what Claude could fabricate
- Defaults to 6 turns of context-gathering

A typical Report prompt:

> *"Write the methods and results section for this scRNA-seq paper. The
> figures are in `figures/`. The count matrices and QC metrics are in
> `data/`. Cite tool versions from `environment.yml`."*

The output is the report file (the user picks the filename) — Markdown,
publication-grade.

## Mode picker quick-reference

| You want to | Mode |
|---|---|
| Get Claude to actually do something | **Agent** |
| Design a workflow before running it | **Plan** |
| Ask a question, get an answer | **Ask** |
| Generate a methods / results writeup | **Report** |
| Read papers without code | **Ask** + PubMed toggle |
