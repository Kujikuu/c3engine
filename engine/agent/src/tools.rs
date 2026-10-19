//! The tools, as an MCP server lists them, and running one by name.

use crate::workspace::Workspace;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// A tool as clients see it: its name, what it does and the JSON schema of its arguments.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// What a tool does with its arguments.
type Run = fn(&mut dyn Workspace, &Value) -> Result<Value, String>;

/// A tool and how it runs.
pub(crate) struct Tool {
    pub spec: ToolSpec,
    pub run: Run,
}

/// A tool taking the arguments `properties` describes (`required` must be given).
pub(crate) fn tool(
    name: &'static str,
    description: &'static str,
    properties: Value,
    required: &[&str],
    run: Run,
) -> Tool {
    Tool {
        spec: ToolSpec {
            name,
            description,
            input_schema: json!({
                "type": "object",
                "properties": properties,
                "required": required,
            }),
        },
        run,
    }
}

fn all() -> Vec<Tool> {
    [
        crate::context::tools(),
        crate::maps::tools(),
        crate::data::tools(),
        crate::content::tools(),
        crate::assets::tools(),
        crate::playtest::tools(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Every tool.
pub fn tools() -> Vec<ToolSpec> {
    all().into_iter().map(|t| t.spec).collect()
}

/// Runs tool `name` with `args` on `ws`: its answer, or what went wrong.
pub fn call(ws: &mut dyn Workspace, name: &str, args: &Value) -> Result<Value, String> {
    let tool = (all().into_iter())
        .find(|t| t.spec.name == name)
        .ok_or_else(|| format!("no tool \"{name}\""))?;
    (tool.run)(ws, args)
}

/// Argument `name` of `args`.
pub(crate) fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    let value = args
        .get(name)
        .ok_or_else(|| format!("missing argument \"{name}\""))?;
    serde_json::from_value(value.clone()).map_err(|e| format!("argument \"{name}\": {e}"))
}

/// Argument `name` of `args`, when given.
pub(crate) fn opt_arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<Option<T>, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => arg(args, name).map(Some),
    }
}

/// `value` as a tool's answer.
pub(crate) fn answer(value: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// What a list of commands did: each outcome or refusal, in order.
pub(crate) fn outcomes<E: std::fmt::Display>(
    results: Vec<Result<c3_edit::Outcome, E>>,
) -> Result<Value, String> {
    let done: Vec<Value> = (results.into_iter())
        .map(|r| match r {
            Ok(o) => json!({ "done": o.summary, "changed": o.changed }),
            Err(e) => json!({ "refused": e.to_string() }),
        })
        .collect();
    Ok(json!({ "results": done }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_have_distinct_names_and_object_schemas() {
        let specs = tools();
        let mut names: Vec<&str> = specs.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), specs.len());
        for spec in &specs {
            assert_eq!(spec.input_schema["type"], "object", "{}", spec.name);
            assert!(!spec.description.is_empty(), "{}", spec.name);
        }
    }
}
