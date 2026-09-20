use super::HausdorffOracle;
use crate::bitset::BitSet;
use crate::config::LazyConfig;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RelationPair {
    pub(crate) left: u64,
    pub(crate) right: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum UncoveredPoint {
    Left(u64),
    Right(u64),
}

#[derive(Default, Debug)]
pub(crate) struct LazyFeasibilityStats {
    pub(crate) search_nodes: u64,
    pub(crate) candidate_scans: u64,
    pub(crate) compatibility_checks: u64,
    pub(crate) maximum_depth: usize,
    pub(crate) memo_hits: u64,
}

pub(crate) fn hyperspace_point_order(eccentricities: &[u32]) -> Vec<u64> {
    let mut frequencies = HashMap::new();
    for value in eccentricities.iter().skip(1) {
        *frequencies.entry(*value).or_insert(0_usize) += 1;
    }
    let mut points: Vec<u64> = (1..eccentricities.len() as u64).collect();
    points.sort_unstable_by_key(|mask| {
        let eccentricity = eccentricities[*mask as usize];
        (
            frequencies[&eccentricity],
            u32::MAX - eccentricity,
            mask.count_ones(),
            *mask,
        )
    });
    points
}

pub(crate) struct LazyCorrespondenceSolver<'oracle, 'metric> {
    pub(crate) left: &'oracle mut HausdorffOracle<'metric>,
    pub(crate) right: &'oracle mut HausdorffOracle<'metric>,
    pub(crate) epsilon: u32,
    pub(crate) config: &'oracle LazyConfig,
    pub(crate) left_eccentricities: &'oracle [u32],
    pub(crate) right_eccentricities: &'oracle [u32],
    pub(crate) left_order: &'oracle [u64],
    pub(crate) right_order: &'oracle [u64],
    pub(crate) covered_left: BitSet,
    pub(crate) covered_right: BitSet,
    pub(crate) covered_left_count: usize,
    pub(crate) covered_right_count: usize,
    star: StarForest,
    left_functional: bool,
    right_functional: bool,
    pub(crate) selected: Vec<RelationPair>,
    pub(crate) failed: HashSet<Vec<RelationPair>>,
    pub(crate) stats: LazyFeasibilityStats,
}

impl<'oracle, 'metric> LazyCorrespondenceSolver<'oracle, 'metric> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        left: &'oracle mut HausdorffOracle<'metric>,
        right: &'oracle mut HausdorffOracle<'metric>,
        epsilon: u32,
        config: &'oracle LazyConfig,
        left_eccentricities: &'oracle [u32],
        right_eccentricities: &'oracle [u32],
        left_order: &'oracle [u64],
        right_order: &'oracle [u64],
    ) -> Result<Self, String> {
        let left_count = left.point_count();
        let right_count = right.point_count();
        if left_count.max(right_count) > config.max_depth {
            return Err(format!(
                "generic lazy search needs at least {} covering edges, above the depth \
                 limit {} (use ultrametric decomposition or raise --max-lazy-depth)",
                left_count.max(right_count),
                config.max_depth
            ));
        }
        let left_functional = epsilon < right.base.minimum_positive_distance();
        let right_functional = epsilon < left.base.minimum_positive_distance();
        let bijection_forced = left_count == right_count && (left_functional || right_functional);
        Ok(Self {
            left,
            right,
            epsilon,
            config,
            left_eccentricities,
            right_eccentricities,
            left_order,
            right_order,
            covered_left: BitSet::empty(left_count),
            covered_right: BitSet::empty(right_count),
            covered_left_count: 0,
            covered_right_count: 0,
            star: StarForest::new(left_count, right_count),
            left_functional: left_functional || bijection_forced,
            right_functional: right_functional || bijection_forced,
            selected: Vec::with_capacity(left_count.max(right_count)),
            failed: HashSet::new(),
            stats: LazyFeasibilityStats::default(),
        })
    }

    pub(crate) fn solve(self) -> Result<(bool, LazyFeasibilityStats), String> {
        let (feasible, stats, _) = self.solve_with_witness()?;
        Ok((feasible, stats))
    }
    pub(crate) fn solve_with_witness(
        mut self,
    ) -> Result<(bool, LazyFeasibilityStats, Vec<RelationPair>), String> {
        if self.epsilon < self.right.base.minimum_positive_distance()
            && self.left.point_count() < self.right.point_count()
        {
            return Ok((false, self.stats, Vec::new()));
        }
        if self.epsilon < self.left.base.minimum_positive_distance()
            && self.right.point_count() < self.left.point_count()
        {
            return Ok((false, self.stats, Vec::new()));
        }
        let result = self.search()?;
        Ok((result, self.stats, self.selected))
    }

    pub(crate) fn compatible_with_selected(&mut self, candidate: RelationPair) -> bool {
        for index in 0..self.selected.len() {
            let selected = self.selected[index];
            self.stats.compatibility_checks += 1;
            let left_distance = self.left.distance(candidate.left, selected.left);
            let right_distance = self.right.distance(candidate.right, selected.right);
            if left_distance.abs_diff(right_distance) > self.epsilon {
                return false;
            }
        }
        true
    }

    pub(crate) fn point_pair_admissible(&self, left: u64, right: u64) -> bool {
        let left_index = left as usize - 1;
        let right_index = right as usize - 1;
        if self.left_functional && self.covered_left.contains(left_index) {
            return false;
        }
        if self.right_functional && self.covered_right.contains(right_index) {
            return false;
        }
        self.left_eccentricities[left as usize].abs_diff(self.right_eccentricities[right as usize])
            <= self.epsilon
    }

    pub(crate) fn candidate_options(
        &mut self,
        point: UncoveredPoint,
        cutoff: Option<usize>,
    ) -> Result<(Vec<RelationPair>, bool), String> {
        let mut options = Vec::new();
        match point {
            UncoveredPoint::Left(left) => {
                for right in 1..=self.right.point_count() as u64 {
                    self.stats.candidate_scans += 1;
                    if self.stats.candidate_scans.is_multiple_of(1024) {
                        crate::runtime::check()?;
                    }
                    if self.stats.candidate_scans > self.config.max_candidate_scans {
                        return Err(format!(
                            "lazy candidate-scan limit {} reached at node {} and depth {}",
                            self.config.max_candidate_scans,
                            self.stats.search_nodes,
                            self.selected.len(),
                        ));
                    }
                    let pair = RelationPair { left, right };
                    if self.point_pair_admissible(left, right)
                        && self.star.allows(left as usize - 1, right as usize - 1)
                        && self.compatible_with_selected(pair)
                    {
                        options.push(pair);
                        if cutoff.is_some_and(|limit| options.len() > limit) {
                            return Ok((Vec::new(), true));
                        }
                    }
                }
                options.sort_unstable_by_key(|pair| {
                    (
                        usize::from(self.covered_right.contains(pair.right as usize - 1)),
                        usize::from(pair.left != pair.right),
                        pair.left.abs_diff(pair.right),
                        pair.right,
                    )
                });
            }
            UncoveredPoint::Right(right) => {
                for left in 1..=self.left.point_count() as u64 {
                    self.stats.candidate_scans += 1;
                    if self.stats.candidate_scans.is_multiple_of(1024) {
                        crate::runtime::check()?;
                    }
                    if self.stats.candidate_scans > self.config.max_candidate_scans {
                        return Err(format!(
                            "lazy candidate-scan limit {} reached at node {} and depth {}",
                            self.config.max_candidate_scans,
                            self.stats.search_nodes,
                            self.selected.len(),
                        ));
                    }
                    let pair = RelationPair { left, right };
                    if self.point_pair_admissible(left, right)
                        && self.star.allows(left as usize - 1, right as usize - 1)
                        && self.compatible_with_selected(pair)
                    {
                        options.push(pair);
                        if cutoff.is_some_and(|limit| options.len() > limit) {
                            return Ok((Vec::new(), true));
                        }
                    }
                }
                options.sort_unstable_by_key(|pair| {
                    (
                        usize::from(self.covered_left.contains(pair.left as usize - 1)),
                        usize::from(pair.left != pair.right),
                        pair.left.abs_diff(pair.right),
                        pair.left,
                    )
                });
            }
        }
        Ok((options, false))
    }

    pub(crate) fn frontier(&self) -> Vec<UncoveredPoint> {
        let window = self.config.mrv_window.max(1);
        let mut result = Vec::with_capacity(2 * window);
        for point in self.left_order {
            if !self.covered_left.contains(*point as usize - 1) {
                result.push(UncoveredPoint::Left(*point));
                if result.len() == window {
                    break;
                }
            }
        }
        let left_entries = result.len();
        for point in self.right_order {
            if !self.covered_right.contains(*point as usize - 1) {
                result.push(UncoveredPoint::Right(*point));
                if result.len() == left_entries + window {
                    break;
                }
            }
        }
        result
    }

    pub(crate) fn best_options(&mut self) -> Result<Vec<RelationPair>, String> {
        let mut best: Option<Vec<RelationPair>> = None;
        for point in self.frontier() {
            let cutoff = best.as_ref().map(Vec::len);
            let (options, truncated) = self.candidate_options(point, cutoff)?;
            if truncated {
                continue;
            }
            if options.is_empty() {
                return Ok(options);
            }
            if best
                .as_ref()
                .is_none_or(|current| options.len() < current.len())
            {
                best = Some(options);
            }
        }
        best.ok_or_else(|| "no uncovered hyperspace point was found".to_string())
    }

    pub(crate) fn search(&mut self) -> Result<bool, String> {
        crate::runtime::check()?;
        self.stats.search_nodes += 1;
        if self.stats.search_nodes > self.config.max_search_nodes {
            return Err(format!(
                "lazy search-node limit {} reached",
                self.config.max_search_nodes
            ));
        }
        self.stats.maximum_depth = self.stats.maximum_depth.max(self.selected.len());
        if self.covered_left_count == self.left.point_count()
            && self.covered_right_count == self.right.point_count()
        {
            return Ok(true);
        }
        if self.selected.len() >= self.config.max_depth {
            return Err(format!(
                "lazy correspondence depth limit {} reached",
                self.config.max_depth
            ));
        }
        let mut memo_key = self.selected.clone();
        memo_key.sort_unstable();
        if self.failed.contains(&memo_key) {
            self.stats.memo_hits += 1;
            return Ok(false);
        }

        let options = self.best_options()?;
        for pair in options {
            let left_index = pair.left as usize - 1;
            let right_index = pair.right as usize - 1;
            let new_left = !self.covered_left.contains(left_index);
            let new_right = !self.covered_right.contains(right_index);
            debug_assert!(new_left || new_right);
            if new_left {
                self.covered_left.set(left_index);
                self.covered_left_count += 1;
            }
            if new_right {
                self.covered_right.set(right_index);
                self.covered_right_count += 1;
            }
            self.star.push(left_index, right_index);
            self.selected.push(pair);
            if self.search()? {
                return Ok(true);
            }
            self.star.pop(left_index, right_index);
            self.selected.pop();
            if new_left {
                self.covered_left.clear(left_index);
                self.covered_left_count -= 1;
            }
            if new_right {
                self.covered_right.clear(right_index);
                self.covered_right_count -= 1;
            }
        }
        if self.failed.len() < 250_000 {
            self.failed.insert(memo_key);
        }
        Ok(false)
    }
}

/// Tracks a star forest under LIFO insertion/removal. Every feasible
/// correspondence contains an inclusion-minimal correspondence, and every
/// edge in such a graph has a leaf endpoint. Searching these forests is complete.
struct StarForest {
    left_degree: Vec<usize>,
    right_degree: Vec<usize>,
    left_neighbor: Vec<usize>,
    right_neighbor: Vec<usize>,
}

impl StarForest {
    fn new(left: usize, right: usize) -> Self {
        Self {
            left_degree: vec![0; left],
            right_degree: vec![0; right],
            left_neighbor: vec![usize::MAX; left],
            right_neighbor: vec![usize::MAX; right],
        }
    }

    fn allows(&self, left: usize, right: usize) -> bool {
        let ld = self.left_degree[left];
        let rd = self.right_degree[right];
        if ld > 0 && rd > 0 {
            return false;
        }
        // An existing leaf may become a center only if its old neighbor is
        // also a leaf. Other components are unchanged by this insertion.
        if ld == 1 && self.right_degree[self.left_neighbor[left]] > 1 {
            return false;
        }
        if rd == 1 && self.left_degree[self.right_neighbor[right]] > 1 {
            return false;
        }
        true
    }

    fn push(&mut self, left: usize, right: usize) {
        debug_assert!(self.allows(left, right));
        if self.left_degree[left] == 0 {
            self.left_neighbor[left] = right;
        }
        if self.right_degree[right] == 0 {
            self.right_neighbor[right] = left;
        }
        self.left_degree[left] += 1;
        self.right_degree[right] += 1;
    }

    fn pop(&mut self, left: usize, right: usize) {
        self.left_degree[left] -= 1;
        self.right_degree[right] -= 1;
        // The first neighbor remains valid while the degree is positive:
        // removals occur in the reverse order of insertions.
        if self.left_degree[left] == 0 {
            self.left_neighbor[left] = usize::MAX;
        }
        if self.right_degree[right] == 0 {
            self.right_neighbor[right] = usize::MAX;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StarForest;

    fn is_star_forest(edges: &[(usize, usize)]) -> bool {
        let mut left = [0; 3];
        let mut right = [0; 3];
        for &(a, b) in edges {
            left[a] += 1;
            right[b] += 1;
        }
        edges.iter().all(|&(a, b)| left[a] == 1 || right[b] == 1)
    }

    #[test]
    fn constant_time_star_check_matches_all_three_by_three_graphs_and_rollbacks() {
        for mask in 0_u16..512 {
            let mut edges: Vec<_> = (0..9)
                .filter(|bit| mask & (1 << bit) != 0)
                .map(|bit| (bit / 3, bit % 3))
                .collect();
            if !is_star_forest(&edges) {
                continue;
            }
            // Reverse insertion order also exercises which neighbor is kept.
            for reverse in [false, true] {
                if reverse {
                    edges.reverse();
                }
                let mut forest = StarForest::new(3, 3);
                for &(a, b) in &edges {
                    forest.push(a, b);
                }
                for kept in (0..=edges.len()).rev() {
                    for a in 0..3 {
                        for b in 0..3 {
                            let mut extended = edges[..kept].to_vec();
                            extended.push((a, b));
                            assert_eq!(
                                forest.allows(a, b),
                                is_star_forest(&extended),
                                "mask={mask}, kept={kept}, candidate=({a},{b})"
                            );
                        }
                    }
                    if kept > 0 {
                        let (a, b) = edges[kept - 1];
                        forest.pop(a, b);
                    }
                }
            }
        }
    }
}
