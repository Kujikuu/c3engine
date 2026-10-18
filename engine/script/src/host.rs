//! The script host: compiles the project's `scripts/*.rhai`, keeps them until their file
//! changes, and runs dialogue steps in a sandbox (bounded work, no file or system access).

use crate::api::{self, Call, Shared};
use crate::dialog::{NpcInfo, PlayerState, Reply};
use crate::tables::Tables;
use crate::{Diagnostic, SCRIPT_SUFFIX, Severity};
use rhai::{AST, Engine, EvalAltResult, ParseError, Position, Scope};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

/// The function run when the player talks to an NPC.
pub const TALK: &str = "talk";

/// Most steps one call may run (stops endless loops).
const MAX_OPERATIONS: u64 = 500_000;
const MAX_CALL_DEPTH: usize = 32;
const MAX_STRING: usize = 64 * 1024;
const MAX_COLLECTION: usize = 10_000;

/// A script that failed to load or run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    /// Script name (`npc/guard`).
    pub script: String,
    pub diagnostic: Diagnostic,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let d = &self.diagnostic;
        if d.line > 0 {
            write!(f, "{}:{}:{}: {}", self.script, d.line, d.column, d.message)
        } else {
            write!(f, "{}: {}", self.script, d.message)
        }
    }
}

impl std::error::Error for ScriptError {}

struct Compiled {
    modified: Option<SystemTime>,
    ast: AST,
}

pub struct ScriptHost {
    engine: Engine,
    dir: PathBuf,
    compiled: HashMap<String, Compiled>,
    call: Shared,
    tables: Rc<RefCell<Tables>>,
}

impl ScriptHost {
    /// A host for the scripts in `dir`, reading tables from `tables` (if the game has any).
    pub fn new(dir: impl Into<PathBuf>, tables: Option<PathBuf>) -> Self {
        let mut engine = Engine::new();
        engine.set_max_operations(MAX_OPERATIONS);
        engine.set_max_call_levels(MAX_CALL_DEPTH);
        engine.set_max_string_size(MAX_STRING);
        engine.set_max_array_size(MAX_COLLECTION);
        engine.set_max_map_size(MAX_COLLECTION);
        engine.disable_symbol("eval");
        let call = Shared::default();
        let tables = Rc::new(RefCell::new(Tables::new(tables)));
        api::register(&mut engine, &call, &tables);
        Self {
            engine,
            dir: dir.into(),
            compiled: HashMap::new(),
            call,
            tables,
        }
    }

    /// The engine, for a game to register its own script functions.
    pub fn engine_mut(&mut self) -> &mut Engine {
        self.compiled.clear();
        &mut self.engine
    }

    /// The file of script `name` (`npc/guard` → `scripts/npc/guard.rhai`).
    pub fn script_file(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}{SCRIPT_SUFFIX}"))
    }

    /// Forgets loaded tables (after they were saved), so scripts read them anew.
    pub fn reload_tables(&mut self) {
        self.tables.borrow_mut().clear();
    }

    /// Problems in `source` without running it: syntax errors, and a warning when
    /// `entry` is not defined.
    pub fn check(&self, source: &str, entry: Option<&str>) -> Vec<Diagnostic> {
        match self.engine.compile(source) {
            Err(e) => vec![parse_diagnostic(&e)],
            Ok(ast) => match entry {
                Some(entry) if !defines(&ast, entry) => vec![Diagnostic {
                    severity: Severity::Warning,
                    line: 0,
                    column: 0,
                    message: format!("no `fn {entry}()`: nothing runs when the player talks"),
                }],
                _ => Vec::new(),
            },
        }
    }

    /// Runs `talk()` of `script` for `player` speaking to `npc`.
    pub fn talk(
        &mut self,
        script: &str,
        npc: &NpcInfo,
        player: &mut PlayerState,
    ) -> Result<Reply, ScriptError> {
        self.run(script, TALK, npc, player)
    }

    /// Runs `function()` of `script` (a dialogue answer's step). `player` keeps the
    /// changes the script made up to an error.
    pub fn run(
        &mut self,
        script: &str,
        function: &str,
        npc: &NpcInfo,
        player: &mut PlayerState,
    ) -> Result<Reply, ScriptError> {
        let error = |diagnostic| ScriptError {
            script: script.to_owned(),
            diagnostic,
        };
        self.load(script).map_err(error)?;
        let ast = &self.compiled[script].ast;
        if !defines(ast, function) {
            return Err(error(Diagnostic::error(format!(
                "no `fn {function}()` in this script"
            ))));
        }
        *self.call.borrow_mut() = Some(Call {
            npc: npc.clone(),
            player: std::mem::take(player),
            page: Default::default(),
            effects: Vec::new(),
        });
        let result = self
            .engine
            .call_fn::<rhai::Dynamic>(&mut Scope::new(), ast, function, ());
        let call = self.call.borrow_mut().take().expect("call set above");
        *player = call.player;
        // What the function returns is unused.
        if let Err(e) = result {
            return Err(error(eval_diagnostic(&e)));
        }
        Ok(Reply {
            page: call.page,
            effects: call.effects,
        })
    }

    /// Compiles `script` unless the compiled copy is as new as its file.
    fn load(&mut self, script: &str) -> Result<(), Diagnostic> {
        if script.is_empty() || script.split(['/', '\\']).any(|p| p == ".." || p.is_empty()) {
            return Err(Diagnostic::error(format!(
                "\"{script}\" is not a script name"
            )));
        }
        let file = self.script_file(script);
        let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
        if let Some(c) = self.compiled.get(script)
            && c.modified == modified
            && modified.is_some()
        {
            return Ok(());
        }
        let source = std::fs::read_to_string(&file)
            .map_err(|e| Diagnostic::error(format!("{}: {e}", file.display())))?;
        let ast = self
            .engine
            .compile(&source)
            .map_err(|e| parse_diagnostic(&e))?;
        self.compiled
            .insert(script.to_owned(), Compiled { modified, ast });
        Ok(())
    }
}

/// Whether `ast` defines `name()` taking no arguments.
fn defines(ast: &AST, name: &str) -> bool {
    ast.iter_functions()
        .any(|f| f.name == name && f.params.is_empty())
}

fn at(pos: Position) -> (usize, usize) {
    (pos.line().unwrap_or(0), pos.position().unwrap_or(0))
}

fn parse_diagnostic(e: &ParseError) -> Diagnostic {
    let (line, column) = at(e.position());
    Diagnostic {
        severity: Severity::Error,
        line,
        column,
        message: e.err_type().to_string(),
    }
}

/// The innermost error of a failed call, with where in the script it happened.
fn eval_diagnostic(e: &EvalAltResult) -> Diagnostic {
    if let EvalAltResult::ErrorInFunctionCall(_, _, inner, _) = e {
        return eval_diagnostic(inner);
    }
    let (line, column) = at(e.position());
    let mut message = e.to_string();
    // The message ends with "(line 3, position 5)"; the diagnostic carries that already.
    if let Some(i) = message.rfind(" (line ") {
        message.truncate(i);
    }
    Diagnostic {
        severity: Severity::Error,
        line,
        column,
        message,
    }
}

/// Script names under `dir` (`npc/guard` for `dir/npc/guard.rhai`), sorted.
pub fn script_names(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                walk(&path, &format!("{prefix}{name}/"), out);
            } else if let Some(stem) = name.strip_suffix(SCRIPT_SUFFIX) {
                out.push(format!("{prefix}{stem}"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out.sort();
    out
}
