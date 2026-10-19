//! The live link: an agent's tool calls carried to a running Studio, which runs them on what
//! it has open (so they show at once and undo with Ctrl+Z). One JSON line per call over a
//! local TCP connection: `{"tool": name, "args": {...}}`, answered by `{"ok": value}` or
//! `{"error": text}`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

/// Where a Studio listens unless told otherwise.
pub const DEFAULT_ADDR: &str = "127.0.0.1:7390";
/// Longest a call may wait for the Studio (a modal dialog may hold it up).
const CALL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    tool: String,
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Response {
    Ok(Value),
    Error(String),
}

/// Runs tool `tool` with `args` in the Studio listening at `addr`.
pub fn call(addr: &str, tool: &str, args: &Value) -> Result<Value, String> {
    let unreachable = |e: std::io::Error| {
        format!("can't reach the Studio at {addr} ({e}); is it open with Live link on?")
    };
    let mut stream = TcpStream::connect(addr).map_err(unreachable)?;
    stream
        .set_read_timeout(Some(CALL_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let request = Request {
        tool: tool.to_owned(),
        args: args.clone(),
    };
    let mut line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(unreachable)?;
    let mut answer = String::new();
    BufReader::new(stream)
        .read_line(&mut answer)
        .map_err(|e| format!("the Studio didn't answer: {e}"))?;
    match serde_json::from_str(&answer).map_err(|e| format!("the Studio's answer: {e}"))? {
        Response::Ok(value) => Ok(value),
        Response::Error(e) => Err(e),
    }
}

/// A tool call waiting for the Studio to run it.
pub struct Pending {
    pub tool: String,
    pub args: Value,
    reply: Sender<Result<Value, String>>,
}

impl Pending {
    /// Answers the call.
    pub fn reply(self, result: Result<Value, String>) {
        let _ = self.reply.send(result);
    }
}

/// The Studio's end: calls arrive on a background thread and wait in [`Listener::next`] for
/// the Studio's frame to run them.
pub struct Listener {
    calls: Receiver<Pending>,
    pub addr: String,
}

impl Listener {
    /// Listens at `addr`; `wake` is called when a call arrives (to redraw the Studio).
    pub fn start(addr: &str, wake: impl Fn() + Send + Sync + 'static) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let addr = listener.local_addr()?.to_string();
        let (send, calls) = channel();
        let wake = std::sync::Arc::new(wake);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (send, wake) = (send.clone(), wake.clone());
                std::thread::spawn(move || serve(stream, &send, &*wake));
            }
        });
        Ok(Self { calls, addr })
    }

    /// The next call waiting, if any.
    pub fn next(&self) -> Option<Pending> {
        self.calls.try_recv().ok()
    }
}

/// Carries the calls of one connection to the Studio and their answers back.
fn serve(stream: TcpStream, calls: &Sender<Pending>, wake: &dyn Fn()) {
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => {
                let (reply, answer) = channel();
                let pending = Pending {
                    tool: request.tool,
                    args: request.args,
                    reply,
                };
                if calls.send(pending).is_err() {
                    return;
                }
                wake();
                match answer.recv() {
                    Ok(Ok(value)) => Response::Ok(value),
                    Ok(Err(e)) => Response::Error(e),
                    Err(_) => Response::Error("the Studio closed".into()),
                }
            }
            Err(e) => Response::Error(format!("bad request: {e}")),
        };
        let mut text = serde_json::to_string(&response).unwrap_or_default();
        text.push('\n');
        if out.write_all(text.as_bytes()).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_reach_the_listener_and_answers_come_back() {
        let listener = Listener::start("127.0.0.1:0", || {}).unwrap();
        let addr = listener.addr.clone();
        let studio = std::thread::spawn(move || {
            loop {
                if let Some(call) = listener.next() {
                    let answer = match call.tool.as_str() {
                        "echo" => Ok(call.args.clone()),
                        _ => Err(format!("no tool \"{}\"", call.tool)),
                    };
                    let last = call.tool != "echo";
                    call.reply(answer);
                    if last {
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let args = serde_json::json!({ "x": 1 });
        assert_eq!(call(&addr, "echo", &args).unwrap(), args);
        assert_eq!(call(&addr, "nope", &args).unwrap_err(), "no tool \"nope\"");
        studio.join().unwrap();
    }
}
