#!/usr/bin/env python3
"""
Topological decoder for parametric curve image encoding.

Decodes payload words from CIELAB colors using two complementary methods
from discrete differential geometry (Keenan Crane's formulation):

1. **Rips filtration decoder** (primary): grow a connection radius on the
   color-space graph and detect jumps in beta_0 (connected components).
   Each persistent component corresponds to a palette word neighborhood.
   This is 0-dimensional persistent homology on the Vietoris-Rips complex.

2. **Spectral decoder** (secondary): normalized graph Laplacian on the
   similarity graph, eigenvectors embed colors into R^k, k-means assigns
   clusters. The eigengap corresponds to the persistence gap.

Both operate on arc-length projections: each color is projected onto
the palette curve gamma to get a 1D parameter s, then the filtration /
clustering operates in this projected coordinate where palette words
are well-separated (spacing = L/(N-1)) regardless of constellation
displacement magnitude.

Key property: **rendering-agnostic**. Uses only color values, not spatial
coordinates. Works for Voronoi, grids, mosaics, brush strokes, etc.

Usage:
    from spectral_decoder import rips_decode, spectral_decode

    # Persistence-based (recommended):
    words = rips_decode(pixels_lab, curve, frame, n_palette,
                        constellation_map=cmap)

    # With full diagnostics:
    words, diag = rips_decode(pixels_lab, curve, frame, n_palette,
                              constellation_map=cmap,
                              return_diagnostics=True)
    print(diag['persistence_diagram'])  # (birth, death) pairs
    print(diag['persistence_gap'])      # gap that determines k
    print(diag['cut_level'])            # filtration threshold
"""

import numpy as np
from scipy.spatial.distance import pdist, squareform
from scipy.linalg import eigh
from scipy.optimize import linear_sum_assignment

from parametric_encoding import (
    PaletteCurve, BishopFrame, ConstellationMap,
    compute_tube_radius, EPSILON,
)


# =========================================================================
# Rips Filtration Decoder (persistent homology, H_0)
# =========================================================================

class UnionFind:
    """Weighted union-find with path compression for H_0 persistence."""

    def __init__(self, n):
        self.parent = list(range(n))
        self.rank = [0] * n
        self.size = [1] * n

    def find(self, x):
        while self.parent[x] != x:
            self.parent[x] = self.parent[self.parent[x]]  # path compression
            x = self.parent[x]
        return x

    def union(self, x, y):
        """Merge components of x and y. Returns (survivor, absorbed) or None."""
        px, py = self.find(x), self.find(y)
        if px == py:
            return None
        # Union by rank: smaller tree absorbed into larger
        if self.rank[px] < self.rank[py]:
            px, py = py, px  # px is the survivor
        self.parent[py] = px
        self.size[px] += self.size[py]
        if self.rank[px] == self.rank[py]:
            self.rank[px] += 1
        return (px, py)  # (survivor, absorbed)


def rips_filtration_h0(distances_1d):
    """Compute 0-dimensional persistent homology via Rips filtration.

    For a 1D point set, this is equivalent to single-linkage clustering.
    Grow the connection radius epsilon from 0 to infinity and track when
    connected components merge.

    Each point is born at epsilon = 0. When two components merge at
    epsilon = d, the younger component (born later or smaller) dies.

    Args:
        distances_1d: (n,) array of 1D values (arc-length projections)

    Returns:
        persistence: list of (birth, death, component_idx) triples
        merge_sequence: list of (epsilon, survivor, absorbed) triples
            in order of increasing epsilon
    """
    n = len(distances_1d)
    if n <= 1:
        return [], []

    # Build all edges sorted by distance
    # In 1D, only adjacent points (in sorted order) matter for single-linkage,
    # but we compute all pairs for generality
    edges = []
    for i in range(n):
        for j in range(i + 1, n):
            d = abs(distances_1d[i] - distances_1d[j])
            edges.append((d, i, j))
    edges.sort()

    # Union-find: process edges in order of increasing distance
    uf = UnionFind(n)
    persistence = []     # (birth, death, absorbed_component)
    merge_sequence = []  # (epsilon, survivor, absorbed)

    for d, i, j in edges:
        result = uf.union(i, j)
        if result is not None:
            survivor, absorbed = result
            persistence.append((0.0, d, absorbed))
            merge_sequence.append((d, survivor, absorbed))

    return persistence, merge_sequence


def find_persistence_gap(persistence, n_max=None):
    """Find the largest gap in death times to determine optimal cluster count.

    The gap separates short-lived components (within-cluster merges) from
    long-lived components (between-cluster separations). This is the
    topological analogue of the eigengap.

    Args:
        persistence: list of (birth, death, idx) from rips_filtration_h0
        n_max: maximum number of clusters to consider

    Returns:
        k_opt: optimal number of clusters
        cut_level: filtration threshold (midpoint of the gap)
        gap_size: size of the largest gap
        deaths_sorted: sorted death times for visualization
    """
    if not persistence:
        return 1, 0.0, 0.0, []

    deaths = sorted([death for _, death, _ in persistence])
    n_total = len(deaths) + 1  # +1 for the last surviving component

    if n_max is not None:
        # We only care about gaps that leave at most n_max clusters
        # k clusters means n-k merges happened, so we look at deaths[:n-k]
        # Actually, reading deaths left to right: after i merges, we have
        # (n_total - i) clusters. We want n_total - i >= 1 and <= n_max.
        pass

    # Find largest gap in death times
    if len(deaths) < 2:
        return 1, deaths[0] / 2 if deaths else 0.0, 0.0, deaths

    gaps = np.diff(deaths)
    best_gap_idx = int(np.argmax(gaps))
    gap_size = float(gaps[best_gap_idx])

    # Cut level = midpoint of the gap
    cut_level = (deaths[best_gap_idx] + deaths[best_gap_idx + 1]) / 2.0

    # Number of clusters = n_total - number of merges before cut
    n_merges_before_cut = sum(1 for d in deaths if d <= cut_level)
    k_opt = n_total - n_merges_before_cut

    if n_max is not None:
        k_opt = min(k_opt, n_max)

    return k_opt, cut_level, gap_size, deaths


def rips_decode(pixels_lab, curve, frame, n_palette,
                constellation_map=None,
                return_diagnostics=False):
    """Decode CIELAB colors using Rips filtration on arc-length projections.

    Drop-in replacement for parametric_encoding.decode().

    Algorithm (Vietoris-Rips H_0 on the palette curve):
      1. Project each color onto gamma to get arc-length parameter s_i
      2. Build Rips filtration on {s_i}: grow epsilon, track beta_0
      3. Persistence gap separates within-word merges (small epsilon)
         from between-word merges (epsilon ~ palette spacing)
      4. Cut at the gap: connected components = palette word groups
      5. Hungarian matching maps components to word indices
      6. Bishop frame recovers constellation position per pixel

    This is rendering-agnostic: only color values are used.

    Args:
        pixels_lab: (n, 3) CIELAB colors
        curve: PaletteCurve
        frame: BishopFrame
        n_palette: number of palette colors (N)
        constellation_map: ConstellationMap
        return_diagnostics: return (words, diagnostics_dict)

    Returns:
        words: list of ints (word indices in pixel order)
        diagnostics: dict (if return_diagnostics) with:
            s_values: arc-length projections
            persistence: (birth, death, idx) triples
            persistence_gap: largest gap in death times
            cut_level: filtration threshold
            k_detected: number of clusters found
            cluster_labels: per-pixel assignments
            cluster_to_word: mapping
            assignment_cost: Hungarian cost
    """
    pixels_lab = np.asarray(pixels_lab, dtype=np.float64)
    n = len(pixels_lab)

    if n == 0:
        return ([], {}) if return_diagnostics else []

    # Constellation map
    if constellation_map is None:
        s_pal = np.array([
            w * curve.arc_length / max(n_palette - 1, 1)
            for w in range(n_palette)
        ])
        _, radii = compute_tube_radius(curve, frame, s_values=s_pal)
        constellation_map = ConstellationMap(radii)

    # Palette points
    s_palette = np.array([
        w * curve.arc_length / max(n_palette - 1, 1)
        for w in range(n_palette)
    ])
    palette_pts = curve.eval(s_palette)

    # === Stage 1: Project onto curve ===
    # For each color, find closest point on gamma -> arc-length s_i
    s_values = np.zeros(n)
    for i in range(n):
        s_nearest, _ = curve.project(pixels_lab[i].reshape(1, 3))
        s_values[i] = float(s_nearest[0])

    # === Stage 2: Rips filtration on 1D arc-length values ===
    persistence, merge_seq = rips_filtration_h0(s_values)

    # Find the persistence gap
    k_detected, cut_level, gap_size, deaths = find_persistence_gap(
        persistence, n_max=n_palette)

    # === Stage 3: Cut the filtration -> connected components ===
    # Re-run union-find up to cut_level
    # Build edges sorted by |s_i - s_j|
    edges = []
    for i in range(n):
        for j in range(i + 1, n):
            d = abs(s_values[i] - s_values[j])
            edges.append((d, i, j))
    edges.sort()

    uf = UnionFind(n)
    for d, i, j in edges:
        if d > cut_level:
            break
        uf.union(i, j)

    # Get component labels
    labels = np.array([uf.find(i) for i in range(n)])
    unique_labels = np.unique(labels)
    # Remap to 0..k-1
    label_map = {old: new for new, old in enumerate(unique_labels)}
    labels = np.array([label_map[l] for l in labels])
    k = len(unique_labels)

    # === Stage 4: Match components to palette words ===
    # Compute mean s per component, match to palette s values
    component_s = np.zeros(k)
    component_lab = np.zeros((k, 3))
    cluster_sizes = {}
    for c in range(k):
        mask = labels == c
        count = int(mask.sum())
        cluster_sizes[c] = count
        if count > 0:
            component_s[c] = s_values[mask].mean()
            component_lab[c] = pixels_lab[mask].mean(axis=0)

    # Hungarian matching: minimize |component_s - palette_s|
    cost_matrix = np.zeros((k, n_palette))
    for i in range(k):
        cost_matrix[i] = np.abs(s_palette - component_s[i])

    row_ind, col_ind = linear_sum_assignment(cost_matrix)
    cluster_to_word = {}
    assignment_cost = 0.0
    for r, c in zip(row_ind, col_ind):
        cluster_to_word[int(r)] = int(c)
        assignment_cost += cost_matrix[r, c]

    # === Stage 5: Assign words ===
    words = np.zeros(n, dtype=int)
    for i in range(n):
        c = int(labels[i])
        if c in cluster_to_word:
            words[i] = cluster_to_word[c]
        else:
            # Fallback: nearest palette by s
            words[i] = int(np.argmin(np.abs(s_palette - s_values[i])))

    result = words.tolist()

    if return_diagnostics:
        diagnostics = {
            's_values': s_values,
            'persistence': persistence,
            'persistence_gap': gap_size,
            'cut_level': cut_level,
            'k_detected': k_detected,
            'k_actual': k,
            'cluster_labels': labels,
            'cluster_sizes': cluster_sizes,
            'cluster_to_word': cluster_to_word,
            'assignment_cost': assignment_cost,
            'deaths_sorted': deaths,
            'palette_spacing': float(curve.arc_length / max(n_palette - 1, 1)),
        }
        return result, diagnostics

    return result


# =========================================================================
# Spectral Decoder (graph Laplacian)
# =========================================================================

def gaussian_similarity(values_1d, sigma):
    """Build Gaussian similarity on 1D values (arc-length projections)."""
    n = len(values_1d)
    diff = values_1d[:, None] - values_1d[None, :]
    W = np.exp(-diff ** 2 / (sigma ** 2))
    np.fill_diagonal(W, 0)
    return W


def normalized_laplacian(W):
    """Compute L_sym = I - D^{-1/2} W D^{-1/2}."""
    d = W.sum(axis=1)
    d[d < 1e-10] = 1e-10
    d_inv_sqrt = 1.0 / np.sqrt(d)
    W_norm = W * np.outer(d_inv_sqrt, d_inv_sqrt)
    L_sym = np.eye(len(W)) - W_norm
    return L_sym, d_inv_sqrt


def kmeans_numpy(X, k, n_init=10, max_iter=100, seed=42):
    """K-means with k-means++ init, pure numpy."""
    rng = np.random.RandomState(seed)
    n, d = X.shape
    k = min(k, n)

    best_labels = np.zeros(n, dtype=int)
    best_centroids = X[:k].copy()
    best_inertia = np.inf

    for init in range(n_init):
        centroids = np.empty((k, d))
        centroids[0] = X[rng.randint(n)]
        for j in range(1, k):
            dists = np.min(
                np.sum((X[:, None, :] - centroids[None, :j, :]) ** 2, axis=2),
                axis=1)
            probs = dists / (dists.sum() + 1e-10)
            centroids[j] = X[rng.choice(n, p=probs)]

        for _ in range(max_iter):
            dists = np.sum(
                (X[:, None, :] - centroids[None, :, :]) ** 2, axis=2)
            labels = np.argmin(dists, axis=1)
            new_centroids = np.empty_like(centroids)
            for j in range(k):
                mask = labels == j
                new_centroids[j] = X[mask].mean(axis=0) if mask.any() else X[rng.randint(n)]
            if np.allclose(centroids, new_centroids, atol=1e-8):
                centroids = new_centroids
                break
            centroids = new_centroids

        inertia = sum(np.sum((X[labels == j] - centroids[j]) ** 2) for j in range(k))
        if inertia < best_inertia:
            best_inertia = inertia
            best_labels = labels.copy()
            best_centroids = centroids.copy()

    return best_labels, best_centroids, best_inertia


def spectral_decode(pixels_lab, curve, frame, n_palette,
                    constellation_map=None,
                    sigma_kernel=None,
                    return_diagnostics=False):
    """Decode via spectral clustering on arc-length-projected similarity graph.

    Uses the normalized graph Laplacian on a 1D similarity graph built
    from arc-length projections onto gamma. The eigengap determines
    the number of clusters (= palette words present in the data).

    Args:
        pixels_lab: (n, 3) CIELAB colors
        curve: PaletteCurve
        frame: BishopFrame
        n_palette: number of palette colors
        constellation_map: ConstellationMap
        sigma_kernel: Gaussian bandwidth for 1D similarity (if None,
                      set to 0.4 * palette spacing)
        return_diagnostics: return (words, diagnostics)

    Returns:
        words: list of ints (word indices)
        diagnostics: dict (if return_diagnostics)
    """
    pixels_lab = np.asarray(pixels_lab, dtype=np.float64)
    n = len(pixels_lab)

    if n == 0:
        return ([], {}) if return_diagnostics else []

    if constellation_map is None:
        s_pal = np.array([
            w * curve.arc_length / max(n_palette - 1, 1)
            for w in range(n_palette)
        ])
        _, radii = compute_tube_radius(curve, frame, s_values=s_pal)
        constellation_map = ConstellationMap(radii)

    s_palette = np.array([
        w * curve.arc_length / max(n_palette - 1, 1)
        for w in range(n_palette)
    ])
    palette_pts = curve.eval(s_palette)
    pal_spacing = curve.arc_length / max(n_palette - 1, 1)

    # Project onto curve -> 1D arc-length
    s_values = np.zeros(n)
    for i in range(n):
        s_nearest, _ = curve.project(pixels_lab[i].reshape(1, 3))
        s_values[i] = float(s_nearest[0])

    # Similarity on 1D projections
    if sigma_kernel is None:
        sigma_kernel = pal_spacing * 0.4

    W = gaussian_similarity(s_values, sigma_kernel)

    # Laplacian
    L_sym, d_inv_sqrt = normalized_laplacian(W)
    eigenvalues, eigenvectors = eigh(L_sym)
    eigenvalues = np.maximum(eigenvalues, 0.0)

    # Eigengap -> number of clusters
    max_k = min(n_palette, n - 1)
    gaps = np.diff(eigenvalues[:max_k + 1])
    if len(gaps) > 1:
        k = int(np.argmax(gaps[1:]) + 2)
    else:
        k = 1
    k = max(1, min(k, n_palette, n))

    # Spectral embedding
    U = eigenvectors[:, 1:k + 1]
    row_norms = np.linalg.norm(U, axis=1, keepdims=True)
    row_norms[row_norms < 1e-10] = 1.0
    U_norm = U / row_norms

    # K-means in spectral space
    labels, _, inertia = kmeans_numpy(U_norm, k, n_init=10, seed=42)

    # Match to palette words via mean s
    component_s = np.zeros(k)
    cluster_sizes = {}
    for c in range(k):
        mask = labels == c
        cluster_sizes[c] = int(mask.sum())
        if mask.any():
            component_s[c] = s_values[mask].mean()

    cost_matrix = np.zeros((k, n_palette))
    for i in range(k):
        cost_matrix[i] = np.abs(s_palette - component_s[i])
    row_ind, col_ind = linear_sum_assignment(cost_matrix)
    cluster_to_word = {int(r): int(c) for r, c in zip(row_ind, col_ind)}

    words = np.zeros(n, dtype=int)
    for i in range(n):
        c = int(labels[i])
        if c in cluster_to_word:
            words[i] = cluster_to_word[c]
        else:
            words[i] = int(np.argmin(np.abs(s_palette - s_values[i])))

    result = words.tolist()

    if return_diagnostics:
        eigengap = float(eigenvalues[k] - eigenvalues[k - 1]) if k < len(eigenvalues) else 0.0
        diagnostics = {
            's_values': s_values,
            'eigenvalues': eigenvalues,
            'eigengap': eigengap,
            'k_used': k,
            'cluster_labels': labels,
            'cluster_sizes': cluster_sizes,
            'cluster_to_word': cluster_to_word,
            'sigma_kernel': sigma_kernel,
        }
        return result, diagnostics

    return result


# =========================================================================
# Comparison infrastructure
# =========================================================================

def compare_decoders(pixels_lab, curve, frame, n_palette,
                     constellation_map=None, payload_truth=None,
                     noise_sigma=0.0, n_trials=1, verbose=True):
    """Run geometric, Rips, and spectral decoders side by side."""
    from parametric_encoding import decode as geometric_decode

    decoders = {
        'geometric': lambda px: geometric_decode(
            px, curve, frame, n_palette, constellation_map=constellation_map),
        'rips': lambda px: rips_decode(
            px, curve, frame, n_palette, constellation_map=constellation_map),
        'spectral': lambda px: spectral_decode(
            px, curve, frame, n_palette, constellation_map=constellation_map),
    }

    results = {name: {'correct': 0, 'total': 0, 'words': None}
               for name in decoders}

    for trial in range(n_trials):
        if noise_sigma > 0:
            rng = np.random.RandomState(trial)
            noisy = pixels_lab + rng.normal(0, noise_sigma, pixels_lab.shape)
            noisy[:, 0] = np.clip(noisy[:, 0], 0, 100)
        else:
            noisy = pixels_lab

        for name, decode_fn in decoders.items():
            words = decode_fn(noisy)
            n = len(words)
            results[name]['total'] += n
            results[name]['words'] = words
            if payload_truth is not None:
                truth = list(payload_truth)
                results[name]['correct'] += sum(
                    1 for a, b in zip(truth, words) if a == b)

    for name in decoders:
        total = results[name]['total']
        correct = results[name]['correct']
        results[name]['accuracy'] = 100.0 * correct / total if total > 0 else 0.0

    if verbose and payload_truth is not None:
        print(f"\n{'Decoder':<12} {'Correct':>8} {'Total':>8} {'Accuracy':>10}")
        print("-" * 42)
        for name in decoders:
            r = results[name]
            print(f"{name:<12} {r['correct']:>8} {r['total']:>8} "
                  f"{r['accuracy']:>9.1f}%")

    return results


def noise_sweep(pixels_lab, curve, frame, n_palette,
                constellation_map, payload_truth,
                sigmas=None, n_trials=50, verbose=True):
    """Sweep noise: geometric vs rips vs spectral."""
    from parametric_encoding import decode as geometric_decode

    if sigmas is None:
        sigmas = [0.0, 0.5, 1.0, 2.0, 3.0, 5.0, 8.0, 10.0, 15.0, 20.0]

    truth = list(payload_truth)
    n = len(truth)
    results = []

    for sigma in sigmas:
        accs = {'geometric': [], 'rips': [], 'spectral': []}

        for trial in range(n_trials):
            rng = np.random.RandomState(trial)
            if sigma > 0:
                noisy = pixels_lab + rng.normal(0, sigma, pixels_lab.shape)
                noisy[:, 0] = np.clip(noisy[:, 0], 0, 100)
            else:
                noisy = pixels_lab.copy()

            for name, decode_fn in [
                ('geometric', lambda px: geometric_decode(
                    px, curve, frame, n_palette,
                    constellation_map=constellation_map)),
                ('rips', lambda px: rips_decode(
                    px, curve, frame, n_palette,
                    constellation_map=constellation_map)),
                ('spectral', lambda px: spectral_decode(
                    px, curve, frame, n_palette,
                    constellation_map=constellation_map)),
            ]:
                words = decode_fn(noisy)
                correct = sum(1 for a, b in zip(truth, words) if a == b)
                accs[name].append(100.0 * correct / n)

        row = {'sigma': sigma}
        for name in ['geometric', 'rips', 'spectral']:
            row[f'{name}_mean'] = np.mean(accs[name])
            row[f'{name}_p5'] = np.percentile(accs[name], 5)
        results.append(row)

    if verbose:
        print(f"\n{'sigma':>6}  "
              f"{'Geo':>8} {'Rips':>8} {'Spec':>8}  "
              f"{'Winner':>10}")
        print("-" * 50)
        for r in results:
            means = {
                'geometric': r['geometric_mean'],
                'rips': r['rips_mean'],
                'spectral': r['spectral_mean'],
            }
            winner = max(means, key=means.get)
            print(f"{r['sigma']:>6.1f}  "
                  f"{r['geometric_mean']:>7.1f}% "
                  f"{r['rips_mean']:>7.1f}% "
                  f"{r['spectral_mean']:>7.1f}%  "
                  f"{winner:>10}")

    return results


# =========================================================================
# CLI
# =========================================================================

if __name__ == '__main__':
    import sys
    import os
    sys.path.insert(0, os.path.dirname(__file__))
    from parametric_encoding import build_encoder, encode

    yaml_path = os.path.join(os.path.dirname(__file__), 'palette.yaml')

    print("=" * 60)
    print("Topological Decoder: Rips Filtration + Spectral Comparison")
    print("=" * 60)

    n_palette = 16
    enc = build_encoder(yaml_path, 'viridis_approx', n_palette=n_palette)
    curve = enc['curve']
    frame = enc['frame']
    cmap = enc['constellation_map']

    rng = np.random.RandomState(42)
    payload = rng.randint(0, n_palette, size=40).tolist()
    print(f"\nPayload ({len(payload)} words): {payload}")

    pixels_lab, meta = encode(payload, curve, frame, n_palette,
                               constellation_map=cmap)
    print(f"Encoded: {len(pixels_lab)} colors")
    print(f"Palette spacing: {curve.arc_length / (n_palette - 1):.1f} CIELAB (arc-length)")
    print(f"Constellation eps: {EPSILON}")

    # --- Rips diagnostics ---
    print("\n--- Rips filtration diagnostics ---")
    rips_words, rips_diag = rips_decode(
        pixels_lab, curve, frame, n_palette,
        constellation_map=cmap, return_diagnostics=True)

    print(f"  Palette spacing (1D): {rips_diag['palette_spacing']:.2f}")
    print(f"  Components detected:  {rips_diag['k_actual']}")
    print(f"  Cut level:            {rips_diag['cut_level']:.3f}")
    print(f"  Persistence gap:      {rips_diag['persistence_gap']:.3f}")
    print(f"  Assignment cost:      {rips_diag['assignment_cost']:.2f}")
    print(f"  Cluster sizes:        {dict(rips_diag['cluster_sizes'])}")

    deaths = rips_diag['deaths_sorted']
    if len(deaths) > 0:
        n_show = min(20, len(deaths))
        print(f"  Death times (first {n_show}): "
              + ", ".join(f"{d:.3f}" for d in deaths[:n_show]))

    # --- Spectral diagnostics ---
    print("\n--- Spectral diagnostics ---")
    spec_words, spec_diag = spectral_decode(
        pixels_lab, curve, frame, n_palette,
        constellation_map=cmap, return_diagnostics=True)
    print(f"  Clusters (eigengap):  {spec_diag['k_used']}")
    print(f"  Eigengap:             {spec_diag['eigengap']:.4f}")
    print(f"  Sigma kernel:         {spec_diag['sigma_kernel']:.2f}")
    evals = spec_diag['eigenvalues']
    n_show = min(spec_diag['k_used'] + 3, len(evals))
    print(f"  Eigenvalues:          "
          + ", ".join(f"{e:.4f}" for e in evals[:n_show]) + "...")

    # --- Clean round-trip ---
    print("\n--- Clean round-trip ---")
    results = compare_decoders(
        pixels_lab, curve, frame, n_palette,
        constellation_map=cmap, payload_truth=payload)

    for name in ['geometric', 'rips', 'spectral']:
        ok = results[name]['words'] == payload
        print(f"  {name}: {'PASS' if ok else 'FAIL'}")
        if not ok:
            mismatches = sum(1 for a, b in zip(payload, results[name]['words'])
                             if a != b)
            print(f"    ({mismatches} mismatches)")

    # --- Noise sweep ---
    print("\n--- Noise sweep (50 trials per sigma) ---")
    noise_sweep(
        pixels_lab, curve, frame, n_palette, cmap, payload,
        sigmas=[0.0, 0.5, 1.0, 2.0, 3.0, 5.0, 8.0],
        n_trials=50)
