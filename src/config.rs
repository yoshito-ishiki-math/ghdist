pub(crate) const KNOWN_LEFT: [u32; 6] = [215, 886, 318, 912, 396, 1191];
pub(crate) const KNOWN_RIGHT: [u32; 6] = [210, 593, 506, 787, 707, 92];
pub(crate) const DEFAULT_MAX_HYPERSPACE_POINTS: usize = 127;
pub(crate) const DEFAULT_MAX_RELATION_VERTICES: usize = 20_000;
pub(crate) const DEFAULT_MAX_LAZY_DEPTH: usize = 4_095;
pub(crate) const DEFAULT_MAX_SEARCH_NODES: u64 = 2_000_000;
pub(crate) const DEFAULT_MAX_CANDIDATE_SCANS: u64 = 100_000_000;
pub(crate) const DEFAULT_MAX_DISTANCE_CACHE: usize = 4_000_000;
pub(crate) const DEFAULT_MRV_WINDOW: usize = 8;
pub(crate) const DEFAULT_MAX_ULTRAMETRIC_BLOCKS: usize = 24;
pub(crate) const DEFAULT_MAX_ULTRAMETRIC_ASSIGNMENTS: u64 = 5_000_000;
pub(crate) const DEFAULT_MAX_Z3_VARIABLES: usize = 5_000;
pub(crate) const DEFAULT_MAX_Z3_CLAUSES: u64 = 10_000_000;
pub(crate) const DEFAULT_Z3_TIMEOUT_SECONDS: u64 = 30;
pub(crate) const AUTO_Z3_MIN_RELATION_VERTICES: usize = 226;
pub(crate) const AUTO_Z3_RELATION_VERTICES: usize = 1_500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LazyStrategy {
    Auto,
    Generic,
    Ultrametric,
    Z3,
}

#[derive(Clone, Debug)]
pub(crate) struct LazyConfig {
    pub(crate) max_depth: usize,
    pub(crate) max_search_nodes: u64,
    pub(crate) max_candidate_scans: u64,
    pub(crate) max_distance_cache: usize,
    pub(crate) mrv_window: usize,
    pub(crate) max_ultrametric_blocks: usize,
    pub(crate) max_ultrametric_assignments: u64,
    pub(crate) max_z3_variables: usize,
    pub(crate) max_z3_clauses: u64,
    pub(crate) z3_timeout_seconds: u64,
}

impl Default for LazyConfig {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_LAZY_DEPTH,
            max_search_nodes: DEFAULT_MAX_SEARCH_NODES,
            max_candidate_scans: DEFAULT_MAX_CANDIDATE_SCANS,
            max_distance_cache: DEFAULT_MAX_DISTANCE_CACHE,
            mrv_window: DEFAULT_MRV_WINDOW,
            max_ultrametric_blocks: DEFAULT_MAX_ULTRAMETRIC_BLOCKS,
            max_ultrametric_assignments: DEFAULT_MAX_ULTRAMETRIC_ASSIGNMENTS,
            max_z3_variables: DEFAULT_MAX_Z3_VARIABLES,
            max_z3_clauses: DEFAULT_MAX_Z3_CLAUSES,
            z3_timeout_seconds: DEFAULT_Z3_TIMEOUT_SECONDS,
        }
    }
}
