//! `tatara-lispc` — compile a `.lisp` defpoint source into `Process` YAML.
//!
//! ```sh
//! tatara-lispc observability-stack.lisp
//! # prints one or more --- Process YAML blocks to stdout
//! ```

use std::env;
use std::fs;
use std::process::ExitCode;

use tatara_process::prelude::Process;

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
    let defs = match tatara_process::compile_source(&source) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("compile {path}: {e}");
            return ExitCode::from(1);
        }
    };
    for def in defs {
        let proc = Process::new(&def.name, def.spec);
        match serde_yaml::to_string(&proc) {
            Ok(y) => {
                println!("---");
                println!("{}", y.trim_end());
            }
            Err(e) => {
                eprintln!("serialize {}: {e}", def.name);
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}
