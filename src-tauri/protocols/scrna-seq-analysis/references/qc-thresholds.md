# QC Threshold Guidelines

## Recommended Thresholds by Organism

### Human
- min_genes: 200
- max_genes: 5000-8000 (tissue dependent)
- max_mito_pct: 20% (brain), 10% (PBMC), 15% (general)
- min_counts: 500

### Mouse
- min_genes: 200
- max_genes: 5000-6000
- max_mito_pct: 10-15%
- min_counts: 500

## When to Adjust
- Nuclei preps: higher mito threshold (up to 5% for nuclei is expected)
- FFPE samples: relax gene count thresholds
- Sorted populations: may need tighter thresholds
- Mixed species: filter each species separately
