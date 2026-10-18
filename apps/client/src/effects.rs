//! 3D effects placed on the map: the map's own (fires, fountains, glows), which play
//! forever like the client's `C3DMapEffect`s, and one-shots such as the click marker.

use c3_assets::effect::{Effect, Playback};
use c3_render::effect::{self, EffectMesh, EffectPlayer};
use c3_render::{Renderer, TextureId};
use c3_scene::{CellPos, EntityKind, Scene};
use game_conquer::Content;
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::rc::Rc;

/// One effect standing on the map.
pub struct MapEffect {
    pub cell: CellPos,
    effect: Rc<Effect>,
    player: EffectPlayer,
    placement: Mat4,
}

impl MapEffect {
    pub fn effect(&self) -> &Effect {
        &self.effect
    }

    /// Plays forever (`CGame3DEffectEx::EverPlay`).
    pub fn endless(mut self) -> Self {
        self.player = self.player.endless();
        self
    }

    pub fn is_finished(&self, now_ms: u64) -> bool {
        self.player.playback(&self.effect, now_ms) == Playback::Finished
    }

    /// World position of the effect's origin.
    pub fn position(&self) -> Vec3 {
        self.placement.w_axis.truncate()
    }

    /// This frame's geometry.
    pub fn meshes(&mut self, now_ms: u64, eye: Vec3) -> Vec<EffectMesh> {
        self.player
            .meshes(&self.effect, now_ms, self.placement, eye)
    }
}

/// Where an effect stands: world `position`, rotated in place (radians about X, Y, Z)
/// and scaled.
#[derive(Debug, Clone, Copy)]
pub struct Spot {
    pub position: (f32, f32),
    pub rotation: Vec3,
    pub scale: Vec3,
}

impl Spot {
    pub fn at(position: (f32, f32)) -> Self {
        Self {
            position,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }
}

/// Loaded effects and their layer textures, shared by every instance.
#[derive(Default)]
pub struct EffectLibrary {
    effects: HashMap<String, Option<Rc<Effect>>>,
    textures: HashMap<String, Option<TextureId>>,
}

impl EffectLibrary {
    /// Effect `name`, loaded once; `None` if it can't be loaded.
    fn effect(&mut self, name: &str, content: &Content) -> Option<Rc<Effect>> {
        self.effects
            .entry(name.to_owned())
            .or_insert_with(|| match content.load_effect(name) {
                Ok(e) => Some(Rc::new(e)),
                Err(err) => {
                    eprintln!("effect {name}: {err}");
                    None
                }
            })
            .clone()
    }

    /// Texture of `effect`'s layer `layer`, uploaded on first use.
    pub fn texture(
        &mut self,
        effect: &Effect,
        layer: usize,
        renderer: &mut Renderer,
        content: &Content,
    ) -> Option<TextureId> {
        let path = effect.layers.get(layer)?.texture.as_deref()?;
        *self.textures.entry(path.to_owned()).or_insert_with(|| {
            content
                .load_image(path)
                .map(|image| renderer.upload_texture(&image))
        })
    }

    /// Effect `name` standing at `spot`, playing its loops from `now_ms`.
    pub fn place(
        &mut self,
        name: &str,
        content: &Content,
        scene: &Scene,
        spot: Spot,
        tilt_deg: f32,
        now_ms: u64,
    ) -> Option<MapEffect> {
        let effect = self.effect(name, content)?;
        let Spot {
            position,
            rotation,
            scale,
        } = spot;
        Some(MapEffect {
            cell: scene.world_to_cell(position),
            player: EffectPlayer::new(&effect, now_ms),
            placement: effect::ground_placement(&effect, position, 0.0, tilt_deg, rotation, scale),
            effect,
        })
    }

    /// Instances of the scene's effect entities, playing from `now_ms`. Simple-object
    /// entities (`$id`) are not effects and are skipped.
    pub fn map_effects(
        &mut self,
        scene: &Scene,
        content: &Content,
        tilt_deg: f32,
        now_ms: u64,
    ) -> Vec<MapEffect> {
        let mut out = Vec::new();
        for entity in &scene.entities {
            let EntityKind::Effect { name, transform } = &entity.kind else {
                continue;
            };
            if name.starts_with('$') {
                continue;
            }
            let mut spot = Spot::at(scene.entity_world(entity));
            if let Some(t) = transform {
                // Classic `C3DMapEffectNew` reads the first angle twice: (vertical, 0, horizontal).
                spot.rotation = Vec3::new(t.angles[1], 0.0, t.angles[2]);
                spot.scale = Vec3::from(t.scale);
            }
            let placed = self.place(name, content, scene, spot, tilt_deg, now_ms);
            out.extend(placed.map(MapEffect::endless));
        }
        out
    }
}
