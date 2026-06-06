//! `tatara-lispc` — compile a `.lisp` source into `Process` YAML.
//!
//! Handles three authoring surfaces, all lowering to the universal `Process`
//! CRD:
//! - `(defenvmatrix …)` — a permutation generator; **fans out** to N Process
//!   YAMLs (one per environment in the sweep).
//! - `(defephemeral …)` — one ephemeral environment → one Process.
//! - `(defpoint …)` — the full Process surface → one Process.
//!
//! ```sh
//! tatara-lispc echo-sweep.lisp
//! # prints one or more --- Process YAML blocks to stdout
//! ```

use std::env;
use std::fs;
use std::process::ExitCode;

use tatara_process::ephemeral::compile_ephemeral_source;
use tatara_process::matrix::compile_env_matrix_source;
use tatara_process::prelude::{Process, ProcessSpec};

fn emit(name: &str, spec: ProcessSpec) -> Result<(), ()> {
    let proc = Process::new(name, spec);
    match serde_yaml::to_string(&proc) {
        Ok(y) => {
            println!("---");
            println!("{}", y.trim_end());
            Ok(())
        }
        Err(e) => {
            eprintln!("serialize {name}: {e}");
            Err(())
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: tatara-lispc <file.lisp>");
        return ExitCode::from(2);
    }
    let path = &args[1];
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return ExitCode::from(1);
        }
    };

    // Dispatch on the authored form. Each path lowers to the universal Process
    // CRD; a matrix fans out to one Process per generated environment.
    macro_rules! compiled {
        ($call:expr) => {
            match $call {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{}", tatara_lisp::format_diagnostic(&source, &e, Some(path)));
                    return ExitCode::from(1);
                }
            }
        };
    }

    if source.contains("(defenvmatrix") {
        for m in compiled!(compile_env_matrix_source(&source)) {
            let envs = m.spec.expand(&m.name);
            eprintln!(
                "tatara-lispc: matrix '{}' → {} environment(s) (selection {})",
                m.name,
                envs.len(),
                m.spec.selection_size()
            );
            for env in envs {
                if emit(&env.name, env.spec.into()).is_err() {
                    return ExitCode::from(1);
                }
            }
        }
    } else if source.contains("(defephemeral") {
        for d in compiled!(compile_ephemeral_source(&source)) {
            if emit(&d.name, d.spec.into()).is_err() {
                return ExitCode::from(1);
            }
        }
    } else {
        for d in compiled!(tatara_process::compile_source(&source)) {
            if emit(&d.name, d.spec).is_err() {
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}
