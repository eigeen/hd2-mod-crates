use super::*;

pub(super) fn assignment_order(
    rankings: &BTreeMap<u64, Vec<(u64, f64)>>,
    patch_unit_ids: &BTreeSet<u64>,
) -> Vec<u64> {
    let mut ids: Vec<u64> = patch_unit_ids
        .iter()
        .copied()
        .filter(|id| rankings.contains_key(id))
        .collect();
    ids.sort_by(|a, b| {
        let sa = rankings[a].first().map(|x| x.1).unwrap_or(f64::INFINITY);
        let sb = rankings[b].first().map(|x| x.1).unwrap_or(f64::INFINITY);
        sa.partial_cmp(&sb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b))
    });
    ids
}

pub(super) fn optimal_variant_assignments(
    rankings: &BTreeMap<u64, Vec<(u64, f64)>>,
    patch_unit_ids: &BTreeSet<u64>,
    taken_targets: &HashSet<u64>,
    settings: &GeometryMatchSettings,
    trusted_source_ids: &HashSet<u64>,
) -> HashMap<u64, u64> {
    let source_ids = assignable_source_ids(rankings, patch_unit_ids);
    let candidates = assignment_candidates(
        rankings,
        &source_ids,
        taken_targets,
        settings,
        trusted_source_ids,
    );
    if source_ids.iter().any(|sid| candidates[sid].is_empty()) {
        return HashMap::new();
    }
    let mut target_set: BTreeSet<u64> = BTreeSet::new();
    for values in candidates.values() {
        for (tid, _) in values {
            target_set.insert(*tid);
        }
    }
    let target_ids: Vec<u64> = target_set.into_iter().collect();
    solve_assignment(&source_ids, &target_ids, &candidates)
}

fn assignable_source_ids(
    rankings: &BTreeMap<u64, Vec<(u64, f64)>>,
    patch_unit_ids: &BTreeSet<u64>,
) -> Vec<u64> {
    let mut ids: Vec<u64> = patch_unit_ids
        .iter()
        .copied()
        .filter(|id| rankings.contains_key(id))
        .collect();
    ids.sort_by(|a, b| rankings[a].len().cmp(&rankings[b].len()).then(a.cmp(b)));
    ids
}

fn assignment_candidates(
    rankings: &BTreeMap<u64, Vec<(u64, f64)>>,
    source_ids: &[u64],
    taken_targets: &HashSet<u64>,
    settings: &GeometryMatchSettings,
    trusted_source_ids: &HashSet<u64>,
) -> HashMap<u64, Vec<(u64, f64)>> {
    let mut out = HashMap::new();
    for &sid in source_ids {
        let trusted = trusted_source_ids.contains(&sid);
        let vals: Vec<(u64, f64)> = rankings[&sid]
            .iter()
            .filter(|(tid, score)| {
                !taken_targets.contains(tid) && (*score <= settings.max_score || trusted)
            })
            .copied()
            .collect();
        out.insert(sid, vals);
    }
    out
}

fn solve_assignment(
    source_ids: &[u64],
    target_ids: &[u64],
    candidates: &HashMap<u64, Vec<(u64, f64)>>,
) -> HashMap<u64, u64> {
    if source_ids.len() > 63 || target_ids.len() > 63 {
        // fall back to greedy assignment when bitmask DP would overflow
        return greedy_assignment(source_ids, candidates);
    }
    let target_index: HashMap<u64, usize> = target_ids
        .iter()
        .enumerate()
        .map(|(i, t)| (*t, i))
        .collect();

    // build per-source candidate index lists for the DP
    let cand_indexed: Vec<Vec<(usize, f64)>> = source_ids
        .iter()
        .map(|sid| {
            candidates[sid]
                .iter()
                .map(|(tid, score)| (target_index[tid], *score))
                .collect()
        })
        .collect();

    // memo: source_pos -> mask -> (best_score, best_targets)
    let n_sources = source_ids.len();
    let _n_targets = target_ids.len();
    let mut memo: HashMap<(usize, u64), (f64, Vec<usize>)> = HashMap::new();

    fn best(
        source_pos: usize,
        used_mask: u64,
        cand_indexed: &[Vec<(usize, f64)>],
        memo: &mut HashMap<(usize, u64), (f64, Vec<usize>)>,
    ) -> (f64, Vec<usize>) {
        if source_pos >= cand_indexed.len() {
            return (0.0, Vec::new());
        }
        if let Some(cached) = memo.get(&(source_pos, used_mask)) {
            return cached.clone();
        }
        let mut best_score = f64::INFINITY;
        let mut best_targets: Vec<usize> = Vec::new();
        for &(idx, score) in &cand_indexed[source_pos] {
            if used_mask & (1u64 << idx) != 0 {
                continue;
            }
            let (tail_score, tail_targets) = best(
                source_pos + 1,
                used_mask | (1u64 << idx),
                cand_indexed,
                memo,
            );
            let total = score + tail_score;
            if total < best_score {
                best_score = total;
                best_targets = std::iter::once(idx).chain(tail_targets).collect();
            }
        }
        memo.insert((source_pos, used_mask), (best_score, best_targets.clone()));
        (best_score, best_targets)
    }

    let (_score, assigned) = best(0, 0, &cand_indexed, &mut memo);
    if assigned.len() != n_sources {
        return HashMap::new();
    }
    source_ids
        .iter()
        .zip(assigned.iter())
        .map(|(sid, idx)| (*sid, target_ids[*idx]))
        .collect()
}

fn greedy_assignment(
    source_ids: &[u64],
    candidates: &HashMap<u64, Vec<(u64, f64)>>,
) -> HashMap<u64, u64> {
    let mut out = HashMap::new();
    let mut used: HashSet<u64> = HashSet::new();
    for &sid in source_ids {
        for &(tid, _) in &candidates[&sid] {
            if !used.contains(&tid) {
                used.insert(tid);
                out.insert(sid, tid);
                break;
            }
        }
    }
    out
}
