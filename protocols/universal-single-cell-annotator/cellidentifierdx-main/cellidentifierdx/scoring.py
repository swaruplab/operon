import numpy as np
from scipy.stats import dirichlet

def bayesian_score(cluster_genes, ref_genes, cluster_pvals_adj):
    alpha = np.array([1.0] * len(cluster_genes))
    pvals_adj = np.array([cluster_pvals_adj[gene] if gene in ref_genes else 1 for gene in cluster_genes])
    gene_expr_prob = 1 - pvals_adj
    alpha += gene_expr_prob
    posterior_mean = dirichlet.mean(alpha)
    matched_genes = len(set(cluster_genes) & set(ref_genes))
    proportion_matched_genes = matched_genes / len(ref_genes)
    bayesian_score = np.sum(posterior_mean) * proportion_matched_genes
    return bayesian_score
