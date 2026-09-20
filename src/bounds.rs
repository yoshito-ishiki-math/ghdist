use crate::metric::Metric;
use std::collections::{BTreeSet, HashMap};

pub(crate) fn candidate_thresholds(left: &Metric, right: &Metric) -> Vec<u32> {
    let left_levels = left.distance_levels();
    let right_levels = right.distance_levels();
    let mut thresholds = BTreeSet::new();
    for left_value in &left_levels {
        for right_value in &right_levels {
            thresholds.insert(left_value.abs_diff(*right_value));
        }
    }
    thresholds.into_iter().collect()
}

pub(crate) fn labelled_correspondence_upper_bound(left: &Metric, right: &Metric) -> u32 {
    let mut relation = Vec::with_capacity(left.order + right.order);
    for i in 0..left.order {
        relation.push((i, i % right.order));
    }
    for j in 0..right.order {
        relation.push((j % left.order, j));
    }
    relation.sort_unstable();
    relation.dedup();
    relation
        .iter()
        .flat_map(|first| relation.iter().map(move |second| (*first, *second)))
        .map(|((i, j), (k, ell))| left.get(i, k).abs_diff(right.get(j, ell)))
        .max()
        .unwrap_or(0)
}

pub(crate) fn structural_lower_bound(left: &Metric, right: &Metric) -> u32 {
    let target_len = left.order.max(right.order) - 1;
    let mut left_spectrum = vec![0; target_len - (left.order - 1)];
    left_spectrum.extend(left.mst_merge_spectrum());
    let mut right_spectrum = vec![0; target_len - (right.order - 1)];
    right_spectrum.extend(right.mst_merge_spectrum());
    let merge_bound = left_spectrum
        .iter()
        .zip(&right_spectrum)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    merge_bound
        .max(left.diameter().abs_diff(right.diameter()))
        .max(left.radius().abs_diff(right.radius()))
}

#[derive(Default, Debug)]
pub(crate) struct StructuralStats {
    pub(crate) thresholds_checked: u64,
    pub(crate) lower_bound_rejections: u64,
    pub(crate) cardinality_rejections: u64,
    pub(crate) component_count_rejections: u64,
    pub(crate) component_dominance_rejections: u64,
    pub(crate) subset_product_vectors_built: u64,
}

pub(crate) struct StructuralFilter<'a> {
    pub(crate) left: &'a Metric,
    pub(crate) right: &'a Metric,
    pub(crate) lower_bound: u32,
    pub(crate) subset_products: HashMap<Vec<usize>, Vec<u64>>,
    pub(crate) stats: StructuralStats,
}

impl<'a> StructuralFilter<'a> {
    pub(crate) fn new(left: &'a Metric, right: &'a Metric) -> Self {
        Self {
            left,
            right,
            lower_bound: structural_lower_bound(left, right),
            subset_products: HashMap::new(),
            stats: StructuralStats::default(),
        }
    }

    pub(crate) fn products_for(&mut self, component_sizes: &[usize]) -> Vec<u64> {
        if let Some(products) = self.subset_products.get(component_sizes) {
            return products.clone();
        }
        let mut products = vec![1_u64];
        for size in component_sizes {
            let weight = (1_u64 << size) - 1;
            let additions: Vec<u64> = products.iter().map(|value| value * weight).collect();
            products.extend(additions);
        }
        products.sort_unstable();
        let empty_position = products
            .iter()
            .position(|value| *value == 1)
            .expect("the empty support contributes one");
        products.remove(empty_position);
        self.stats.subset_product_vectors_built += 1;
        self.subset_products
            .insert(component_sizes.to_vec(), products.clone());
        products
    }

    pub(crate) fn dominance_holds(&mut self, source: &[usize], target: &[usize]) -> bool {
        if source == target {
            return true;
        }
        let source_products = self.products_for(source);
        let target_products = self.products_for(target);
        source_products.len() == target_products.len()
            && source_products
                .iter()
                .zip(target_products)
                .all(|(source_value, target_value)| *source_value >= target_value)
    }

    pub(crate) fn accepts(&mut self, epsilon: u32) -> bool {
        self.stats.thresholds_checked += 1;
        if epsilon < self.lower_bound {
            self.stats.lower_bound_rejections += 1;
            return false;
        }
        if (epsilon < self.right.minimum_positive_distance() && self.left.order < self.right.order)
            || (epsilon < self.left.minimum_positive_distance()
                && self.right.order < self.left.order)
        {
            self.stats.cardinality_rejections += 1;
            return false;
        }

        let epsilon64 = u64::from(epsilon);
        let mut events = BTreeSet::from([0_u64]);
        for distance in self
            .left
            .distance_levels()
            .into_iter()
            .chain(self.right.distance_levels())
        {
            let distance64 = u64::from(distance);
            events.insert(distance64);
            if distance64 >= epsilon64 {
                events.insert(distance64 - epsilon64);
            }
        }

        for scale in events {
            let left_now = self.left.component_sizes_leq(scale);
            let right_now = self.right.component_sizes_leq(scale);
            let shifted = scale + epsilon64;
            let left_shifted = self.left.component_sizes_leq(shifted);
            let right_shifted = self.right.component_sizes_leq(shifted);

            if right_shifted.len() > left_now.len() || left_shifted.len() > right_now.len() {
                self.stats.component_count_rejections += 1;
                return false;
            }

            if left_now.len() == right_shifted.len()
                && epsilon < self.right.minimum_positive_distance()
                && !self.dominance_holds(&left_now, &right_shifted)
            {
                self.stats.component_dominance_rejections += 1;
                return false;
            }
            if right_now.len() == left_shifted.len()
                && epsilon < self.left.minimum_positive_distance()
                && !self.dominance_holds(&right_now, &left_shifted)
            {
                self.stats.component_dominance_rejections += 1;
                return false;
            }
        }
        true
    }
}
