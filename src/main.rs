#[macro_use]
extern crate log;

use env_logger;

use rustyline::{config::Configurer, error::ReadlineError, Cmd, Editor, Helper, KeyPress};

use std::env;
use std::io::{Error as IOError, ErrorKind, Result as IOResult};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Hash)]
enum Action {
    Loop,
    Buffer(String),
    // StoreEnv{ name: String, value: String },
    // RemoveEnv{ name: String },
    Exit(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Hash)]
enum Runnable {
    Action(Action),
    Executable(Vec<String>),
}

impl Runnable {
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

struct Context {
    buffer: Option<String>,
}

impl Context {
    pub fn new() -> Self {
        let buffer = None;
        Self { buffer }
    }

    pub fn push_buffer(&mut self, push: &str) {
        debug!("Pushing line to buffer: {}", push);
        if log_enabled!(log::Level::Trace) {
            trace!(
                "Current buffer contents: {}",
                self.buffer.as_ref().unwrap_or(&"".to_owned())
            );
        };
        let mut buf = self.buffer.take().unwrap_or_else(|| String::new());
        buf.push_str(push);
        trace!("New buffer contents: {}", buf);
        self.buffer = Some(buf);
    }

    pub fn take_buffer(&mut self, join_with: &str) -> String {
        let full_line = match self.buffer.take() {
            None => join_with.to_owned(),
            Some(s) => format!("{}{}", s, join_with),
        };
        trace!("Buffer removed, resulting contents: {}", full_line);
        full_line
    }

    pub fn has_buffer(&self) -> bool {
        self.buffer.is_some()
    }
}

fn run_executable(args: Vec<String>) {
    assert!(args.len() >= 1);
    if log_enabled!(log::Level::Trace) {
        for arg in args.iter().by_ref() {
            trace!("Arg found: \"{}\"", arg);
        }
    };
    let output = Command::new(&args[0])
        .args(&args[1..])
        .output()
        .expect("failed to execute the process");
    info!("Captured raw stdout: {:?}", String::from_utf8(output.stdout).expect("command output was not UTF-8"));
}

fn get_cwd<'a>() -> IOResult<String> {
    let cwd = env::current_dir()?.to_string_lossy().to_string();
    trace!("current dir: {}", cwd);
    Ok(cwd)
}

fn get_prompt(ctx: &Context) -> IOResult<String> {
    match ctx.has_buffer() {
        true => Ok("... ".to_owned()),
        false => {
            let mut prompt = get_cwd()?;
            prompt.push_str("> ");

            trace!("prompt generated: {}", prompt);
            Ok(prompt)
        }
    }
}

fn get_builtin<'a>(line: &str) -> Option<Runnable> {
    match line {
        "exit" | "quit" => Some(Runnable::Action(Action::Exit(None))),
        _ => None,
    }
}

fn parse_shell_input(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .into_iter()
        .filter(|s| s.trim().len() > 0)
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

fn init_context() -> Context {
    Context::new()
}

fn repl_loop() -> Result<(), String> {
    let mut rl = init_editor();
    let hist_path = load_history(&mut rl);
    let mut ctx = init_context();

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
                };
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
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

fn main() {
    env_logger::init();
    match repl_loop() {
        Ok(_) => {
            info!("Loop is exiting");
        }
        Err(s) => {
            error!("{}", s);
        }
    };
}
