#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::multiple_crate_versions)]


use clap::{
    App, AppSettings, Arg,
    ErrorKind::{UnknownArgument, UnrecognizedSubcommand, HelpDisplayed},
    SubCommand,
};

use std::borrow::ToOwned;
use std::ffi::OsString;
use std::iter::Peekable;

use crate::context::{self, get_home_dir, Context, MshConfig, MshConfigBuilder};
use crate::repl::Action;


fn get_builtin<I, T>(args: I) -> Option<Action>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut builtins = App::new("builtins").usage("[SUBCOMMAND]")
        .settings(&[AppSettings::NoBinaryName, AppSettings::ColorNever])
        .subcommand(SubCommand::with_name("exit").visible_alias("quit").about("Terminates the shell"))
        .subcommand(SubCommand::with_name("help").about("Displays this help message"))
        .subcommand(SubCommand::with_name("dirs").about("Prints all registered directories"))
        .subcommand(
            SubCommand::with_name("cd").about("Change the current working directory")
                .arg(Arg::with_name("DIR").index(1).default_value(get_home_dir())),
        )
        .subcommand(SubCommand::with_name("echo").about("Prints all arguments").arg(Arg::with_name("ARGS").multiple(true)))
        .subcommand(
            SubCommand::with_name("var").about("Set or delete environment variables")
                .arg(Arg::with_name("NAME").required(true))
                .arg(Arg::with_name("VALUE"))
                .arg(Arg::with_name("DELETE").short("d").long("delete").help("Deletes NAME from the environment")),
        )
        .subcommand(
            SubCommand::with_name("register").about("Add directories to the registry")
                .visible_alias("reg")
                .arg(Arg::with_name("DIRS").required(true).min_values(1)),
        )
        .subcommand(
            SubCommand::with_name("unregister").about("Remove directories from the registry")
                .visible_alias("unreg")
                .arg(Arg::with_name("DIRS").required(true).min_values(1)),
        )
        .subcommand(
            SubCommand::with_name("register-file").about("Add all directories from FILE to the registry")
                .visible_alias("regfile")
                .arg(Arg::with_name("FILE").required(true)),
        )
        .subcommand(
            SubCommand::with_name("clear-register").about("Remove all directories from the registry")
                .visible_alias("clreg")
                .arg(Arg::with_name("DIRS").multiple(true)),
        );

    match builtins.get_matches_from_safe_borrow(args) {
        Err(e) => match e.kind {
            UnknownArgument | UnrecognizedSubcommand => {
                debug!("Line does not match any known builtin, forwarding to command executor");
                None
            }
            HelpDisplayed => {
                println!("{}", e.message);
                Some(Action::Loop)
            }
            _ => {
                error!("Builtin parsing error: {:?}", e);
                Some(Action::Loop)
            }
        },
        Ok(x) => match x.subcommand() {
            ("dump", _) => Some(Action::Dump),
            ("exit", _) => Some(Action::Exit(None)),
            ("cd", Some(args)) => Some(Action::ChDir(args.value_of("DIR").unwrap().to_owned())),
            ("echo", Some(args)) => {
                let joined = args.values_of_lossy("ARGS").unwrap_or_else(Vec::new).join(" ");
                println!("{}", joined);
                Some(Action::Loop)
            }
            ("var", Some(args)) => {
                let name = args.value_of("NAME").unwrap().into();  // Required by the parser
                let value = args.value_of("VALUE").unwrap().into();  // Default provided py the parser
                let delete = args.is_present("DELETE");
                let action = if delete {
                    Action::RemoveEnv{name}
                } else {
                    Action::StoreEnv{name, value}
                };
                Some(action)
            }
            ("help", _) => {
                builtins
                    .print_long_help()
                    .expect("Failed to print builtin help message");
                println!("");
                Some(Action::Loop)
            }
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
            ("register-file", Some(args)) => Some(Action::RegisterFile(
                args.value_of("FILE").unwrap().to_owned(),
            )),
            ("clear-register", Some(args)) => Some(Action::ClearRegistry({
                if args.is_present("DIRS") {
                    args.values_of("DIRS")
                        .unwrap()
                        .map(ToOwned::to_owned)
                        .collect()
                } else {
                    Vec::new()
                }
            })),
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

    let args = match expand_line(&full_line) {
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



pub(crate) fn expand_line(line: &str) -> Result<Vec<String>, &'static str> {
    let mut bytes = line.bytes().enumerate().peekable();
    let mut args = Vec::new();

    while let Some(&(_, b)) = bytes.peek() {
        // trace!("Line expansion: index {}, char {}", i+1, b as char);
        match b {
            b' ' | b'\t' => {
                bytes.next(); // Move peek to next byte, skip current
            }
            _ => {
                if let Some(v) = arg(line, &mut bytes)? {
                    args.push(v)
                }
            }
        }
    }

    // Ok(args)
    let expanded = args.drain(..).map(expand_var).collect();
    Ok(expanded)
}

fn expand_var(s: &str) -> String {
    debug!("Checking for var expansion in str: {}", s);
    if s.starts_with('\'') {
        return s.into();
    }

    let mut buf = String::with_capacity(s.len() * 2);
    let mut bytes = s.bytes().peekable();

    while let Some(&b) = bytes.peek() {
        // trace!("Line expansion: index {}, char {}", i+1, b as char);
        match b {
            b'\\' => {
                // buf.push(b as char);
                // Treat the next char as normal
                bytes.next(); // Skip backslash
                match bytes.next() {
                    // Move peek to next byte, but consume the current one.
                    Some(b) => {
                        buf.push(b as char);
                    }
                    // Cannot be none, previous parser would have consumed the next char.
                    None => unreachable!(),
                }
            }
            b'$' => {
                bytes.next(); // Skip the leading '$'
                let mut var_name = String::new();
                while let Some(&b) = bytes.peek() {
                    match b {
                        b'a'...b'z' | b'A'...b'Z' | b'_' | b'0'...b'9' => {
                            var_name.push(b as char);
                            bytes.next(); // Move peek to next byte
                        }
                        _ => {
                            break;
                        }
                    }
                }

                let maybe_expansion =
                    std::env::var(&var_name).unwrap_or_else(|_| format!("${}", &var_name));
                debug!("Expanded ${} to {}", var_name, maybe_expansion);
                buf.push_str(&maybe_expansion);
            }
            b'~' => {
                let home = get_home_dir();
                debug!("Expanding ~ to {}", home);
                buf.push_str(home);
                bytes.next(); // Move peek to next byte, skip the ~
            }
            _ => {
                buf.push(b as char);
                bytes.next(); // Move peek to next byte, take the char.
            }
        }
    }

    debug!("Finished var expansion: {}", buf);
    buf
}

fn arg<'a, I>(line: &'a str, bytes: &mut Peekable<I>) -> Result<Option<&'a str>, &'static str>
where
    I: Iterator<Item = (usize, u8)>,
{
    let mut start = None;
    let mut end = None;

    // Skip over any leading whitespace
    while let Some(&(_, b)) = bytes.peek() {
        match b {
            b' ' | b'\t' => {
                bytes.next();
            }
            _ => break,
        }
    }

    while let Some(&(i, b)) = bytes.peek() {
        if start.is_none() {
            start = Some(i)
        }
        match b {
            // Evaluate a quoted string but do not return it
            // We pass in i, the index of a quote, but start a character later. This ensures
            // the production rules will produce strings with the quotes intact
            b'"' => {
                bytes.next();
                double_quoted(line, bytes, i)?;
            }
            b'\'' => {
                bytes.next();
                single_quoted(line, bytes, i)?;
            }
            // If we see a backslash, assume that it is leading up to an escaped character
            // and skip the next character
            b'\\' => {
                bytes.next();
                bytes.next();
            }
            // If we see a byte from the following set, we've definitely reached the end of
            // the argument
            b' ' | b'\t' => {
                end = Some(i);
                break;
            }
            // By default just pop the next byte: it will be part of the argument
            _ => {
                bytes.next();
            }
        }
    }

    match (start, end) {
        (Some(i), Some(j)) if i < j => Ok(Some(&line[i..j])),
        (Some(i), None) => Ok(Some(&line[i..])),
        _ => Ok(None),
    }
}

fn double_quoted<'a, I>(
    line: &'a str,
    bytes: &mut Peekable<I>,
    start: usize,
) -> Result<&'a str, &'static str>
where
    I: Iterator<Item = (usize, u8)>,
{
    while let Some(&(i, b)) = bytes.peek() {
        bytes.next();

        if b == b'"' {
            // We return an inclusive range to keep the quote type intact
            return Ok(&line[start..=i]);
        } else if b == b'\\' {
            // Skip the next character even if it's a quote,
            bytes.next();
        }
    }

    Err("Unterminated double quote")
}

fn single_quoted<'a, I>(
    line: &'a str,
    bytes: &mut Peekable<I>,
    start: usize,
) -> Result<&'a str, &'static str>
where
    I: Iterator<Item = (usize, u8)>,
{
    while let Some(&(i, b)) = bytes.peek() {
        bytes.next();

        if b == b'\'' {
            // We return an inclusive range to keep the quote type intact
            return Ok(&line[start..=i]);
        };
    }

    Err("Unterminated single quote")
}
