//! Cooperative deadlines, progress, and certified search intervals.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static CANCELLED: AtomicBool = AtomicBool::new(false);
thread_local! { static ACTIVE: RefCell<Option<Control>> = const { RefCell::new(None) }; }

#[derive(Clone, Debug)]
pub(crate) struct Interval {
    pub(crate) lower: u32,
    pub(crate) upper: u32,
    pub(crate) lower_evidence: String,
}
struct Control {
    started: Instant,
    limit: Option<Duration>,
    progress: bool,
    phase: String,
    intervals: BTreeMap<String, Interval>,
    last_print: Instant,
    max_nodes: u64,
    unit: crate::input::Rational,
}
pub(crate) struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.with(|active| *active.borrow_mut() = None);
    }
}

pub(crate) fn begin(
    limit: Option<Duration>,
    progress: bool,
    max_nodes: u64,
    unit: crate::input::Rational,
) -> Guard {
    ACTIVE.with(|active| {
        *active.borrow_mut() = Some(Control {
            started: Instant::now(),
            limit,
            progress,
            phase: "base".into(),
            intervals: BTreeMap::new(),
            last_print: Instant::now(),
            max_nodes,
            unit,
        })
    });
    Guard
}
pub(crate) fn phase(phase: &str) {
    ACTIVE.with(|active| {
        if let Some(control) = active.borrow_mut().as_mut() {
            control.phase = phase.into();
        }
    });
}
pub(crate) fn bounds(lower: u32, upper: u32, evidence: &str) {
    debug_assert!(lower <= upper);
    ACTIVE.with(|active| {
        if let Some(control) = active.borrow_mut().as_mut() {
            let previous = control.intervals.get(&control.phase);
            let evidence = if previous.is_some_and(|old| old.lower == lower) {
                previous.unwrap().lower_evidence.clone()
            } else {
                evidence.into()
            };
            control.intervals.insert(
                control.phase.clone(),
                Interval {
                    lower,
                    upper,
                    lower_evidence: evidence,
                },
            );
            if control.progress {
                print_progress(control);
            }
        }
    });
}
fn print_progress(control: &mut Control) {
    let bound = control
        .intervals
        .get(&control.phase)
        .map(|b| {
            let lower = control
                .unit
                .half_multiple(b.lower)
                .map(|x| x.to_string())
                .unwrap_or_else(|_| "?".into());
            let upper = control
                .unit
                .half_multiple(b.upper)
                .map(|x| x.to_string())
                .unwrap_or_else(|_| "?".into());
            format!("{lower} <= d_GH <= {upper}")
        })
        .unwrap_or_else(|| "working".into());
    eprintln!(
        "[{}] {:.2}s  {}",
        control.phase,
        control.started.elapsed().as_secs_f64(),
        bound
    );
    control.last_print = Instant::now();
}
pub(crate) fn check() -> Result<(), String> {
    if cancelled() {
        return Err("interrupted by Ctrl-C; computation incomplete".into());
    }
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let Some(control) = active.as_mut() else {
            return Ok(());
        };
        if control
            .limit
            .is_some_and(|limit| control.started.elapsed() >= limit)
        {
            return Err("time limit reached; computation incomplete".into());
        }
        if control.progress && control.last_print.elapsed() >= Duration::from_secs(1) {
            print_progress(control);
        }
        Ok(())
    })
}
pub(crate) fn node(nodes: u64) -> Result<(), String> {
    check()?;
    ACTIVE.with(|active| {
        if let Some(control) = active.borrow().as_ref()
            && nodes > control.max_nodes
        {
            return Err(format!("search-node limit {} reached", control.max_nodes));
        }
        Ok(())
    })
}
pub(crate) fn interval(phase: &str) -> Option<Interval> {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .and_then(|c| c.intervals.get(phase).cloned())
    })
}
pub(crate) fn cancelled() -> bool {
    CANCELLED.load(Ordering::Relaxed)
}

#[cfg(unix)]
extern "C" fn handle_interrupt(_: libc::c_int) {
    CANCELLED.store(true, Ordering::Relaxed);
}

pub(crate) fn install_interrupt_handler() -> Result<(), String> {
    #[cfg(unix)]
    {
        // The handler performs only a lock-free atomic store. No allocation,
        // I/O, locks, or unwinding occurs in signal context.
        let previous = unsafe {
            libc::signal(
                libc::SIGINT,
                handle_interrupt as *const () as libc::sighandler_t,
            )
        };
        if previous == libc::SIG_ERR {
            return Err("could not install the interrupt handler".into());
        }
    }
    Ok(())
}
