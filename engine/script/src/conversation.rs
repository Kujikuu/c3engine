//! A dialogue in progress: the page shown and how picking an answer moves it on. Hosts (the
//! client, the server, the Studio's Play mode) keep one per player talking to an NPC.

use crate::dialog::{DialogPage, Effect, NpcInfo, PlayerState};
use crate::host::{EVENTS_SCRIPT, ScriptError, ScriptHost, TALK};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Conversation {
    pub script: String,
    pub npc: NpcInfo,
    /// What the NPC says now and the answers offered.
    pub page: DialogPage,
}

/// A dialogue step's outcome: the conversation if it goes on, and what the script did.
pub type Step = (Option<Conversation>, Vec<Effect>);

impl Conversation {
    /// Talks to `npc`, running `talk()` of `script`.
    pub fn start(
        host: &mut ScriptHost,
        script: &str,
        npc: NpcInfo,
        player: &mut PlayerState,
    ) -> Result<Step, ScriptError> {
        Self::step(host, script, TALK, npc, player)
    }

    /// Picks answer `index` of the page: runs its function, or ends the conversation.
    pub fn choose(
        self,
        host: &mut ScriptHost,
        index: usize,
        player: &mut PlayerState,
    ) -> Result<Step, ScriptError> {
        match self.page.options.get(index).and_then(|o| o.next.as_deref()) {
            Some(next) => Self::step(host, &self.script, next, self.npc, player),
            None => Ok((None, Vec::new())),
        }
    }

    /// Runs event `function(args...)` of the events script for `player`; a page it builds
    /// opens as a dialogue (`npc` names who speaks).
    pub fn event(
        host: &mut ScriptHost,
        function: &str,
        args: Vec<rhai::Dynamic>,
        npc: NpcInfo,
        player: &mut PlayerState,
    ) -> Result<Step, ScriptError> {
        Self::event_in(host, EVENTS_SCRIPT, function, args, npc, player)
    }

    /// Runs event `function(args...)` of `script` (the events script, or a blueprint's).
    pub fn event_in(
        host: &mut ScriptHost,
        script: &str,
        function: &str,
        args: Vec<rhai::Dynamic>,
        npc: NpcInfo,
        player: &mut PlayerState,
    ) -> Result<Step, ScriptError> {
        let reply = host.run_with(script, function, args, &npc, player)?;
        let next = (!reply.page.is_empty()).then(|| Conversation {
            script: script.to_owned(),
            npc,
            page: reply.page,
        });
        Ok((next, reply.effects))
    }

    fn step(
        host: &mut ScriptHost,
        script: &str,
        function: &str,
        npc: NpcInfo,
        player: &mut PlayerState,
    ) -> Result<Step, ScriptError> {
        let reply = host.run(script, function, &npc, player)?;
        let next = (!reply.page.is_empty()).then(|| Conversation {
            script: script.to_owned(),
            npc,
            page: reply.page,
        });
        Ok((next, reply.effects))
    }
}
