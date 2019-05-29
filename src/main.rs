#[macro_use]
extern crate clap;
#[macro_use]
extern crate derive_builder;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

use clap::Arg;

use env_logger;

use regex::{Captures, Regex};

use rustyline::{config::Configurer, error::ReadlineError, Cmd, Editor, Helper, KeyPress};

use std::collections::HashSet;
use std::env;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::fs::File;
use std::io::{prelude::*, stdin, Error as IOError, ErrorKind, Result as IOResult};
use std::mem;
use std::path::PathBuf;
// use std::process::Command;

#[derive(Debug, Clone, PartialEq, Hash)]
enum Action {
    Loop,
    Buffer(String),
    // StoreEnv{ name: String, value: String },
    // RemoveEnv{ name: String },
    Register(Vec<String>),
    // RegisterFile(String),
    // Unregister(Vec<String>),
    // ClearRegistry(Vec<String>),
    Dump,
    Exit(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Hash)]
enum Runnable {
    Action(Action),
    Executable(Vec<String>),
}

impl Runnable {
    pub fn noop() -> Runnable {
        Runnable::Action(Action::Loop)
    }

    pub fn exit(s: Option<String>) -> Runnable {
        Runnable::Action(Action::Exit(s))
    }

    pub fn run(self) -> Action {
        match self {
            Runnable::Action(a) => a,
            Runnable::Executable(v) => {
                run_executable(v);
                Action::Loop
            }
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, Builder)]
#[builder(setter(into))]
struct MshConfig {
    #[builder(default)]
    preload_dirs: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq)]
struct Context {
    buffer: String,
    dir_registry: HashSet<PathBuf>,
}

impl Context {
    pub fn push_buffer(&mut self, push: &str) {
        debug!("Pushing line to buffer: {}", push);
        if log_enabled!(log::Level::Trace) {
            trace!("Current buffer contents: {}", self.buffer);
        };
        // let mut buf = self.buffer.take().unwrap_or_else(|| String::new());
        self.buffer.push_str(push);
        trace!("New buffer contents: {}", self.buffer);
        // self.buffer = buf;
    }

    pub fn take_buffer(&mut self, join_with: &str) -> String {
        // let full_line = match self.buffer.take() {
        //     None => join_with.to_owned(),
        //     Some(s) => format!("{}{}", s, join_with),
        // };
        let mut full_line = String::new();
        self.buffer.push_str(join_with);
        mem::swap(&mut full_line, &mut self.buffer);
        trace!("Buffer removed, resulting contents: {}", full_line);
        trace!("Current buffer contents after removal: \"{}\"", self.buffer);
        full_line
    }

    pub fn has_buffer(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn register(&mut self, path: PathBuf) -> Result<(PathBuf, bool), String> {
        let real_path = path.canonicalize().map_err(to_string)?;
        Ok((real_path.clone(), self.dir_registry.insert(real_path)))
    }
}

impl Display for Context {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        // formatter.write_str("Current buffer contents: ")?;
        // formatter.write_str(&self.buffer)?;
        // formatter.write_str("\n");
        writeln!(formatter, "Registered directories:")?;
        for path in &self.dir_registry {
            writeln!(formatter, "{}", &path.display())?;
        }
        Ok(())
    }
}

fn run_executable(args: Vec<String>) {
    assert!(!args.is_empty());
    if log_enabled!(log::Level::Trace) {
        for arg in args.iter().by_ref() {
            trace!("Arg found: \"{}\"", arg);
        }
    };
    println!("Execute command: {:?}", args);
    // info!(
    //     "Captured raw stdout: {:?}",
    //     String::from_utf8(output.stdout).expect("command output was not UTF-8")
    // );
}

fn get_cwd() -> IOResult<String> {
    let cwd = env::current_dir()?.to_string_lossy().to_string();
    trace!("current dir: {}", cwd);
    Ok(cwd)
}

fn get_prompt(ctx: &Context) -> IOResult<String> {
    if ctx.has_buffer() {
        Ok("... ".to_owned())
    } else {
        let mut prompt = get_cwd()?;
        prompt.push_str("> ");

        trace!("prompt generated: {}", prompt);
        Ok(prompt)
    }
}

fn parse_command_input(caps: Captures) -> Option<Runnable> {
    let maybe_args = caps.name("args");
    let command = caps.name("command").unwrap().as_str();

    trace!("Parsing command input -- command: \"{}\"", command);

    let opt: Runnable = match command {
        "dump" => Runnable::Action(Action::Dump),
        "reg" | "register" => {
            // unimplemented!();
            match maybe_args {
                None => {
                    eprintln!("register command takes at least 1 argument, found 0");
                    Runnable::noop()
                }
                Some(s) => Runnable::Action(Action::Register(
                    s.as_str()
                        .trim()
                        .split_whitespace()
                        .map(|s| s.to_owned())
                        .collect(),
                )),
            }
        }
        "unreg" | "unregister" => {
            unimplemented!();
            // match maybe_args {
            //     None => {
            //         eprintln!("unregister command takes at least 1 argument, found 0");
            //         Runnable::noop()
            //     }
            //     Some(s) => Runnable::Action(Action::Unregister(
            //         s.as_str()
            //             .trim()
            //             .split_whitespace()
            //             .map(|s| s.to_owned())
            //             .collect(),
            //     )),
            // }
        }
        "clreg" | "clear-register" => {
            unimplemented!();
            // match maybe_args {
            //     None => Runnable::Action(Action::ClearRegistry(vec![])),
            //     Some(s) => Runnable::Action(Action::ClearRegistry(
            //         s.as_str()
            //             .trim()
            //             .split_whitespace()
            //             .map(|s| s.to_owned())
            //             .collect(),
            //     )),
            // }
        }
        "regfile" | "register-file" => {
            unimplemented!();
            // match maybe_args {
            //     None => {
            //         eprintln!("unregister command takes 1 argument, found 0");
            //         Runnable::noop()
            //     }
            //     Some(s) => Runnable::Action(Action::RegisterFile(s.as_str().to_owned())),
            // }
        }
        _ => unreachable!(),
    };

    Some(opt)
}

fn get_builtin(line: &str) -> Option<Runnable> {
    match line {
        "exit" | "quit" => Some(Runnable::exit(None)),
        "help" => {
            print_help();
            Some(Runnable::noop())
        }
        x => {
            lazy_static! {
                static ref BUILTIN: Regex = {
                    trace!("init built-in regex");
                    Regex::new(include_str!("builtin.regex")).unwrap()
                };
            }
            BUILTIN.captures(x).and_then(parse_command_input)
            // unimplemented!()
        }
    }
}

fn print_help() {
    println!("\n{}", include_str!("help.txt").trim_end());
}

fn parse_shell_input(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        // .into_iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_owned())
        .collect()
}

fn handle_line(ctx: &mut Context, line: &str) -> Action {
    trace!("Raw line: {}", line);

    if line.ends_with('\\') {
        let mut store = line.to_owned();
        store.pop(); // guaranteed to succeed
        return Action::Buffer(store);
    };

    let full_line = ctx.take_buffer(line);

    get_builtin(&full_line)
        .unwrap_or_else(|| Runnable::Executable(parse_shell_input(&full_line)))
        .run()
}

fn init_context(cfg: &MshConfig) -> Context {
    let mut ctx = Default::default();
    register_paths(&mut ctx, &cfg.preload_dirs);

    ctx
}

fn handle_loop_error(err: ReadlineError) {
    match err {
        ReadlineError::Interrupted => {
            println!("CTRL-C");
        }
        ReadlineError::Eof => {
            println!("CTRL-D");
        }
        err => {
            println!("Error: {:?}", err);
        }
    };
}

fn register_paths(ctx: &mut Context, paths: &Vec<String>) {
    for path in paths {
        let (real_path, new) = match ctx.register(PathBuf::from(&path)) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("Cannot register path {}: {}", path, e);
                continue;
            }
        };

        if new {
            println!("Registered new path: {}", real_path.display());
        } else {
            println!("Already Registered: {}", real_path.display());
        };
    }
}

fn repl_loop(cfg: &MshConfig) -> Result<(), String> {
    let mut rl = init_editor();
    let hist_path = load_history(&mut rl);
    let mut ctx = init_context(cfg);

    info!("Starting REPL");

    loop {
        let prompt = match get_prompt(&ctx) {
            Ok(string) => string,
            Err(io_err) => {
                return Err(format!("{}", io_err));
            }
        };
        match rl.readline(&prompt) {
            Ok(line) => {
                // rl.add_history_entry(line.as_str());
                match handle_line(&mut ctx, &line) {
                    Action::Loop => {}
                    Action::Exit(opt_s) => {
                        if let Some(s) = opt_s {
                            println!("{}", s);
                        }
                        break;
                    }
                    Action::Buffer(s) => ctx.push_buffer(&s),
                    Action::Register(v) => register_paths(&mut ctx, &v),
                    Action::Dump => {
                        println!("{}", &ctx);
                    }
                };
            }
            Err(e) => {
                handle_loop_error(e);
                break;
            }
        }
    }

    if let Ok(path) = hist_path {
        match rl.save_history(&path) {
            Ok(_) => info!("Saving history file: {}", &path.display()),
            Err(e) => warn!("Cannot save history file: {} error: {}", &path.display(), e),
        };
    } else {
        warn!("No history file to save to, skipping save procedure");
    };

    Ok(())
}

fn find_history_file() -> Option<PathBuf> {
    dirs::data_local_dir().and_then(|mut d| {
        d.push("msh-history");
        Some(d)
    })
}

fn load_history<H: Helper>(editor: &mut Editor<H>) -> IOResult<PathBuf> {
    match find_history_file() {
        Some(p) => {
            match editor.load_history(&p) {
                Err(e) => {
                    warn!("History could not be loaded; error: {}", e);
                }
                Ok(_) => {
                    info!("Loaded history file: {}", p.display());
                }
            };
            Ok(p)
        }
        None => {
            warn!("Could not determine history file location");
            Err(IOError::from(ErrorKind::NotFound))
        }
    }
}

fn init_editor() -> Editor<()> {
    trace!("Init Editor");
    let mut rl = Editor::<()>::new();

    rl.set_auto_add_history(true);
    rl.bind_sequence(KeyPress::Up, Cmd::PreviousHistory);
    rl.bind_sequence(KeyPress::Down, Cmd::NextHistory);

    rl
}

fn get_boxed_file(name: &str) -> Result<Box<dyn Read>, String> {
    Ok(Box::new(File::open(name).map_err(to_string)?))
}

fn to_string(obj: impl std::string::ToString) -> String {
    obj.to_string()
}

fn read_registry_file(name: &str) -> Result<Vec<String>, String> {
    let mut reader: Box<dyn Read> = match name {
        "-" => Box::new(stdin()),
        x => get_boxed_file(x)?,
    };

    let mut buf = String::new();
    reader.read_to_string(&mut buf).map_err(to_string)?;

    Ok(buf.split_whitespace().map(|s| s.to_owned()).collect())
}

fn parse_args() -> Result<MshConfig, String> {
    let matches = app_from_crate!().arg(
        Arg::with_name("registry")
            .short("r")
            .long("registry")
            .value_name("FILE")
            .help("pre-loads registered directories")
            .long_help("A whitespace-separated list of directories to automatically register.  '-' reads from stdin.")).get_matches();

    let mut cfg_build = MshConfigBuilder::default();

    if let Some(x) = matches.value_of("registry") {
        match read_registry_file(x) {
            Ok(v) => {
                cfg_build.preload_dirs(v);
            }
            Err(e) => {
                warn!("{}", e);
            }
        };
    };
    cfg_build.build()
}

fn main() {
    env_logger::init();

    let cfg = match parse_args() {
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

    match repl_loop(&cfg) {
        Ok(_) => {
            info!("Loop is exiting");
        }
        Err(s) => {
            error!("{}", s);
        }
    };
}
