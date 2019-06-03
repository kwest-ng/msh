#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::multiple_crate_versions)]


use clap::{
    App, AppSettings, Arg,
    ErrorKind::{UnknownArgument, UnrecognizedSubcommand},
    SubCommand,
};

// use rayon::prelude::*;

use std::borrow::ToOwned;
use std::ffi::OsString;

use crate::context::{self, get_home_dir, Context, MshConfig, MshConfigBuilder};
use crate::parser;
use crate::repl::Action;

fn get_builtin<I, T>(args: I) -> Option<Action>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut builtins = App::new("builtins")
        .settings(&[AppSettings::NoBinaryName, AppSettings::ColorNever])
        .subcommand(SubCommand::with_name("exit").visible_alias("quit"))
        .subcommand(SubCommand::with_name("help"))
        .subcommand(SubCommand::with_name("dump"))
        .subcommand(
            SubCommand::with_name("cd")
                .arg(Arg::with_name("DIR").index(1).default_value(get_home_dir())),
        )
        .subcommand(
            SubCommand::with_name("register")
                .visible_alias("reg")
                .arg(Arg::with_name("DIRS").required(true).min_values(1)),
        )
        .subcommand(
            SubCommand::with_name("unregister")
                .visible_alias("unreg")
                .arg(Arg::with_name("DIRS").required(true).min_values(1)),
        )
        .subcommand(
            SubCommand::with_name("register-file")
                .visible_alias("regfile")
                .arg(Arg::with_name("FILE").required(true)),
        )
        .subcommand(
            SubCommand::with_name("clear-register")
                .visible_alias("clreg")
                .arg(Arg::with_name("DIRS").multiple(true)),
        );

    match builtins.get_matches_from_safe_borrow(args) {
        Err(e) => match e.kind {
            UnknownArgument | UnrecognizedSubcommand => {
                debug!("Line does not match any known builtin, forwarding to command executor");
                None
            }
            _ => {
                error!("Builtin parsing error: {:?}", e);
                Some(Action::Loop)
            }
        },
        Ok(x) => match x.subcommand() {
            ("exit", _) => Some(Action::Exit(None)),
            ("cd", Some(args)) => Some(Action::ChDir(args.value_of("DIR").unwrap().to_owned())),
            ("help", _) => {
                builtins
                    .print_long_help()
                    .expect("Failed to print builtin help message");
                Some(Action::Loop)
            }
            ("dump", _) => Some(Action::Dump),
            ("register", Some(args)) => Some(Action::Register(
                args.values_of("DIRS")
                    .unwrap()
                    .map(ToOwned::to_owned)
                    .collect(),
            )),
            ("unregister", Some(args)) => Some(Action::Unregister(
                args.values_of("DIRS")
                    .unwrap()
                    .map(ToOwned::to_owned)
                    .collect(),
            )),
            ("register-file", _) => {
                println!("unimplemented command: register-file");
                Some(Action::Loop)
            }
            ("clear-register", _) => {
                println!("unimplemented command: clear-register");
                Some(Action::Loop)
            }
            _ => unreachable!(),
        },
    }
}

pub(crate) fn handle_line(ctx: &mut Context, line: &str) -> Action {
    trace!("Raw line: {}", line);

    if line.ends_with('\\') {
        let mut store = line.to_owned();
        store.pop(); // guaranteed to succeed
        return Action::Buffer(store);
    };

    let full_line = ctx.take_buffer(line);

    if full_line.trim().is_empty() {
        return Action::Loop;
    };

    let args = match parser::expand_line(&full_line) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            return Action::Loop;
        }
    };

    get_builtin(&args).unwrap_or_else(|| {
        debug!("reading line into process executor: {}", &full_line);
        Action::Execute(args.iter().map(ToString::to_string).collect())
    })
    // unimplemented!()
}

pub(crate) fn parse_external_args() -> Result<MshConfig, String> {
    let matches = app_from_crate!()
        .arg(
            Arg::with_name("registry")
                .short("r")
                .long("registry")
                .value_name("FILE")
                .help("pre-loads registered directories")
                .long_help("A whitespace-separated list of directories to automatically register."),
        )
        .get_matches();

    let mut cfg_build = MshConfigBuilder::default();

    if let Some(x) = matches.value_of("registry") {
        match context::read_registry_file(x) {
            Ok(v) => {
                cfg_build.preload_dirs(v);
            }
            Err(e) => {
                warn!("Could not read registry file \"{}\" -> error: {}", x, e);
            }
        };
    };
    cfg_build.build()
}
