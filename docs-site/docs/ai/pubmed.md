# PubMed integration

Ground Claude's answers in real biomedical literature.

![PubMed toggle](../img/pubmed-toggle.png){ width=400 }

## Toggling PubMed

In **Ask mode**, the chat panel shows a **PubMed** pill button under the
text box. Click it to enable; it stays on for the session.

When enabled, Claude can search NCBI's PubMed database via the MCP server
(see [MCP catalog](mcp.md#pubmed-literature)) and cite real DOIs inline.

## When to use it

| Use case | Example prompt |
|---|---|
| Catching up on a field | *"What are the latest methods for single-cell ATAC peak calling?"* |
| Verifying a tool choice | *"Has anyone benchmarked DESeq2 vs edgeR on bulk RNA-seq with <6 replicates?"* |
| Finding a marker gene | *"What's the canonical microglia marker panel for adult human brain?"* |
| Methods writeup | *"Summarize how SCTransform handles overdispersion. Cite the paper."* |
| Reproducing a published analysis | *"How did this 2024 Nature paper [PMID:XXX] preprocess their Visium data?"* |

## What you get back

A typical PubMed-grounded answer includes:

- A direct prose answer
- Inline citations like `[Hafemeister & Satija, 2019, DOI: 10.1186/s13059-019-1874-1]`
- A "Sources" footer listing every paper used
- DOI links you can click open

## What it doesn't do

- **No full-text scraping** — searches operate on titles, abstracts, and
  MeSH terms via NCBI's E-utilities. For paywalled full-text, you'd need a
  separate tool with institutional access.
- **No citation count or impact-factor weighting** — Claude reads what
  PubMed returns and reasons over it; you should still sanity-check the
  prominence of cited work.
- **No preprint coverage** — PubMed indexes peer-reviewed only. For
  bioRxiv / medRxiv, add the bioRxiv MCP server (it's bundled — see
  [MCP catalog](mcp.md)).

## API limits

NCBI E-utilities is rate-limited to **3 requests/second** without an API
key. If you're hammering it for a literature-review session, register a
free NCBI API key in Settings → MCP → PubMed to bump to 10/sec. The
integration works without a key for casual use.

## Privacy

Your query strings go to NCBI's public E-utilities endpoint. Standard
research-grade etiquette: don't search anything you wouldn't paste into
the public PubMed website.

## Combining with Agent mode

You can't toggle PubMed *in* Agent mode — that's by design (Agent should
be doing work, not reading). But you can:

1. Use Ask + PubMed to scope a problem
2. Switch to Plan to design an analysis informed by the literature
3. Switch to Agent to execute

Each session preserves context, so the literature reasoning carries through.
