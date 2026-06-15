# CellAgent (CellTypeAgent)

**ID:** `biomedical.genomics.cell_annotation`
**Version:** 1.1.0
**Status:** Integrated & Downloaded
**Category:** Genomics / Single-Cell Annotation

---

## Overview

CellTypeAgent is an LLM-driven framework for automated cell type annotation in scRNA-seq data. It interprets marker genes, assigns labels, and generates explanations with confidence scores.

---

## Inputs

| Field | Type | Notes |
|------|------|------|
| `markers` | list[str] | Marker genes per cluster |
| `species` | str | `human` or `mouse` |
| `tissue` | str | Optional tissue context |
| `reference` | str | Optional atlas reference |

---

## Outputs

- Cell type labels per cluster
- Confidence score and evidence markers
- Suggested ambiguous or mixed identities

---

## Quick Start

```bash
cd repo
pip install -r requirements.txt
python repo/main.py --data your_data.h5ad
```

---

## Guardrails

- Do not over-specify cell types without marker support.
- Use tissue context to resolve ambiguous marker sets.
- Flag clusters with mixed signatures for manual review.

---

## References

- https://github.com/jianghao-zhang/CellTypeAgent
