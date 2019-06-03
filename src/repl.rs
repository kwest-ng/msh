#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::multiple_crate_versions)]

use rustyline::{config::Configurer, error::ReadlineError, Cmd, Editor, Helper, KeyPress};

use std::borrow::ToOwned;
use std::env;
use std::io::{Error as IOError, ErrorKind, Result as IOResult};
use std::path::PathBuf;
use std::string::ToString;

use crate::context::{self, Context, MshConfig};
use crate::exec;

#[derive(Debug, Clone, PartialEq, Hash)]
pub(crate) enum Action {
    Loop,
    Dump,
    Buffer(String),
    Register(Vec<String>),
    Unregister(Vec<String>),
    // RegisterFile(String),
    // ClearRegistry(Vec<String>),
    ChDir(String),
    // StoreEnv{ name: String, value: String },
    // RemoveEnv{ name: String },
    Execute(Vec<String>),
    Exit(Option<String>),
}

fn find_history_file() -> Option<PathBuf> {
    dirs::data_local_dir().and_then(|mut d| {
        d.push("msh-history");
        Some(d)
    })
}

pub fn get_cwd() -> IOResult<String> {
    let cwd = env::current_dir()?.to_string_lossy().to_string();
    debug!("current dir: {}", cwd);
    let home = context::get_home_dir();
    let len = home.len();
    if cwd.starts_with(&home) {
        Ok(format!("~{}", &cwd[len..]))
    } else {
        Ok(cwd)
    }
}

fn get_prompt(ctx: &Context) -> IOResult<String> {
    if ctx.has_buffer() {
        Ok("... ".to_owned())
    } else {
        let mut prompt = String::with_capacity(40);
        prompt.push('(');
        prompt.push_str(&ctx.dir_count().to_string());
        prompt.push_str(") ");
        prompt.push_str(&get_cwd()?);
        prompt.push_str("> ");

        trace!("prompt generated: {}", prompt);
        Ok(prompt)
    }
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

fn init_context(cfg: &MshConfig) -> Context {
    debug!("Initializing context");
    let mut ctx = Context::default();

    trace!("Preloading registry");
    context::register_paths(&mut ctx, cfg.dirs());

    ctx
}

fn init_editor() -> Editor<()> {
    debug!("Init Editor");
    let mut rl = Editor::<()>::new();

    trace!("history auto add: true");
    rl.set_auto_add_history(true);

    trace!("Keybind:: Up Arrow: Move previous history");
    rl.bind_sequence(KeyPress::Up, Cmd::PreviousHistory);

    trace!("Keybind:: Down Arrow: Move next history");
    rl.bind_sequence(KeyPress::Down, Cmd::NextHistory);

    rl
}

fn load_history<H: Helper>(editor: &mut Editor<H>) -> IOResult<PathBuf> {
    if let Some(p) = find_history_file() {
        debug!("History file location: {}", p.display());
        if let Err(e) = editor.load_history(&p) {
            warn!("History could not be loaded; error: {}", e);
        } else {
            info!("History file loaded");
        };

        Ok(p)
    } else {
        warn!("Could not determine history file location");
        Err(IOError::from(ErrorKind::NotFound))
    }
}

pub(crate) fn repl_loop(cfg: &MshConfig) -> Result<(), String> {
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
                match exec::handle_line(&mut ctx, &line) {
                    Action::Loop => {}
                    Action::Exit(opt_s) => {
                        if let Some(s) = opt_s {
                            println!("{}", s);
                        }
                        break;
                    }
                    Action::Buffer(s) => ctx.push_buffer(&s),
                    Action::Register(v) => context::register_paths(&mut ctx, &v),
                    Action::Unregister(v) => context::unregister_paths(&mut ctx, &v),
                    Action::Execute(v) => ctx.run_executable(&v),
                    Action::ChDir(p) => {
                        env::set_current_dir(p).unwrap_or_else(|e| {
                            println!("ChDir error: {}", e);
                        });
                    }
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
