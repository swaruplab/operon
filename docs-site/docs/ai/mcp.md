# MCP catalog

[Model Context Protocol](https://modelcontextprotocol.io/) is an open
standard from Anthropic for giving models structured, typed access to
external tools. Instead of Claude scraping HTML or faking API calls, each
MCP server exposes real endpoints with real return types.

Operon ships a curated catalog of biology-aware MCPs and lets you add any
community server.

## Bundled MCPs

### Research databases

#### PubMed — literature

Full-text literature search via NCBI's E-utilities. Cite real DOIs inline.

| Tool | What |
|---|---|
| `search_papers` | Query PubMed with keywords + filters |
| `fetch_abstract` | Get the full abstract by PMID |
| `get_doi` | DOI lookup |

No API key required. NCBI registration bumps the rate limit from
3 req/sec to 10 req/sec.

#### bioRxiv — preprints

Same shape, for preprints.

| Tool | What |
|---|---|
| `search_preprints` | Keyword + author + date range |
| `get_preprint` | Full details by DOI |
| `get_categories` | List subject areas |

#### GEO — expression data

Query GSE / GSM accessions, pull metadata, stream count matrices.

| Tool | What |
|---|---|
| `get_gse` | Series metadata |
| `list_samples` | All GSMs in a series |
| `download_matrix` | Streamed count matrix download with caching |

#### GTEx — tissue expression

Tissue-specific expression and eQTL lookup across GTEx v8.

| Tool | What |
|---|---|
| `gene_expression` | TPM by tissue |
| `tissue_specificity` | τ score |
| `eqtl_lookup` | Variant-gene associations |

#### KEGG — pathways

Pathway enrichment and gene-ontology crosswalks.

| Tool | What |
|---|---|
| `pathway_enrich` | Enrichment from a gene list |
| `get_pathway` | Structured pathway map |
| `gene_to_pathway` | Reverse lookup |

#### JASPAR — TF motifs

Transcription-factor PFMs / PWMs for motif enrichment.

| Tool | What |
|---|---|
| `get_motif` | Fetch motif by name |
| `scan_sequences` | Scan FASTA for matches |
| `enrichment` | Run enrichment over a peak set |

#### AlphaFold — predicted structures

Fetch predicted structures by UniProt ID with confidence-aware maps.

| Tool | What |
|---|---|
| `fetch_structure` | PDB by UniProt |
| `plddt` | Per-residue confidence |
| `align_homologs` | Multi-structure alignment |

#### Gene databases — annotation

Cross-species ortholog lookup, Ensembl / RefSeq / HGNC resolution.

| Tool | What |
|---|---|
| `resolve_id` | Any ID → any ID |
| `ortholog` | Cross-species mapping |
| `gene_info` | Annotation, summary, exons |

### Compute / workflow

#### LatchBio

Submit and track workflows on Latch's cloud-native bioinformatics platform.

#### BioMCP — structural

PDB retrieval, UniProt, AlphaFold combined.

### Core (filesystem / web / notebooks)

#### Filesystem

Typed file ops that respect `.gitignore`, sandbox to the project root, and
work identically over SSH.

| Tool | What |
|---|---|
| `read_file` | Read with optional line range |
| `write_file` | Write with diff approval |
| `list_dir` | Tree-style listing |

#### Web fetch

Controlled URL fetching with allowlists.

| Tool | What |
|---|---|
| `fetch_url` | GET with content-type detection |
| `fetch_pdf` | PDF → text extraction |
| `extract_text` | Strip HTML to plain text |

#### Notebook

Read and edit Jupyter notebooks cell-by-cell. Claude can re-run a cell and
inspect outputs without re-running the whole notebook.

| Tool | What |
|---|---|
| `read_cells` | All cells with outputs |
| `edit_cell` | Edit one cell by index |
| `execute_cell` | Re-execute a cell |

## Installing / toggling

1. **Settings → MCP**.
2. Every bundled server is listed with a description and tool preview.
3. Flip the switch — Operon auto-installs the server binary if needed and
   registers it with Claude.

## Custom MCPs

Paste any MCP server config (stdio, SSE, or HTTP) into the settings panel.
Operon validates the schema and wires it up — same UI, same approval model.

Example custom server (stdio):

```json
{
  "name": "my-custom-tool",
  "command": "uvx",
  "args": ["my-mcp-package"],
  "env": {
    "MY_API_KEY": "secret"
  }
}
```

## How it works under the hood

When Claude sends a tool call, Claude Code dispatches it to the registered
MCP server, which returns a typed response. Operon doesn't translate or
wrap — it lets Claude Code own the MCP plumbing.

If a server fails or times out, the error surfaces in the chat panel and
Claude can fall back to a plain-text answer or retry.
