//! What a running game tells a debugger about its blueprints: the nodes that ran, the values
//! their outputs took, and whether a breakpoint was reached. Compiled blueprints call
//! `__trace(script, node)` and `__watch(script, node, pin, value)`; outside a debugger (a
//! server, the formulas) those do nothing.

use rhai::{Dynamic, Engine};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

/// The most recent nodes kept (older ones fall off).
const RAN_KEPT: usize = 512;

/// A node of a blueprint: the blueprint's name and the node's id.
pub type NodeKey = (String, u32);

#[derive(Debug, Default)]
pub struct DebugLog {
    /// Nodes that ran, oldest first, with a count that grows with each (to fade old ones).
    pub ran: VecDeque<(NodeKey, u64)>,
    /// The last value each output took: (blueprint, node, pin) → as text.
    pub watches: BTreeMap<(String, u32, String), String>,
    /// Where to stop: the game pauses after a step reaching one.
    pub breakpoints: BTreeSet<NodeKey>,
    /// The breakpoint reached, until the debugger takes it.
    pub hit: Option<NodeKey>,
    count: u64,
}

impl DebugLog {
    fn trace(&mut self, script: &str, node: u32) {
        self.count += 1;
        let key = (script.to_owned(), node);
        if self.breakpoints.contains(&key) {
            self.hit = Some(key.clone());
        }
        self.ran.push_back((key, self.count));
        while self.ran.len() > RAN_KEPT {
            self.ran.pop_front();
        }
    }

    /// The trace count now, to tell how long ago a node ran.
    pub fn now(&self) -> u64 {
        self.count
    }

    /// When node `node` of `script` last ran (its count), if it did.
    pub fn last_ran(&self, script: &str, node: u32) -> Option<u64> {
        (self.ran.iter().rev())
            .find(|((s, n), _)| s == script && *n == node)
            .map(|(_, at)| *at)
    }
}

/// `__trace` and `__watch` doing nothing (every engine has them, so blueprints always run).
pub(crate) fn register_quiet(engine: &mut Engine) {
    engine.register_fn("__trace", |_: &str, _: i64| {});
    engine.register_fn("__watch", |_: &str, _: i64, _: &str, _: Dynamic| {});
}

/// `__trace` and `__watch` writing to `log`.
pub(crate) fn register(engine: &mut Engine, log: &Rc<RefCell<DebugLog>>) {
    let l = log.clone();
    engine.register_fn("__trace", move |script: &str, node: i64| {
        l.borrow_mut().trace(script, node as u32);
    });
    let l = log.clone();
    engine.register_fn(
        "__watch",
        move |script: &str, node: i64, pin: &str, value: Dynamic| {
            l.borrow_mut().watches.insert(
                (script.to_owned(), node as u32, pin.to_owned()),
                value.to_string(),
            );
        },
    );
}
