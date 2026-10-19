//! The engine's own animation sets, `.c3char`: a glTF model as one of the game's looks, which
//! of its animations each action plays (how fast, with what sound), and models attached to its
//! parts (a sword in a hand).
//!
//! ```toml
//! model = "models/hero.gltf"
//! look = 1             # roles whose body is look 1 wear it…
//! role = "player"      # …players only (or "monster", "npc"; none: any role)
//! frame_ms = 33        # time per animation frame, unless an action says
//!
//! [actions.stand]
//! animation = "idle"
//! [actions.walk]
//! animation = "walk"
//! sound = "sounds/step.wav"
//! [actions.attack]
//! animation = "slash"
//! frame_ms = 25
//!
//! [[attachments]]
//! socket = "hand"     # the mesh it follows, held at that mesh's centre
//! model = "models/sword.gltf"
//! ```

use crate::character::{Attachment, Character, Look, LookKind, PartModel};
use crate::module::Act;
use crate::native::NativeAssets;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterFile {
    #[serde(default)]
    pub model: String,
    /// The look (body) id roles wear it by.
    #[serde(default)]
    pub look: u32,
    /// Which roles wear it; none: any role with the look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(default = "default_frame_ms")]
    pub frame_ms: u32,
    /// What each action plays, by name (`stand`, `walk`, `run`, `attack`, `die`, `jump`).
    #[serde(default)]
    pub actions: BTreeMap<String, ActionFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentFile>,
}

impl Default for CharacterFile {
    fn default() -> Self {
        Self {
            model: String::new(),
            look: 0,
            role: None,
            frame_ms: default_frame_ms(),
            actions: BTreeMap::new(),
            attachments: Vec::new(),
        }
    }
}

/// The kind of role a set dresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Player,
    Monster,
    Npc,
}

impl Role {
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Player, "Players"),
        (Self::Monster, "Monsters"),
        (Self::Npc, "NPCs"),
    ];

    /// Whether a look of `kind` is this role.
    pub fn fits(self, kind: LookKind) -> bool {
        matches!(
            (self, kind),
            (Self::Player, LookKind::Player | LookKind::Showcase)
                | (Self::Monster, LookKind::Monster)
                | (Self::Npc, LookKind::Npc)
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionFile {
    /// The model's animation, by name.
    #[serde(default)]
    pub animation: String,
    /// Time per frame; none: the set's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_ms: Option<u32>,
    /// Played when the action starts; none for silence.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sound: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentFile {
    /// The body part it follows.
    pub socket: String,
    pub model: String,
}

fn default_frame_ms() -> u32 {
    33
}

/// The name an action goes by in a set.
pub fn act_name(act: Act) -> &'static str {
    match act {
        Act::Stand => "stand",
        Act::Walk => "walk",
        Act::Run => "run",
        Act::RunAlt => "run_alt",
        Act::Attack => "attack",
        Act::Die => "die",
        Act::Jump => "jump",
    }
}

/// Every action a set can name, in order.
pub const ACTS: [Act; 7] = [
    Act::Stand,
    Act::Walk,
    Act::Run,
    Act::RunAlt,
    Act::Attack,
    Act::Die,
    Act::Jump,
];

pub fn parse(text: &str) -> Result<CharacterFile, String> {
    toml::from_str(text).map_err(|e| e.message().to_owned())
}

pub fn write(file: &CharacterFile) -> String {
    toml::to_string_pretty(file).unwrap_or_default()
}

impl CharacterFile {
    /// Whether roles looking like `look` wear this set.
    pub fn dresses(&self, look: &Look) -> bool {
        self.look != 0 && self.look == look.body && self.role.is_none_or(|r| r.fits(look.kind))
    }

    /// What `act` plays: its own entry, else the stand's (a set needs only that).
    pub fn action(&self, act: Act) -> Option<&ActionFile> {
        (self.actions.get(act_name(act))).or_else(|| self.actions.get(act_name(Act::Stand)))
    }

    pub fn frame_ms(&self, act: Act) -> u32 {
        self.action(act)
            .and_then(|a| a.frame_ms)
            .unwrap_or(self.frame_ms)
            .max(1)
    }

    /// The character posed for `act`: the model playing that action's animation, with its
    /// attachments.
    pub fn build(&self, act: Act, assets: &NativeAssets) -> Result<Character, String> {
        let imported = assets.model(&self.model)?;
        let wanted = self
            .action(act)
            .map(|a| a.animation.as_str())
            .unwrap_or_default();
        // The named animation, else the model's first.
        let animation = (imported.animations.iter())
            .find(|a| a.name == wanted)
            .or(imported.animations.first());
        let mut model = imported.model.clone();
        for (n, part) in model.parts.iter_mut().enumerate() {
            part.clip = animation.and_then(|a| a.clips.get(n).cloned().flatten());
        }
        let mut attachments = Vec::new();
        for a in &self.attachments {
            let socket = (model.parts.iter())
                .find(|p| mesh_named(&p.mesh.name, &a.socket))
                .ok_or_else(|| format!("{} has no part \"{}\"", self.model, a.socket))?;
            let (lo, hi) = socket.mesh.bounds;
            let mut held = assets.model(&a.model)?.model.clone();
            for part in &mut held.parts {
                let mesh = &mut part.mesh;
                for v in &mut mesh.vertices {
                    v.position += (lo + hi) / 2.0;
                }
                mesh.bounds = (
                    mesh.bounds.0 + (lo + hi) / 2.0,
                    mesh.bounds.1 + (lo + hi) / 2.0,
                );
            }
            attachments.push(Attachment {
                socket: socket.mesh.name.clone(),
                part: part_model(held),
                item: None,
            });
        }
        let mut character = Character::single(part_model(model));
        character.attachments = attachments;
        Ok(character)
    }
}

/// Whether glTF part `part` (`<mesh>.<primitive>`) is mesh `name`'s.
fn mesh_named(part: &str, name: &str) -> bool {
    part.eq_ignore_ascii_case(name)
        || (part.rsplit_once('.')).is_some_and(|(mesh, _)| mesh.eq_ignore_ascii_case(name))
}

/// `model` drawn with its first texture, in its material's colour.
fn part_model(model: c3_assets::model::Model) -> PartModel {
    let texture = (model.parts.iter())
        .map(|p| p.mesh.texture.clone())
        .find(|t| !t.is_empty());
    let color = (model.parts.iter())
        .find_map(|p| p.mesh.vertices.first())
        .map_or(u32::MAX, |v| v.color);
    let tint = [16, 8, 0, 24].map(|shift| ((color >> shift) & 0xff) as f32 / 255.0);
    PartModel {
        tint,
        ..PartModel::plain(model, texture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_read_write_and_fall_back_to_stand() {
        let file = parse(
            r#"
            model = "models/hero.gltf"
            look = 7
            [actions.stand]
            animation = "idle"
            [actions.walk]
            animation = "walk"
            frame_ms = 20
            sound = "sounds/step.wav"
            "#,
        )
        .unwrap();
        assert_eq!(file.look, 7);
        assert_eq!(file.frame_ms(Act::Walk), 20);
        assert_eq!(file.frame_ms(Act::Stand), 33);
        assert_eq!(file.action(Act::Attack).unwrap().animation, "idle");
        assert_eq!(parse(&write(&file)).unwrap(), file);
        assert!(parse("shine = 1").is_err());
    }
}
