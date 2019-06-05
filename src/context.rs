#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::multiple_crate_versions)]

use rayon::prelude::*;

use std::borrow::ToOwned;
use std::collections::HashSet;
use std::env;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::fs::File;
use std::io::prelude::*;
use std::mem;
use std::path::PathBuf;
use std::process::Command;
use std::string::ToString;

#[derive(Debug, Default, Clone, PartialEq, Hash, Builder)]
#[builder(setter(into))]
pub(crate) struct MshConfig {
    #[builder(default)]
    preload_dirs: Vec<String>,
}

impl MshConfig {
    pub const fn dirs(&self) -> &Vec<String> {
        &self.preload_dirs
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct Context {
    buffer: String,
    dir_registry: HashSet<PathBuf>,
}

impl Context {
    pub fn push_buffer(&mut self, push: &str) {
        debug!("Pushing line to buffer: {}", push);
        if log_enabled!(log::Level::Trace) {
            trace!("Current buffer contents: {}", self.buffer);
        };
        self.buffer.push_str(push);
        trace!("New buffer contents: {}", self.buffer);
    }

    pub fn take_buffer(&mut self, join_with: &str) -> String {
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

    pub fn register(&mut self, path: &PathBuf) -> Result<(PathBuf, bool), String> {
        let real_path = path.canonicalize().map_err(|e| e.to_string())?;
        Ok((real_path.clone(), self.dir_registry.insert(real_path)))
    }

    pub fn unregister(&mut self, path: &PathBuf) -> Result<(PathBuf, bool), String> {
        let real_path = path.canonicalize().map_err(|e| e.to_string())?;
        let was_there = self.dir_registry.remove(&real_path);
        Ok((real_path, was_there))
    }

    pub fn clear_registry(&mut self) {
        self.dir_registry.clear();
    }

    pub fn dir_count(&self) -> usize {
        self.dir_registry.len()
    }

    pub fn run_executable(&mut self, args: &[String]) {
        assert!(!args.is_empty());
        debug!("Execute command: {:?}", args);
        if log_enabled!(log::Level::Trace) {
            for arg in args.iter().by_ref() {
                trace!("Arg found: \"{}\"", arg);
            }

            for dir in &self.dir_registry {
                trace!("Registered directory: {}", dir.display());
            }
        };

        let empty = self.dir_registry.is_empty();

        if empty {
            let curdir = env::current_dir().expect("Current dir could not be read.");
            debug!(
                "no registered directories: executing against curdir: {}",
                curdir.display()
            );
            self.dir_registry.insert(curdir);
        }

        self.dir_registry
            .par_iter()
            .map(|p| run_executable(args, p))
            .for_each(drop);

        if empty {
            self.dir_registry.clear();
        }
    }
}

impl Display for Context {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        writeln!(formatter, "Registered directories:")?;
        for path in &self.dir_registry {
            writeln!(formatter, "{}", &path.display())?;
        }
        Ok(())
    }
}

fn run_executable(args: &[String], path: &PathBuf) {
    let raw_output = match Command::new(&args[0])
        .args(args.iter().skip(1))
        .current_dir(path)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "Could not execute process on dir: {}, failed with error: {}",
                path.display(),
                e
            );
            return;
        }
    };

    let output = String::from_utf8_lossy(&raw_output.stdout);
    if !output.trim().is_empty() {
        println!("{}:\n{}", &path.display(), output)
    };
}

pub(crate) fn get_home_dir() -> &'static str {
    lazy_static::lazy_static! {
        static ref HOME: String = dirs::home_dir()
            .expect("HOME or equivalent not set")
            .to_str()
            .expect("HOME or equivalent is not valid UTF-8")
            .into();
    }

    &HOME
}

fn get_boxed_file(name: &str) -> Result<Box<dyn Read>, String> {
    Ok(Box::new(File::open(name).map_err(|e| e.to_string())?))
}

pub(crate) fn read_registry_file(name: &str) -> Result<Vec<String>, String> {
    debug!("Searching for registry file: {}", name);
    let mut reader = get_boxed_file(name)?;
    trace!("File reader initialized");

    let mut buf = String::new();
    reader.read_to_string(&mut buf).map_err(|e| e.to_string())?;

    trace!("File read, raw contents: {}", &buf);

    Ok(buf.split_whitespace().map(ToOwned::to_owned).collect())
}

pub(crate) fn register_paths(ctx: &mut Context, paths: &[String]) {
    for path in paths {
        let (real_path, new) = match ctx.register(&PathBuf::from(&path)) {
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

pub(crate) fn unregister_paths(ctx: &mut Context, paths: &[String]) {
    for path in paths {
        let (real_path, new) = match ctx.unregister(&PathBuf::from(&path)) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("Cannot unregister path {}: {}", path, e);
                continue;
            }
        };

        if new {
            println!("Removed path from registry: {}", real_path.display());
        } else {
            println!(
                "Path not registered, cannot remove: {}",
                real_path.display()
            );
        };
    }
}
