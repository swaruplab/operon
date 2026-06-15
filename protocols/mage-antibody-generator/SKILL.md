---
name: mage-antibody-generator
description: Ab seq forge
keywords:
  - antibody
  - antigen
  - FASTA
  - generation
  - validation
measurable_outcome: Generate the requested number of antibody sequences (default ≥5) with metadata (model checkpoint, seed) and deliver FASTA files within 10 minutes.
license: MIT
metadata:
  author: MAGE Team
  version: "1.0.0"
compatibility:
  - system: Python 3.9+ / GPU
allowed-tools:
  - run_shell_command
  - read_file
---

# MAGE (Monoclonal Antibody Generator)

Run the MAGE antibody generation workflow to propose antigen-conditioned antibody sequences for downstream structural validation.

## Workflow
1. **Prep env:** `cd repo` and install dependencies, then point to GPU if available.
2. **Run generator:** `python generate_antibodies.py --antigen_sequence <SEQ> --num_candidates N --output_dir ./results`.
3. **Collect outputs:** Provide FASTA paths + metadata, optionally translate into JSON manifest.
4. **Recommend validation:** Suggest AlphaFold/Rosetta checks and wet-lab follow-up.

## Guardrails
- Never imply binding efficacy without structural/experimental confirmation.
- Track model version + seeds to ensure reproducibility.
- Encourage downstream filtering (liability motifs, developability metrics).

## References
- Source instructions in `README.md` and repo scripts.
