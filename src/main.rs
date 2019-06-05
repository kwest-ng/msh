#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
// #![warn(clippy::cargo)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::multiple_crate_versions)]

#[macro_use]
extern crate clap;
#[macro_use]
extern crate derive_builder;
#[macro_use]
extern crate log;

use env_logger;

mod context;
// mod exec;
mod parser;
mod repl;

fn main() {
    env_logger::init();

    let cfg = match parser::parse_external_args() {
        Ok(c) => c,
        Err(s) => {
            eprintln!("{}", s);
            return;
        }
    };

    if atty::isnt(atty::Stream::Stdin) {
        eprintln!("Cannot accept piped input");
        return;
    }

    match repl::repl_loop(&cfg) {
        Ok(_) => {
            info!("Loop is exiting");
        }
        Err(s) => {
            error!("{}", s);
        }
    };
}
