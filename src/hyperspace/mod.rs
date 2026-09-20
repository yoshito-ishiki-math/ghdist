pub(crate) mod generic;
mod search;
pub(crate) mod ultrametric;
pub(crate) mod z3;

use crate::metric::Metric;
pub(crate) use search::minimum_hyperspace_distortion_lazy;
use std::collections::{BTreeMap, HashMap};

pub(crate) fn hyperspace_metric(base: &Metric, max_points: usize) -> Result<Metric, String> {
    let subset_count = 1_usize
        .checked_shl(base.order as u32)
        .ok_or_else(|| "base order is too large for subset masks".to_string())?;
    let point_count = subset_count - 1;
    if point_count > max_points {
        return Err(format!(
            "Exp(X) has {point_count} points; safety limit is {max_points} \
             (override with --max-hyperspace-points)"
        ));
    }
    let entry_count = point_count
        .checked_mul(point_count)
        .ok_or_else(|| "hyperspace distance-matrix size overflowed usize".to_string())?;

    // nearest[i, mask] = d(i, mask), built in O(n 2^n).
    let mut nearest = vec![u32::MAX; base.order * subset_count];
    for point in 0..base.order {
        crate::runtime::check()?;
        for mask in 1..subset_count {
            let singleton = mask.trailing_zeros() as usize;
            let remainder = mask & (mask - 1);
            let edge = base.get(point, singleton);
            nearest[point * subset_count + mask] = if remainder == 0 {
                edge
            } else {
                edge.min(nearest[point * subset_count + remainder])
            };
        }
    }

    let directed = |source: usize, target: usize| -> u32 {
        let mut remaining = source;
        let mut maximum = 0;
        while remaining != 0 {
            let point = remaining.trailing_zeros() as usize;
            maximum = maximum.max(nearest[point * subset_count + target]);
            remaining &= remaining - 1;
        }
        maximum
    };

    let mut entries = vec![0; entry_count];
    for left_mask in 1..subset_count {
        crate::runtime::check()?;
        let left_index = left_mask - 1;
        for right_mask in (left_mask + 1)..subset_count {
            let right_index = right_mask - 1;
            let value = directed(left_mask, right_mask).max(directed(right_mask, left_mask));
            entries[left_index * point_count + right_index] = value;
            entries[right_index * point_count + left_index] = value;
        }
    }
    Ok(Metric {
        order: point_count,
        entries,
    })
}

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct DilationStats {
    pub(crate) distance_queries: u64,
    pub(crate) distance_cache_hits: u64,
    pub(crate) dilation_tests: u64,
}

pub(crate) struct HausdorffOracle<'a> {
    pub(crate) base: &'a Metric,
    pub(crate) levels: Vec<u32>,
    pub(crate) balls: Vec<Vec<u64>>,
    pub(crate) distance_cache: HashMap<u64, u32>,
    pub(crate) max_cache_entries: usize,
    pub(crate) stats: DilationStats,
}

impl<'a> HausdorffOracle<'a> {
    pub(crate) fn new(base: &'a Metric, max_cache_entries: usize) -> Result<Self, String> {
        if base.order > 20 {
            return Err(format!(
                "lazy hyperspace masks support base order at most 20, received {}",
                base.order
            ));
        }
        let levels: Vec<u32> = base.distance_levels().into_iter().collect();
        let mut balls = Vec::with_capacity(levels.len());
        for level in &levels {
            let mut point_balls = vec![0_u64; base.order];
            for (center, ball) in point_balls.iter_mut().enumerate() {
                for point in 0..base.order {
                    if base.get(center, point) <= *level {
                        *ball |= 1_u64 << point;
                    }
                }
            }
            balls.push(point_balls);
        }
        Ok(Self {
            base,
            levels,
            balls,
            distance_cache: HashMap::new(),
            max_cache_entries,
            stats: DilationStats::default(),
        })
    }

    pub(crate) fn point_count(&self) -> usize {
        (1_usize << self.base.order) - 1
    }

    pub(crate) fn pair_key(&self, left: u64, right: u64) -> u64 {
        let (small, large) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };
        (small << self.base.order) | large
    }

    pub(crate) fn dilation(&mut self, subset: u64, level_index: usize) -> u64 {
        let mut remaining = subset;
        let mut result = 0_u64;
        while remaining != 0 {
            let point = remaining.trailing_zeros() as usize;
            result |= self.balls[level_index][point];
            remaining &= remaining - 1;
        }
        result
    }

    pub(crate) fn distance_leq_at_level(
        &mut self,
        left: u64,
        right: u64,
        level_index: usize,
    ) -> bool {
        self.stats.dilation_tests += 1;
        let right_dilation = self.dilation(right, level_index);
        if left & !right_dilation != 0 {
            return false;
        }
        let left_dilation = self.dilation(left, level_index);
        right & !left_dilation == 0
    }

    pub(crate) fn distance(&mut self, left: u64, right: u64) -> u32 {
        self.stats.distance_queries += 1;
        if left == right {
            return 0;
        }
        let key = self.pair_key(left, right);
        if let Some(value) = self.distance_cache.get(&key) {
            self.stats.distance_cache_hits += 1;
            return *value;
        }
        let mut lower = 0;
        let mut upper = self.levels.len() - 1;
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            if self.distance_leq_at_level(left, right, middle) {
                upper = middle;
            } else {
                lower = middle + 1;
            }
        }
        let result = self.levels[lower];
        if self.distance_cache.len() < self.max_cache_entries {
            self.distance_cache.insert(key, result);
        }
        result
    }

    pub(crate) fn subset_eccentricities(&self) -> Vec<u32> {
        let subset_count = 1_usize << self.base.order;
        let point_eccentricities: Vec<u32> = (0..self.base.order)
            .map(|point| {
                (0..self.base.order)
                    .map(|other| self.base.get(point, other))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let mut result = vec![0; subset_count];
        for mask in 1..subset_count {
            let point = mask.trailing_zeros() as usize;
            let remainder = mask & (mask - 1);
            result[mask] = result[remainder].max(point_eccentricities[point]);
        }
        result
    }

    pub(crate) fn partition_by_open_support(
        &self,
        points: &[u64],
        threshold: u32,
    ) -> Vec<Vec<u64>> {
        let labels = self.base.open_component_labels(threshold);
        let mut groups: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for subset in points {
            let mut remaining = *subset;
            let mut support = 0_u64;
            while remaining != 0 {
                let point = remaining.trailing_zeros() as usize;
                support |= 1_u64 << labels[point];
                remaining &= remaining - 1;
            }
            groups.entry(support).or_default().push(*subset);
        }
        groups.into_values().collect()
    }
}
