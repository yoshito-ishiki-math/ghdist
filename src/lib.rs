//! Exact finite Gromov–Hausdorff correspondence search.
//! Integer arithmetic, exhaustive decision procedures, and explicit resource limits.

mod bitset;
mod bounds;
mod cli;
mod config;
mod explicit;
mod hyperspace;
mod metric;
mod verification;

#[cfg(test)]
mod tests;

mod app;
mod input;
mod job;
mod runtime;
mod witness;

pub fn main_cli() -> std::process::ExitCode {
    app::main()
}

#[cfg(test)]
mod usability_tests;
