import numpy as np
from cellidentifierdx.scoring import bayesian_score

def test_bayesian_score():
    cluster_genes = ['gene1', 'gene2', 'gene3']
    ref_genes = ['gene1', 'gene3']
    cluster_pvals_adj = {'gene1': 0.01, 'gene2': 0.05, 'gene3': 0.001}
    score = bayesian_score(cluster_genes, ref_genes, cluster_pvals_adj)
    assert np.isclose(score, 0.66666666, atol=1e-08)
