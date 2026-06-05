# Recipe: PubMed-grounded literature review

A real literature-scoping exercise using Operon's Ask mode + the PubMed
MCP. By the end you'll have a cited writeup you can adapt into a paper's
introduction or grant aims section.

## What you'll build

- A focused literature search on your topic
- A curated reading list of ~10-20 papers
- A synthesis paragraph with inline DOI citations
- (Optional) A BibTeX file for downstream LaTeX use

## Inputs

- A research question. Specificity matters — "single-cell" is too broad;
  "single-cell ATAC peak calling in low-input samples" is the right
  shape.

No data files needed for this recipe — everything happens in chat.

## Setup

1. Open a project folder where you want to save the writeup (anywhere
   works — even a fresh empty directory).
2. **Settings → MCP** — make sure **PubMed** is toggled on. Optionally
   toggle **bioRxiv** too if you want preprint coverage.
3. New chat session in **Ask** mode. Toggle the **PubMed** pill button
   under the text input.

## Step 1 — Scope the question

Don't dump your final question on Claude immediately. Start with scope:

> *I want to write a literature review on `<topic>`. Before I commit to
> a specific question, help me understand the landscape. What are the
> main subtopics I should know about? What are the canonical papers in
> each?*

Claude searches PubMed and returns:

- 3-5 subtopics
- The 2-3 most-cited / foundational papers per subtopic
- Recent (last 2 years) developments

This gives you a map. From here you can either:

- Drill into a subtopic
- Pick a specific angle (e.g. methodological gaps, applications, etc.)

## Step 2 — Focused search

Once you've picked a focused angle:

> *Now I'm focusing on `<specific angle>`. Search PubMed for the most
> cited papers in this area from 2023-2026, plus any landmark earlier
> papers. Give me a short annotated bibliography — title, authors, year,
> DOI, and one sentence on what the paper contributes.*

You get a 10-20 paper reading list with citations.

## Step 3 — Read and probe

Don't just take Claude's word for what each paper says. Drill in:

> *Tell me more about [PMID XYZ]. What method did they use? What were
> the main limitations they acknowledged?*

Claude pulls the abstract and discusses. If you have institutional
full-text access via your institution, you can paste in a section text
and ask Claude to summarize / critique:

> *Here is the methods section of [PMID XYZ]: ...
>
> Two questions: (1) what assumptions does their statistical model
> make? (2) does this method scale to ~1M cells?*

## Step 4 — Identify gaps

A good review identifies what's missing, not just what exists:

> *Across the papers we've discussed, what are the consistent gaps or
> unresolved questions? Are there obvious experiments nobody has done
> yet?*

This is where Claude shines — synthesizing across multiple papers to
spot patterns.

## Step 5 — Write the synthesis

Switch to **Report** mode and ask for the writeup:

> *Write a 600-word literature review section on `<focused angle>` based
> on the papers we've discussed. Use inline citations with DOIs. Save as
> literature_review.md.*

Report mode is restricted to read-only tools, so it can't make things
up via Bash; it has to ground every claim in the conversation history
and the cited papers.

You can iterate:

> *The third paragraph is too vague on the methodological controversies.
> Expand it with specific contrasts between [PMID A] and [PMID B].*

## Step 6 — BibTeX for LaTeX

Final step:

> *Generate a BibTeX file with entries for every paper we cited. Use
> the format `LastNameYear` for citation keys. Save as `refs.bib`.*

## Variations

### Preprints included

Toggle the **bioRxiv** MCP server in Settings. Then add to your prompts:
"Include preprints from the last 12 months."

### Specific journals only

> *Restrict the search to Nature, Science, Cell, and their methods
> spin-offs (Nat Methods, Nat Biotech, etc.).*

PubMed has venue filtering.

### Compare two specific tools

> *Search PubMed for benchmarks comparing [Tool A] and [Tool B]. List
> the papers, summarize the consensus on which is better for [specific
> use case], and tell me if any group is the obvious benchmark authority.*

## Pitfalls

- **Claude is good at synthesis, not at verifying citation accuracy.**
  Spot-check 2-3 random citations — pull the DOI, open the paper, make
  sure Claude's one-sentence summary actually matches. PubMed grounding
  reduces hallucinations but doesn't eliminate them.
- **"Most cited" requires a citation database** — PubMed doesn't have
  this. Claude approximates from publication venue + year, which is
  imperfect. For truly citation-ranked queries, use Semantic Scholar or
  Web of Science separately.
- **Paywalled content** — PubMed only returns abstracts. For full-text
  reasoning, you'll need institutional access and paste-in. Operon's MCP
  doesn't bypass paywalls.
- **Bias toward English** — PubMed's English-language bias affects what
  Claude finds. For European clinical literature in non-English journals,
  add "Include German / French / Italian / Spanish abstracts" explicitly.
- **Topic drift** — over a long session, Claude's focus drifts. If you
  notice it pulling tangentially related papers, refocus:
  "Step back — what specifically were we asking about?"

## Sanity checks

After Step 5 (the synthesis paragraph):

- Click 2-3 DOIs to verify they exist
- Read the abstracts and confirm Claude's summary matches
- Check that the references support the claims they're attached to —
  Claude sometimes attaches a citation to the *adjacent* claim, not the
  one it actually supports

## Next steps

- Use the writeup as a starting point for a paper's introduction
- Take the gap analysis (Step 4) into Plan mode in another session to
  design experiments addressing it
- Run a [scRNA-seq](scrna-pbmc.md) / [bulk RNA-seq](bulk-rnaseq-deseq2.md)
  recipe to actually generate the data
