#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::multiple_crate_versions)]

use colored::*;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::config::{Configurer, OutputStreamType};
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::{
    Cmd, CompletionType, Config, Context as RLContext, EditMode, Editor, Helper, KeyPress,
};

use std::borrow::{Cow, Cow::Borrowed, Cow::Owned, ToOwned};
use std::env;
use std::io::{Error as IOError, ErrorKind, Result as IOResult};
use std::path::PathBuf;
use std::string::ToString;

use crate::context::{self, Context, MshConfig};
use crate::parser;

struct MshHelper(FilenameCompleter, MatchingBracketHighlighter, HistoryHinter);

impl Completer for MshHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &RLContext<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        self.0.complete(line, pos, ctx)
    }
}

impl Hinter for MshHelper {
    fn hint(&self, line: &str, pos: usize, ctx: &RLContext<'_>) -> Option<String> {
        self.2.hint(line, pos, ctx)
    }
}

impl Highlighter for MshHelper {
    fn highlight_prompt<'p>(&self, prompt: &'p str) -> Cow<'p, str> {
        Borrowed(prompt)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned(hint.dimmed().to_string())
    }

    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.1.highlight(line, pos)
    }

    fn highlight_char(&self, line: &str, pos: usize) -> bool {
        self.1.highlight_char(line, pos)
    }
}

impl Helper for MshHelper {}

impl Default for MshHelper {
    fn default() -> Self {
        Self(FilenameCompleter::default(), MatchingBracketHighlighter::default(), HistoryHinter {},)
    }
}

#[derive(Debug, Clone, PartialEq, Hash)]
pub(crate) enum Action {
    Loop,
    Dump,
    Buffer(String),
    Register(Vec<String>),
    Unregister(Vec<String>),
    RegisterFile(String),
    ClearRegistry(Vec<String>),
    ChDir(String),
    StoreEnv{ name: String, value: String },
    RemoveEnv{ name: String },
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
        let count = ctx.dir_count();
        prompt.push_str(&format!("({}) ", count).blue().to_string());
        prompt.push_str(&get_cwd()?.green().to_string());
        prompt.push_str("> ");

        trace!("prompt generated: {:?}", prompt);
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

fn init_editor() -> Editor<MshHelper> {
    debug!("Init Editor");
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .output_stream(OutputStreamType::Stdout)
        .build();
    let helper = MshHelper::default();
    let mut rl = Editor::with_config(config);
    rl.set_helper(Some(helper));

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
                match parser::handle_line(&mut ctx, &line) {
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
                    Action::ClearRegistry(v) => {
                        ctx.clear_registry();
                        context::register_paths(&mut ctx, &v);
                    }
                    Action::RegisterFile(s) => {
                        match context::read_registry_file(&s) {
                            Ok(v) => {
                                context::register_paths(&mut ctx, &v);
                            }
                            Err(e) => {
                                eprintln!("{}", e);
                            }
                        };
                    }
                    Action::Execute(v) => ctx.run_executable(&v),
                    Action::ChDir(p) => {
                        env::set_current_dir(p).unwrap_or_else(|e| {
                            println!("ChDir error: {}", e);
                        });
                    }
                    Action::StoreEnv{name, value} => env::set_var(name, value),
                    Action::RemoveEnv{name} => env::remove_var(name),
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
