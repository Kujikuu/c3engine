//! A dialogue in progress: the page shown and how picking an answer moves it on. Hosts (the
//! client, the server, the Studio's Play mode) keep one per player talking to an NPC.

use crate::dialog::{DialogPage, Effect, NpcInfo, PlayerState};
use crate::host::{ScriptError, ScriptHost, TALK};

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
