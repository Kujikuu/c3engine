//! c3-mcp: a C3Engine project's tools for AI agents, served over MCP on stdio.
//!
//! `c3-mcp <project>` opens the project on its own (headless); `c3-mcp --live [host:port]`
//! drives the Studio that has it open (Window > Live link; 127.0.0.1:7390 by default), so
//! changes show there at once and undo there. The tools are `c3-agent`'s, the same in both.
//!
//! For Claude Code: `claude mcp add c3 -- c3-mcp D:/Projects/MyGame`.

use c3_agent::{Headless, live};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::Value;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

/// A tool call for the thread that holds the project, and where its answer goes.
type Call = (String, Value, Sender<Result<Value, String>>);

/// Where tool calls run.
#[derive(Clone)]
enum Backend {
    /// A project open on its own thread (its documents aren't `Send`).
    Headless(Sender<Call>),
    /// The Studio listening at this address.
    Live(String),
}

impl Backend {
    fn run(&self, tool: String, args: Value) -> Result<Value, String> {
        match self {
            Self::Headless(calls) => {
                let (reply, answer) = channel();
                calls
                    .send((tool, args, reply))
                    .map_err(|_| "the project closed")?;
                answer.recv().map_err(|_| "the project closed".to_owned())?
            }
            Self::Live(addr) => live::call(addr, &tool, &args),
        }
    }
}

/// Opens `path` on a thread of its own, which runs the calls sent to it.
fn open_headless(path: &str) -> Result<Sender<Call>, String> {
    let (calls, receive) = channel::<Call>();
    let (opened, wait) = channel();
    let path = path.to_owned();
    std::thread::spawn(move || {
        let open = || -> Result<Headless, String> {
            let project = c3_project::Project::open(&path).map_err(|e| e.to_string())?;
            let (game, problems) = c3_games::open_project(&project)?;
            problems.iter().for_each(|p| eprintln!("c3-mcp: {p}"));
            Ok(Headless::new(project, game))
        };
        let mut ws = match open() {
            Ok(ws) => {
                let _ = opened.send(Ok(()));
                ws
            }
            Err(e) => {
                let _ = opened.send(Err(e));
                return;
            }
        };
        for (tool, args, reply) in receive {
            let _ = reply.send(c3_agent::call(&mut ws, &tool, &args));
        }
    });
    wait.recv()
        .map_err(|_| "the project thread stopped".to_owned())??;
    Ok(calls)
}

#[derive(Clone)]
struct Server {
    backend: Backend,
    tools: Arc<Vec<Tool>>,
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        let mode = match &self.backend {
            Backend::Headless(_) => "a project opened on its own".to_owned(),
            Backend::Live(addr) => format!("the Studio at {addr}"),
        };
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("c3-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
                "C3Engine game project tools, working on {mode}. Call project_overview first \
                 and read_docs agents for how to work; every change is undoable and reaches \
                 the files only with save."
            ))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.to_vec()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let backend = self.backend.clone();
        let tool = request.name.to_string();
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = tokio::task::spawn_blocking(move || backend.run(tool, args))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        Ok(match result {
            Ok(Value::String(text)) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Ok(value) => {
                let text = serde_json::to_string_pretty(&value).unwrap_or_default();
                CallToolResult::success(vec![ContentBlock::text(text)])
            }
            Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]),
        }
        .into())
    }
}

fn tools() -> Vec<Tool> {
    (c3_agent::tools().into_iter())
        .map(|spec| {
            let schema = match spec.input_schema {
                Value::Object(map) => map,
                _ => Default::default(),
            };
            Tool::new(spec.name, spec.description, Arc::new(schema))
        })
        .collect()
}

fn backend() -> Result<Backend, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--live") => {
            let addr = args.get(1).map_or(live::DEFAULT_ADDR, String::as_str);
            Ok(Backend::Live(addr.to_owned()))
        }
        Some(project) if !project.starts_with("--") => {
            Ok(Backend::Headless(open_headless(project)?))
        }
        _ => Err("usage: c3-mcp <project folder> | c3-mcp --live [host:port]".into()),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let backend = match backend() {
        Ok(backend) => backend,
        Err(e) => {
            eprintln!("c3-mcp: {e}");
            std::process::exit(2);
        }
    };
    let server = Server {
        backend,
        tools: Arc::new(tools()),
    };
    let served = match server.serve(rmcp::transport::stdio()).await {
        Ok(service) => service
            .waiting()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    if let Err(e) = served {
        eprintln!("c3-mcp: {e}");
        std::process::exit(1);
    }
}
