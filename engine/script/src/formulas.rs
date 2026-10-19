//! A game's formulas as a script: `scripts/rules.rhai` defines any of the functions the game's
//! rules ask for (`hit`, `kill_exp`, …) and the engine's own formula answers the others.
//! Values go in and out as numbers and maps of numbers; formulas run in the same sandbox as
//! NPC scripts (bounded work, no files).

use crate::host::sandboxed_engine;
use rhai::{AST, Dynamic, Engine, FuncArgs, Map, Scope};
use std::path::Path;

/// File of a project's formulas, in its `scripts/`.
pub const RULES_SCRIPT: &str = "rules.rhai";

pub struct Formulas {
    engine: Engine,
    ast: AST,
}

impl Formulas {
    /// The formulas in `file`; `None` when there is no such file.
    pub fn load(file: &Path) -> Result<Option<Self>, String> {
        if !file.exists() {
            return Ok(None);
        }
        let source = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        Self::compile(&source).map(Some)
    }

    /// The formulas of a project's `scripts` folder: `rules.rhai` and those its blueprints
    /// make (`graphs/*.rhai`); `None` when there are none.
    pub fn load_dir(scripts: &Path) -> Result<Option<Self>, String> {
        let mut files = vec![scripts.join(RULES_SCRIPT)];
        let graphs = scripts.join(crate::blueprint::GRAPHS_DIR);
        files.extend(
            crate::host::names_with_suffix(&graphs, crate::SCRIPT_SUFFIX)
                .iter()
                .map(|name| graphs.join(format!("{name}{}", crate::SCRIPT_SUFFIX))),
        );
        let engine = sandboxed_engine();
        let mut merged: Option<AST> = None;
        for file in files.iter().filter(|f| f.is_file()) {
            let source = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
            let ast = (engine.compile(&source)).map_err(|e| format!("{}: {e}", file.display()))?;
            merged = Some(match merged {
                Some(all) => all.merge(&ast),
                None => ast,
            });
        }
        Ok(merged.map(|ast| Self { engine, ast }))
    }

    /// Formulas from `source`.
    pub fn compile(source: &str) -> Result<Self, String> {
        let engine = sandboxed_engine();
        let ast = engine.compile(source).map_err(|e| e.to_string())?;
        Ok(Self { engine, ast })
    }

    /// Whether the script defines `name`.
    pub fn has(&self, name: &str) -> bool {
        self.ast.iter_functions().any(|f| f.name == name)
    }

    /// `name(args)` as a whole number.
    pub fn int(&self, name: &str, args: impl FuncArgs) -> Result<i64, String> {
        let value = self.call(name, args)?;
        value
            .as_int()
            .or_else(|_| value.as_float().map(|f| f.round() as i64))
            .map_err(|kind| format!("{name}() gave {kind}, not a number"))
    }

    /// `name(args)` as a map (named numbers).
    pub fn map(&self, name: &str, args: impl FuncArgs) -> Result<Map, String> {
        let value = self.call(name, args)?;
        let kind = value.type_name();
        value
            .try_cast::<Map>()
            .ok_or_else(|| format!("{name}() gave {kind}, not a map"))
    }

    fn call(&self, name: &str, args: impl FuncArgs) -> Result<Dynamic, String> {
        self.engine
            .call_fn::<Dynamic>(&mut Scope::new(), &self.ast, name, args)
            .map_err(|e| format!("{name}(): {e}"))
    }
}

/// The whole number `key` of `map` (0 when missing).
pub fn map_int(map: &Map, key: &str) -> i64 {
    map.get(key).map_or(0, |v| {
        v.as_int()
            .or_else(|_| v.as_float().map(|f| f.round() as i64))
            .unwrap_or(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_answer_with_numbers_and_maps() {
        let f = Formulas::compile(
            r#"
            fn hit(a, t) { a.roll * 2 - t.defence }
            fn player_stats(level) { #{ max_life: 50 + level * 10, attack_min: 3.6 } }
            "#,
        )
        .unwrap();
        assert!(f.has("hit") && !f.has("kill_exp"));
        let mut a = Map::new();
        a.insert("roll".into(), 10_i64.into());
        let mut t = Map::new();
        t.insert("defence".into(), 4_i64.into());
        assert_eq!(f.int("hit", (a, t)), Ok(16));
        let stats = f.map("player_stats", (2_i64,)).unwrap();
        assert_eq!(
            (map_int(&stats, "max_life"), map_int(&stats, "attack_min")),
            (70, 4)
        );
        assert!(f.int("player_stats", (2_i64,)).is_err());
    }
}
