//! A role's models: a body plus parts attached to its sockets (helmet, weapons), each with its
//! texture, and the motions they play. A game builds them from a [`Look`] ([`crate::Looks`]).

use c3_anim::Clip;
use c3_assets::model::Model;

/// What kind of role a look is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LookKind {
    #[default]
    Player,
    Monster,
    Npc,
    /// One of the game's models by id ([`Look::body`]), as it is.
    Model,
}

/// What a role looks like, in the game's ids.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub kind: LookKind,
    /// The game's body (players, monsters) or model (NPCs) id.
    pub body: u32,
    pub hair: u32,
    /// Item types worn, in any order.
    pub worn: Vec<u32>,
    /// Drawing size in percent.
    pub zoom_percent: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            kind: LookKind::Player,
            body: 0,
            hair: 0,
            worn: Vec::new(),
            zoom_percent: 100.0,
        }
    }
}

/// A model with the texture it is drawn with.
#[derive(Debug, Clone)]
pub struct PartModel {
    pub model: Model,
    pub texture: Option<String>,
    /// The game's key selecting the part's action effects ([`crate::Looks::part_effects`]);
    /// `None` for parts without any.
    pub effect_key: Option<u64>,
}

impl PartModel {
    pub fn plain(model: Model, texture: Option<String>) -> Self {
        Self {
            model,
            texture,
            effect_key: None,
        }
    }
}

/// A part attached to a body socket.
#[derive(Debug, Clone)]
pub struct Attachment {
    /// Socket part name in the body (e.g. `v_r_weapon`).
    pub socket: String,
    pub part: PartModel,
    /// The item the part shows (weapons have a motion of their own per action).
    pub item: Option<u32>,
}

/// Clips for one model (one per part), from a motion file.
pub type Motion = Vec<Clip>;

/// A character's motions for one action.
#[derive(Debug, Clone, Default)]
pub struct CharacterMotions {
    /// The game's body motion id, for display.
    pub body_id: Option<u64>,
    /// One per model: body first, then attachments (`None` = no motion of its own).
    pub models: Vec<Option<Motion>>,
}

#[derive(Debug, Clone)]
pub struct Character {
    pub body: PartModel,
    pub attachments: Vec<Attachment>,
    /// The game's body motion id in use, for display.
    pub motion_id: Option<u64>,
}

impl Character {
    /// A character of one model.
    pub fn single(body: PartModel) -> Self {
        Self {
            body,
            attachments: Vec::new(),
            motion_id: None,
        }
    }

    /// Switches to `motions` (from [`crate::Looks::motions`]).
    pub fn apply_motions(&mut self, motions: &CharacterMotions) {
        let models = std::iter::once(&mut self.body.model)
            .chain(self.attachments.iter_mut().map(|a| &mut a.part.model));
        for (model, motion) in models.zip(&motions.models) {
            if let Some(clips) = motion {
                model.apply_motion(clips);
            }
        }
        if motions.body_id.is_some() {
            self.motion_id = motions.body_id;
        }
    }
}
