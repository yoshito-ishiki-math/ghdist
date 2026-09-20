use crate::config::*;
use crate::explicit::minimum_distortion;
use crate::hyperspace::{hyperspace_metric, minimum_hyperspace_distortion_lazy};
use crate::metric::{Metric, family_metric};
use crate::verification::run_self_test;
use std::env;
use std::process::ExitCode;
use std::time::Instant;

fn parse_edges(text: &str) -> Result<Vec<u32>, String> {
    text.split(',')
        .map(|part| {
            part.trim()
                .parse::<u32>()
                .map_err(|error| format!("invalid edge length {part:?}: {error}"))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EngineMode {
    Auto,
    Explicit,
    Lazy,
    LazyGeneric,
    Ultrametric,
    Z3,
}

fn parse_engine(value: &str) -> Result<EngineMode, String> {
    match value {
        "auto" => Ok(EngineMode::Auto),
        "explicit" => Ok(EngineMode::Explicit),
        "lazy" => Ok(EngineMode::Lazy),
        "lazy-generic" => Ok(EngineMode::LazyGeneric),
        "ultrametric" => Ok(EngineMode::Ultrametric),
        "z3" => Ok(EngineMode::Z3),
        _ => Err(format!(
            "unknown engine {value:?}; choose auto, explicit, lazy, \
             lazy-generic, ultrametric, or z3"
        )),
    }
}

#[derive(Debug)]
struct Arguments {
    self_test: bool,
    known_pair: bool,
    hyperspace: bool,
    compare_base: bool,
    left_edges: Option<Vec<u32>>,
    right_edges: Option<Vec<u32>>,
    left_family: Option<String>,
    right_family: Option<String>,
    order: Option<usize>,
    engine: EngineMode,
    max_hyperspace_points: usize,
    max_relation_vertices: usize,
    lazy_config: LazyConfig,
}

fn usage() -> &'static str {
    "Usage:\n\
  cargo run --release -- --self-test\n\
  cargo run --release -- --known-pair [--hyperspace]\n\
  cargo run --release -- --left-edges a,b,c --right-edges d,e,f [--hyperspace]\n\
  cargo run --release -- --left-family NAME --right-family NAME --order N\n\
Options:\n\
  --engine MODE               auto, explicit, lazy, lazy-generic, ultrametric, z3\n\
  --compare-base              use the exact base optimum as a lifted upper bound\n\
  --max-hyperspace-points N   explicit Exp safety limit (default 127)\n\
  --max-relation-vertices N   compatibility-search safety limit (default 20000)\n\
  --max-lazy-depth N          generic implicit-cover depth limit (default 4095)\n\
  --max-search-nodes N        generic/ultrametric recursion limit (default 2000000)\n\
  --max-candidate-scans N     implicit relation-pair scan limit (default 100000000)\n\
  --max-distance-cache N      cached lazy Hausdorff distances (default 4000000)\n\
  --mrv-window N              uncovered rows/columns inspected per side (default 8)\n\
  --max-ultrametric-blocks N  block-assignment width limit (default 24)\n\
  --max-ultrametric-assignments N  recursive block assignments (default 5000000)\n\
  --max-z3-variables N        optional Z3 variable limit (default 5000)\n\
  --max-z3-clauses N          optional Z3 clause limit (default 10000000)\n\
  --z3-timeout-seconds N      optional Z3 timeout (default 30)\n"
}

fn parse_arguments() -> Result<Arguments, String> {
    let mut result = Arguments {
        self_test: false,
        known_pair: false,
        hyperspace: false,
        compare_base: false,
        left_edges: None,
        right_edges: None,
        left_family: None,
        right_family: None,
        order: None,
        engine: EngineMode::Auto,
        max_hyperspace_points: DEFAULT_MAX_HYPERSPACE_POINTS,
        max_relation_vertices: DEFAULT_MAX_RELATION_VERTICES,
        lazy_config: LazyConfig::default(),
    };
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--self-test" => result.self_test = true,
            "--known-pair" => result.known_pair = true,
            "--hyperspace" => result.hyperspace = true,
            "--compare-base" => result.compare_base = true,
            "--left-edges" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--left-edges needs a comma-separated value".to_string())?;
                result.left_edges = Some(parse_edges(&value)?);
            }
            "--right-edges" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--right-edges needs a comma-separated value".to_string())?;
                result.right_edges = Some(parse_edges(&value)?);
            }
            "--left-family" => {
                result.left_family = Some(
                    arguments
                        .next()
                        .ok_or_else(|| "--left-family needs a family name".to_string())?,
                );
            }
            "--right-family" => {
                result.right_family = Some(
                    arguments
                        .next()
                        .ok_or_else(|| "--right-family needs a family name".to_string())?,
                );
            }
            "--order" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--order needs an integer".to_string())?;
                result.order = Some(
                    value
                        .parse()
                        .map_err(|error| format!("invalid order: {error}"))?,
                );
            }
            "--engine" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--engine needs a mode".to_string())?;
                result.engine = parse_engine(&value)?;
            }
            "--max-hyperspace-points" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-hyperspace-points needs an integer".to_string())?;
                result.max_hyperspace_points = value
                    .parse()
                    .map_err(|error| format!("invalid hyperspace limit: {error}"))?;
            }
            "--max-relation-vertices" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-relation-vertices needs an integer".to_string())?;
                result.max_relation_vertices = value
                    .parse()
                    .map_err(|error| format!("invalid relation limit: {error}"))?;
            }
            "--max-lazy-depth" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-lazy-depth needs an integer".to_string())?;
                result.lazy_config.max_depth = value
                    .parse()
                    .map_err(|error| format!("invalid lazy depth: {error}"))?;
            }
            "--max-search-nodes" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-search-nodes needs an integer".to_string())?;
                result.lazy_config.max_search_nodes = value
                    .parse()
                    .map_err(|error| format!("invalid search-node limit: {error}"))?;
            }
            "--max-candidate-scans" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-candidate-scans needs an integer".to_string())?;
                result.lazy_config.max_candidate_scans = value
                    .parse()
                    .map_err(|error| format!("invalid candidate-scan limit: {error}"))?;
            }
            "--max-distance-cache" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-distance-cache needs an integer".to_string())?;
                result.lazy_config.max_distance_cache = value
                    .parse()
                    .map_err(|error| format!("invalid distance-cache limit: {error}"))?;
            }
            "--mrv-window" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--mrv-window needs an integer".to_string())?;
                result.lazy_config.mrv_window = value
                    .parse()
                    .map_err(|error| format!("invalid MRV window: {error}"))?;
            }
            "--max-ultrametric-blocks" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-ultrametric-blocks needs an integer".to_string())?;
                result.lazy_config.max_ultrametric_blocks = value
                    .parse()
                    .map_err(|error| format!("invalid ultrametric block limit: {error}"))?;
            }
            "--max-ultrametric-assignments" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-ultrametric-assignments needs an integer".to_string())?;
                result.lazy_config.max_ultrametric_assignments = value
                    .parse()
                    .map_err(|error| format!("invalid ultrametric assignment limit: {error}"))?;
            }
            "--max-z3-variables" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-z3-variables needs an integer".to_string())?;
                result.lazy_config.max_z3_variables = value
                    .parse()
                    .map_err(|error| format!("invalid Z3 variable limit: {error}"))?;
            }
            "--max-z3-clauses" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-z3-clauses needs an integer".to_string())?;
                result.lazy_config.max_z3_clauses = value
                    .parse()
                    .map_err(|error| format!("invalid Z3 clause limit: {error}"))?;
            }
            "--z3-timeout-seconds" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--z3-timeout-seconds needs an integer".to_string())?;
                result.lazy_config.z3_timeout_seconds = value
                    .parse()
                    .map_err(|error| format!("invalid Z3 timeout: {error}"))?;
            }
            "--help" | "-h" => {
                print!("{}", usage());
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument {argument:?}\n{}", usage())),
        }
    }
    if result.left_edges.is_some() != result.right_edges.is_some() {
        return Err("provide both --left-edges and --right-edges".to_string());
    }
    let any_family = result.left_family.is_some() || result.right_family.is_some();
    if any_family
        && (result.left_family.is_none() || result.right_family.is_none() || result.order.is_none())
    {
        return Err("family mode needs --left-family, --right-family, and --order".to_string());
    }
    if result.order.is_some() && !any_family {
        return Err("--order is used only with catalog families".to_string());
    }
    let selected_modes = usize::from(result.known_pair)
        + usize::from(result.left_edges.is_some())
        + usize::from(any_family);
    if selected_modes > 1 {
        return Err("choose only one of known-pair, explicit-edge, and family modes".to_string());
    }
    if !result.hyperspace && !matches!(result.engine, EngineMode::Auto | EngineMode::Explicit) {
        return Err("lazy and ultrametric engines require --hyperspace".to_string());
    }
    if result.compare_base && !result.hyperspace {
        return Err("--compare-base requires --hyperspace".to_string());
    }
    if result.compare_base && result.engine == EngineMode::Explicit {
        return Err("--compare-base is intended for a lazy hyperspace engine".to_string());
    }
    if result.lazy_config.mrv_window == 0 {
        return Err("--mrv-window must be positive".to_string());
    }
    if result.lazy_config.max_ultrametric_blocks == 0 {
        return Err("--max-ultrametric-blocks must be positive".to_string());
    }
    if result.lazy_config.z3_timeout_seconds == 0 {
        return Err("--z3-timeout-seconds must be positive".to_string());
    }
    Ok(result)
}

fn half_integer(value: u32) -> String {
    if value.is_multiple_of(2) {
        (value / 2).to_string()
    } else {
        format!("{}.5", value / 2)
    }
}

fn run(arguments: Arguments) -> Result<(), String> {
    if arguments.self_test {
        run_self_test()?;
    }

    let requested_pair = if arguments.known_pair {
        Some((
            Metric::from_edges(&KNOWN_LEFT)?,
            Metric::from_edges(&KNOWN_RIGHT)?,
        ))
    } else if let (Some(left_family), Some(right_family), Some(order)) = (
        arguments.left_family,
        arguments.right_family,
        arguments.order,
    ) {
        Some((
            family_metric(&left_family, order)?,
            family_metric(&right_family, order)?,
        ))
    } else {
        arguments
            .left_edges
            .zip(arguments.right_edges)
            .map(|(left, right)| {
                Ok::<(Metric, Metric), String>((
                    Metric::from_edges(&left)?,
                    Metric::from_edges(&right)?,
                ))
            })
            .transpose()?
    };
    let Some((left, right)) = requested_pair else {
        if arguments.self_test {
            return Ok(());
        }
        return Err(format!(
            "choose --known-pair or provide explicit edges\n{}",
            usage()
        ));
    };

    let base_left_order = left.order;
    let base_right_order = right.order;
    let total_started = Instant::now();
    let explicit_engine = !arguments.hyperspace || matches!(arguments.engine, EngineMode::Explicit);
    if explicit_engine {
        let (search_left, search_right, layer) = if arguments.hyperspace {
            (
                hyperspace_metric(&left, arguments.max_hyperspace_points)?,
                hyperspace_metric(&right, arguments.max_hyperspace_points)?,
                "hyperspace",
            )
        } else {
            (left, right, "base")
        };
        let metric_setup_elapsed = total_started.elapsed();
        let search_started = Instant::now();
        let (distortion, stats) =
            minimum_distortion(&search_left, &search_right, arguments.max_relation_vertices)?;
        let search_elapsed = search_started.elapsed();
        let total_elapsed = total_started.elapsed();
        println!(
            "EXACT_RESULT layer={} engine=explicit base_orders={}x{} search_orders={}x{} \
             relation_vertices={} distortion={} d_GH={} candidate_thresholds={} \
             feasibility_checks={} search_nodes={} compatibility_masks_built={} \
             memo_hits={} failed_states={} metric_setup_ms={:.3} search_ms={:.3} \
             total_ms={:.3}",
            layer,
            base_left_order,
            base_right_order,
            search_left.order,
            search_right.order,
            search_left.order * search_right.order,
            distortion,
            half_integer(distortion),
            stats.candidate_thresholds,
            stats.feasibility_checks,
            stats.search_nodes,
            stats.compatibility_masks_built,
            stats.memo_hits,
            stats.failed_states,
            metric_setup_elapsed.as_secs_f64() * 1_000.0,
            search_elapsed.as_secs_f64() * 1_000.0,
            total_elapsed.as_secs_f64() * 1_000.0,
        );
    } else {
        let strategy = match arguments.engine {
            EngineMode::Auto | EngineMode::Lazy => LazyStrategy::Auto,
            EngineMode::LazyGeneric => LazyStrategy::Generic,
            EngineMode::Ultrametric => LazyStrategy::Ultrametric,
            EngineMode::Z3 => LazyStrategy::Z3,
            EngineMode::Explicit => unreachable!(),
        };
        let engine_name = match strategy {
            LazyStrategy::Auto => "lazy-auto",
            LazyStrategy::Generic => "lazy-generic",
            LazyStrategy::Ultrametric => "ultrametric",
            LazyStrategy::Z3 => "z3",
        };
        let base_distortion = if arguments.compare_base {
            Some(minimum_distortion(&left, &right, arguments.max_relation_vertices)?.0)
        } else {
            None
        };
        let (distortion, stats) = minimum_hyperspace_distortion_lazy(
            &left,
            &right,
            &arguments.lazy_config,
            strategy,
            base_distortion,
        )?;
        if base_distortion.is_some_and(|base| distortion > base) {
            return Err("lazy hyperspace result exceeds its lifted base upper bound".to_string());
        }
        let base_report = base_distortion
            .map(|value| value.to_string())
            .unwrap_or_else(|| "not-computed".to_string());
        let comparison_report = base_distortion
            .map(|value| {
                if value == distortion {
                    "equal"
                } else {
                    "strictly-smaller"
                }
            })
            .unwrap_or("not-computed");
        let total_elapsed = total_started.elapsed();
        println!(
            "EXACT_RESULT layer=hyperspace engine={} base_orders={}x{} \
             search_orders={}x{} implicit_relation_vertices={} distortion={} d_GH={} \
             candidate_thresholds={} feasibility_checks={} generic_checks={} \
             ultrametric_checks={} ultrametric_fallbacks={} structural_rejections={} \
             dominance_rejections={} lazy_nodes={} candidate_scans={} \
             compatibility_checks={} lazy_memo_hits={} max_depth={} ultrametric_calls={} \
             ultrametric_assignments={} ultrametric_max_blocks={} distance_queries={} \
             distance_cache_hits={} dilation_tests={} z3_checks={} z3_fallbacks={} \
             z3_variables={} z3_clauses={} base_distortion={} comparison={} \
             total_ms={:.3}",
            engine_name,
            base_left_order,
            base_right_order,
            (1_usize << base_left_order) - 1,
            (1_usize << base_right_order) - 1,
            ((1_u128 << base_left_order) - 1) * ((1_u128 << base_right_order) - 1),
            distortion,
            half_integer(distortion),
            stats.candidate_thresholds,
            stats.feasibility_checks,
            stats.generic_checks,
            stats.ultrametric_checks,
            stats.ultrametric_fallbacks,
            stats.structural.lower_bound_rejections
                + stats.structural.cardinality_rejections
                + stats.structural.component_count_rejections
                + stats.structural.component_dominance_rejections,
            stats.structural.component_dominance_rejections,
            stats.lazy_search_nodes,
            stats.candidate_scans,
            stats.compatibility_checks,
            stats.lazy_memo_hits,
            stats.maximum_depth,
            stats.ultrametric_recursive_calls,
            stats.ultrametric_assignment_candidates,
            stats.ultrametric_maximum_block_count,
            stats.left_dilation.distance_queries + stats.right_dilation.distance_queries,
            stats.left_dilation.distance_cache_hits + stats.right_dilation.distance_cache_hits,
            stats.left_dilation.dilation_tests + stats.right_dilation.dilation_tests,
            stats.z3_checks,
            stats.z3_fallbacks,
            stats.z3_variables,
            stats.z3_clauses,
            base_report,
            comparison_report,
            total_elapsed.as_secs_f64() * 1_000.0,
        );
    }
    Ok(())
}

pub(crate) fn main() -> ExitCode {
    match parse_arguments().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}
