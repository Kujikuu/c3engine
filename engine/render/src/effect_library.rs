//! Playing 3D effects: the map's own (fires, fountains, glows), which play forever like the
//! client's `C3DMapEffect`s, one-shots such as the click marker, and effects carried by actors.
//! [`EffectLibrary`] loads each effect and its textures once and shares them by every instance.

use crate::assets::SceneAssets;
use crate::effect::{self, EffectMesh, EffectPlayer};
use crate::{Renderer, TextureId};
use c3_assets::effect::{Effect, Playback};
use c3_scene::{CellPos, EntityKind, Scene};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::rc::Rc;

/// An effect playing somewhere: its asset and clock.
pub struct RunningEffect {
    name: String,
    effect: Rc<Effect>,
    player: EffectPlayer,
}

impl RunningEffect {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn effect(&self) -> &Effect {
        &self.effect
    }

    /// The effect asset, shared.
    pub fn shared(&self) -> Rc<Effect> {
        Rc::clone(&self.effect)
    }

    /// Plays forever (`CGame3DEffectEx::EverPlay`).
    pub fn endless(mut self) -> Self {
        self.player = self.player.endless();
        self
    }

    pub fn is_finished(&self, now_ms: u64) -> bool {
        self.player.playback(&self.effect, now_ms) == Playback::Finished
    }

    /// This frame's geometry, with effect space placed by `placement`.
    pub fn meshes(&mut self, now_ms: u64, placement: Mat4, eye: Vec3) -> Vec<EffectMesh> {
        self.player.meshes(&self.effect, now_ms, placement, eye)
    }
}

/// One effect standing on the map.
pub struct MapEffect {
    pub cell: CellPos,
    pub running: RunningEffect,
    placement: Mat4,
}

impl MapEffect {
    /// World position of the effect's origin.
    pub fn position(&self) -> Vec3 {
        self.placement.w_axis.truncate()
    }

    /// This frame's geometry.
    pub fn meshes(&mut self, now_ms: u64, eye: Vec3) -> Vec<EffectMesh> {
        self.running.meshes(now_ms, self.placement, eye)
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
    /// Load failures not yet reported (see [`EffectLibrary::take_errors`]).
    errors: Vec<String>,
}

impl EffectLibrary {
    /// Effect `name`, loaded once; `None` if it can't be loaded.
    fn effect(&mut self, name: &str, assets: &dyn SceneAssets) -> Option<Rc<Effect>> {
        if let Some(effect) = self.effects.get(name) {
            return effect.clone();
        }
        let effect = match assets.effect(name) {
            Ok(e) => Some(Rc::new(e)),
            Err(err) => {
                self.errors.push(format!("effect {name}: {err}"));
                None
            }
        };
        self.effects.insert(name.to_owned(), effect.clone());
        effect
    }

    /// Load failures since the last call, each reported once.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    /// Starts effect `name` at `now_ms`, playing its own loops.
    pub fn start(
        &mut self,
        name: &str,
        assets: &dyn SceneAssets,
        now_ms: u64,
    ) -> Option<RunningEffect> {
        let effect = self.effect(name, assets)?;
        Some(RunningEffect {
            name: name.to_owned(),
            player: EffectPlayer::new(&effect, now_ms),
            effect,
        })
    }

    /// Texture of `effect`'s layer `layer`, uploaded on first use.
    pub fn texture(
        &mut self,
        effect: &Effect,
        layer: usize,
        renderer: &mut Renderer,
        assets: &dyn SceneAssets,
    ) -> Option<TextureId> {
        let path = effect.layers.get(layer)?.texture.as_deref()?;
        *self.textures.entry(path.to_owned()).or_insert_with(|| {
            assets
                .image(path)
                .map(|image| renderer.upload_texture(&image))
        })
    }

    /// Releases the uploaded textures (the effects stay loaded).
    pub fn release_textures(&mut self, renderer: &mut Renderer) {
        for id in self.textures.drain().filter_map(|(_, t)| t) {
            renderer.release_texture(id);
        }
    }

    /// Effect `name` standing at `spot`, playing its loops from `now_ms`.
    pub fn place(
        &mut self,
        name: &str,
        assets: &dyn SceneAssets,
        scene: &Scene,
        spot: Spot,
        tilt_deg: f32,
        now_ms: u64,
    ) -> Option<MapEffect> {
        let running = self.start(name, assets, now_ms)?;
        let Spot {
            position,
            rotation,
            scale,
        } = spot;
        Some(MapEffect {
            cell: scene.world_to_cell(position),
            placement: effect::ground_placement(
                running.effect(),
                position,
                0.0,
                tilt_deg,
                rotation,
                scale,
            ),
            running,
        })
    }

    /// Instances of the scene's effect entities, playing forever from `now_ms`.
    /// Simple-object entities (`$id`) are not effects and are skipped.
    pub fn map_effects(
        &mut self,
        scene: &Scene,
        assets: &dyn SceneAssets,
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
            if let Some(mut e) = self.place(name, assets, scene, spot, tilt_deg, now_ms) {
                e.running = e.running.endless();
                out.push(e);
            }
        }
        out
    }
}
