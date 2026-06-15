---
name: chematagent-drug-discovery
description: Chemical Lab Agent
keywords:
  - chemistry
  - drug-discovery
  - tools
  - synthesis
  - property-prediction
measurable_outcome: Plan a synthesis route and predict ADMET properties for a candidate molecule with >80% validity.
license: MIT
metadata:
  author: CheMatAgent Team
  version: "1.0.0"
compatibility:
  - system: Python 3.9+
allowed-tools:
  - run_shell_command
  - read_file
---

# CheMatAgent

A two-tiered agent system with access to 137 Python-wrapped chemical tools for drug discovery and materials science.

## When to Use

*   **Molecule Design**: Generating novel structures with specific properties.
*   **Property Prediction**: Estimating solubility, toxicity, and bioactivity.
*   **Synthesis Planning**: Designing retro-synthetic routes.

## Core Capabilities

1.  **Tool Orchestration**: Manages a library of 137 chemical tools.
2.  **Multi-Scale Modeling**: Bridges quantum mechanics and molecular dynamics.
3.  **Lab Automation**: Generates instructions for robotic synthesis platforms.

## Workflow

1.  **Goal**: Define target property (e.g., "LogP < 5").
2.  **Design**: Generate candidates.
3.  **Filter**: Use property prediction tools.
4.  **Plan**: Output synthesis recipe.

## Example Usage

**User**: "Design a molecule similar to Aspirin but with higher solubility."

**Agent Action**:
```bash
python -m chematagent.design --scaffold "Aspirin" --objective "maximize solubility"
```
