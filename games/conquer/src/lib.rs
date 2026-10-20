//! Conquer game module: everything specific to Conquer Online 5065 on top of the engine.
//! Engine crates must never depend on this.
//!
//! [`Content`] opens a 5065 client folder and hands out engine types (scenes, models, images).

pub mod character;
pub mod game;
pub mod guild_war;
pub mod import;
mod magic;
pub mod module;
pub mod npc;
pub mod npc_logic;
pub mod protocol;
pub mod rule_blueprint;
pub mod rules;
pub mod server_data;
pub mod sound;

pub use module::{ConquerGameRules, ConquerModule};

use c3_anim::Clip;
use c3_assets::Image;
use c3_assets::effect::Effect;
use c3_assets::model::Model;
use c3_assets::sprite::{Prefab, SpriteSet};
use c3_formats_tq::{
    ani::Ani,
    c3::C3File,
    dat::ActionTable,
    effect_io::{self, EffectTable},
    gamemap::GameMapCatalog,
    ini::{Ini, PartTable, PathTable},
    model_io,
    scene::TerrainObject,
    scene_io, sprite_io, texture,
    vfs::ClientFs,
};
use c3_scene::Scene;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use scene_io::Imported;

/// A 5065 client's assets: loose files, packed archives and the lookup tables.
pub struct Content {
    pub root: PathBuf,
    fs: ClientFs,
    catalog: GameMapCatalog,
    tables: Tables,
    npc_tables: npc::NpcTables,
    /// `ani/ItemMinIcon.Ani`, read when an item's icon is first wanted.
    item_icons: std::sync::OnceLock<Option<Ani>>,
}

/// Lookup tables from `ini/`.
#[derive(Default)]
struct Tables {
    /// `3dobj.ini`: model id → C3 path.
    objects: PathTable,
    /// `3dtexture.ini`: texture id → image path.
    textures: PathTable,
    /// `3dmotion.ini`: body motion id → C3 path.
    motions: PathTable,
    /// `WeaponMotion.ini`: weapon motion id → C3 path.
    weapon_motions: PathTable,
    /// `3DEffect.ini`: effect name → parts and clock.
    effects: EffectTable,
    /// `3DEffectObj.ini`: effect part id → C3 path.
    effect_files: PathTable,
    armor: PartTable,
    armet: PartTable,
    weapon: PartTable,
    /// Socket part names (`RolePart.ini [Dumy]`), never drawn.
    sockets: Vec<String>,
    /// `Action.dat`: action key (look·10⁶ + weapon family·10³ + action) → ms per frame.
    frame_intervals: HashMap<u32, u32>,
    /// `Action3DEffect.ini`, `Action3DEffect1.ini`, `Action3DEffect2.ini` (effect slots 0–2):
    /// look·10⁹ + action·10⁶ + part·10³ + variant → effect name.
    action_effects: [HashMap<u64, String>; 3],
    /// `Monster.dat` (encrypted): monster type id → drawing size in percent.
    monster_zoom: HashMap<u32, u32>,
    sounds: sound::SoundTables,
    /// `MagicType.dat`: skill type → its effect and sound.
    magic: HashMap<u32, magic::MagicMedia>,
}

impl Tables {
    fn load(fs: &ClientFs) -> Self {
        let bytes = |name: &str| {
            fs.read(&format!("ini/{name}"))
                .map(|b| b.into_owned())
                .unwrap_or_default()
        };
        let sockets = Ini::parse(&bytes("RolePart.ini"))
            .section("Dumy")
            .map(|s| {
                s.entries
                    .iter()
                    .filter(|(k, _)| k.starts_with("Dumy"))
                    .map(|(_, v)| v.clone())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            objects: PathTable::parse(&bytes("3dobj.ini")),
            textures: PathTable::parse(&bytes("3dtexture.ini")),
            motions: PathTable::parse(&bytes("3dmotion.ini")),
            weapon_motions: PathTable::parse(&bytes("WeaponMotion.ini")),
            effects: EffectTable::parse(&bytes("3DEffect.ini")),
            effect_files: PathTable::parse(&bytes("3DEffectObj.ini")),
            armor: PartTable::parse(&bytes("armor.ini")),
            armet: PartTable::parse(&bytes("armet.ini")),
            weapon: PartTable::parse(&bytes("weapon.ini")),
            sockets,
            frame_intervals: ActionTable::parse(&bytes("Action.dat"))
                .map(|t| t.entries.iter().map(|e| (e.action, e.interval)).collect())
                .unwrap_or_default(),
            action_effects: [
                "Action3DEffect.ini",
                "Action3DEffect1.ini",
                "Action3DEffect2.ini",
            ]
            .map(|name| character::parse_action_effects(&bytes(name))),
            monster_zoom: monster_zoom(&bytes("Monster.dat")),
            sounds: sound::SoundTables::parse(
                &bytes("ActionSound.ini"),
                &bytes("MediaEffect.ini"),
                &bytes("MusicRegion.ini"),
            ),
            magic: magic::parse(&bytes("MagicType.dat")),
        }
    }
}

/// Monster type id → `ZoomPercent`, from `Monster.dat` (`[Name]` sections with `TypeID`).
fn monster_zoom(encrypted: &[u8]) -> HashMap<u32, u32> {
    let plain = c3_formats_tq::cipher::decrypt(encrypted, c3_formats_tq::cipher::SEED_DATA);
    Ini::parse(&plain)
        .sections
        .iter()
        .filter_map(|s| Some((s.get_parsed("TypeID")?, s.get_parsed("ZoomPercent")?)))
        .collect()
}

impl Content {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref().to_path_buf();
        let fs = ClientFs::open(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        let catalog = fs
            .read("ini/GameMap.dat")
            .map(|b| GameMapCatalog::parse(&b))
            .transpose()
            .map_err(|e| format!("ini/GameMap.dat: {e}"))?
            .unwrap_or_default();
        let tables = Tables::load(&fs);
        let ini = |name: &str| {
            fs.read(&format!("ini/{name}"))
                .map(|b| b.into_owned())
                .unwrap_or_default()
        };
        let npc_tables = npc::NpcTables::parse(
            &ini("Npc.ini"),
            &ini("3DSimpleObj.ini"),
            &ini("3DsimpleRole.ini"),
        );
        Ok(Self {
            root,
            fs,
            catalog,
            tables,
            npc_tables,
            item_icons: std::sync::OnceLock::new(),
        })
    }

    /// How large monster type `kind` is drawn, in percent (100 when `Monster.dat` doesn't say).
    pub fn monster_zoom(&self, kind: u32) -> f32 {
        self.tables.monster_zoom.get(&kind).copied().unwrap_or(100) as f32
    }

    /// DMap path of map document `id` (as the server refers to maps).
    pub fn map_path(&self, id: u32) -> Option<String> {
        self.catalog.get(id).map(|e| e.path_str())
    }

    /// Ids of the maps whose DMap is `path` (case-insensitive, either slash).
    pub fn map_ids_for_path(&self, path: &str) -> Vec<u32> {
        // A copy of a map file is the one map its key names.
        if let (_, Some(id)) = c3_play::map_list::split_instance(path) {
            return vec![id];
        }
        let norm = |p: &str| p.replace('\\', "/").to_ascii_lowercase();
        let wanted = norm(path);
        self.catalog
            .entries
            .iter()
            .filter(|e| norm(&e.path_str()) == wanted)
            .map(|e| e.id)
            .collect()
    }

    /// Map files of the client (`map/map/*.DMap`), as asset paths, sorted.
    pub fn list_maps(&self) -> Vec<String> {
        let mut maps: Vec<String> = std::fs::read_dir(self.root.join("map").join("map"))
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.to_ascii_lowercase().ends_with(".dmap"))
                    .map(|n| format!("map/map/{n}"))
                    .collect()
            })
            .unwrap_or_default();
        maps.sort_by_key(|m| m.to_ascii_lowercase());
        maps
    }

    /// Map ids in the catalog.
    pub fn map_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.catalog.entries.iter().map(|e| e.id)
    }

    /// Loads a map (by DMap path) as an engine scene.
    pub fn load_map(&self, path: &str) -> Result<Imported, String> {
        // A copy of a map file loads that file.
        let (path, _) = c3_play::map_list::split_instance(path);
        scene_io::import_map(&self.fs, path).map_err(|e| format!("{path}: {e}"))
    }

    /// Loads map document `id`.
    pub fn load_map_id(&self, id: u32) -> Result<(String, Scene, Vec<String>), String> {
        let path = self
            .map_path(id)
            .ok_or_else(|| format!("map {id} is not in GameMap.dat"))?;
        let imported = self.load_map(&path)?;
        Ok((path, imported.scene, imported.warnings))
    }

    /// Loads model `id` from `3dobj.ini`, with the texture registered under the same id in
    /// `3dtexture.ini` (true for NPCs and monsters; equipment uses the part tables).
    pub fn load_model(&self, id: u64) -> Result<(Model, Option<String>), String> {
        let path = self
            .tables
            .objects
            .get(id)
            .ok_or_else(|| format!("model {id} is not in 3dobj.ini"))?;
        let mut model = self.load_model_path(path)?;
        model.mark_sockets(
            &self
                .tables
                .sockets
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        Ok((model, self.tables.textures.get(id).map(str::to_owned)))
    }

    /// Loads a C3 model file (its embedded motion becomes the default animation).
    pub fn load_model_path(&self, path: &str) -> Result<Model, String> {
        let bytes = self
            .fs
            .read(path)
            .ok_or_else(|| format!("file not found: {path}"))?;
        let file = C3File::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
        model_io::import_model(&file).map_err(|e| format!("{path}: {e}"))
    }

    /// Loads a motion file's clips (one per model part, by index).
    pub fn load_motion(&self, path: &str) -> Option<Vec<Clip>> {
        model_io::import_motion(&C3File::parse(&self.fs.read(path)?).ok()?).ok()
    }

    /// Camera tilt for 3D actors in degrees: the 7th field of `ini/graphic.ini`
    /// (`LoadGraphicConfig5065`), −30 if absent or malformed.
    pub fn oblique_angle(&self) -> f32 {
        let text = self
            .fs
            .read("ini/graphic.ini")
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        text.and_then(|t| {
            let fields: Vec<&str> = t.split_whitespace().collect();
            (fields.len() >= 7)
                .then(|| fields[6].parse::<f32>().ok())
                .flatten()
        })
        .unwrap_or(-30.0)
    }

    /// Loads effect `name` from `3DEffect.ini`. Like the client, a part whose file or
    /// texture is missing ends the effect there.
    pub fn load_effect(&self, name: &str) -> Result<Effect, String> {
        let info = self
            .tables
            .effects
            .get(name)
            .ok_or_else(|| format!("effect {name} is not in 3DEffect.ini"))?;
        let mut layers = Vec::with_capacity(info.parts.len());
        for part in &info.parts {
            let Some(path) = self.tables.effect_files.get(part.effect) else {
                break;
            };
            let Some(texture) = self.tables.textures.get(part.texture) else {
                break;
            };
            let Some(bytes) = self.fs.read(path) else {
                break;
            };
            let file = C3File::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
            let mut layer =
                effect_io::import_effect_layer(&file).map_err(|e| format!("{path}: {e}"))?;
            layer.texture = Some(texture.to_owned());
            layer.blend = effect_io::blend_from_d3d(part.src_blend, part.dst_blend);
            layers.push(layer);
        }
        let [x, y, z] = info.offset.map(|v| v as f32);
        Ok(Effect {
            layers,
            timing: info.timing,
            offset: glam::Vec3::new(x, y, z),
        })
    }

    /// Effect names in `3DEffect.ini`.
    pub fn effect_names(&self) -> impl Iterator<Item = &str> {
        self.tables.effects.names()
    }

    /// Loads an ANI file as a sprite set; a single image (a DDS placed on a map in the
    /// Studio) is a set of one frame.
    pub fn load_sprite_set(&self, path: &str) -> Option<SpriteSet> {
        let bytes = self.fs.read(path)?;
        if let Ok(ani) = Ani::parse(&bytes) {
            return Some(sprite_io::sprite_set_from_ani(&ani));
        }
        self.load_image(path)?;
        Some(SpriteSet {
            animations: [(
                c3_play::native::ALL_FRAMES.to_owned(),
                vec![path.to_owned()],
            )]
            .into(),
        })
    }

    /// Loads a `.scene` terrain object as a prefab.
    pub fn load_prefab(&self, path: &str) -> Option<Prefab> {
        Some(sprite_io::prefab_from_scene(
            &TerrainObject::parse(&self.fs.read(path)?).ok()?,
        ))
    }

    /// The client's files: loose and packed.
    pub fn files(&self) -> &ClientFs {
        &self.fs
    }

    /// Decodes an image asset (DDS/TGA).
    /// Loads a DDS or TGA texture, or a picture (JPG, BMP, PNG: minimaps, GUI art).
    pub fn load_image(&self, path: &str) -> Option<Image> {
        let bytes = self.fs.read(path)?;
        (texture::decode_image(&bytes).ok()).or_else(|| c3_play::native::decode_image(&bytes))
    }

    /// The minimap of the first of `map_ids` the client has (see `ConquerModule::minimap`).
    /// The radar's picture for mark `kind` (see [`RADAR_MARKS`]).
    /// The login list from `Server.dat` (a project's own copy first: a native one naming
    /// Server.ani's pictures, as the pre-Flash clients' does) (CDlgLogin's LoadNativeGroups and
    /// LoadNativeServers): `[Header]` GroupAmount and GroupN pictures, each `[GroupN]`'s
    /// ServerAmount, ServerNameN (else ServerN) and PicN, every server at `address`. A
    /// picture shows when Server.ani has a section of its name; the 5065 list names the Flash
    /// login's files (`Group11.swf`), which the native login, like the client's, leaves blank.
    pub fn login_list(
        address: &str,
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Option<c3_play::module::LoginList> {
        use c3_core::settings::{ServerEntry, ServerGroup};
        let ini = c3_formats_tq::ini::Ini::parse(&read(SERVER_DAT)?);
        let ani = read(SERVER_ANI).and_then(|bytes| Ani::parse(&bytes).ok());
        // A picture is the Server.ani section of that name, as CMyImage::InsertImage finds it.
        let picture = |name: &str| -> String {
            match ani.as_ref().is_some_and(|a| a.get(name).is_some()) {
                true => name.to_owned(),
                false => String::new(),
            }
        };
        let header = ini.section("Header")?;
        let amount: usize = header.get_parsed("GroupAmount").unwrap_or(0);
        let mut groups = Vec::new();
        let mut servers = Vec::new();
        for g in 1..=amount {
            let name = format!("Group{g}");
            let icon = picture(header.get(&name).unwrap_or_default());
            let Some(section) = ini.section(&name) else {
                continue;
            };
            for s in 1..=section.get_parsed::<usize>("ServerAmount").unwrap_or(0) {
                let shown = (section.get(&format!("ServerName{s}")))
                    .filter(|n| !n.trim().is_empty())
                    .or_else(|| section.get(&format!("Server{s}")))
                    .unwrap_or_default();
                servers.push(ServerEntry {
                    name: shown.trim().to_owned(),
                    address: address.to_owned(),
                    group: name.clone(),
                    icon: picture(section.get(&format!("Pic{s}")).unwrap_or_default()),
                });
            }
            groups.push(ServerGroup { name, icon });
        }
        (!servers.is_empty()).then_some((groups, servers))
    }

    /// An item type's icon as the client finds it (`CItem::ShowMiniItem`, `GetItemMinIcon`):
    /// helmets, armour and shields by their colour (`color × 1,000,000 + type`, colour 3 when
    /// it has none or that one is missing), others by their type; then `ItemDefault`.
    /// The light played under an item's icon, as CDlgMpcGoods::GetQualityEffect picks it
    /// and CMyGrid draws it from `ani/effect.ani`: by the quality digit of gear (6 refined to
    /// 9 super), gems, DragonBalls and Meteors, and the three exp balls.
    pub fn item_glow(&self, kind: u32) -> Vec<Image> {
        let Some(section) = quality_light(kind) else {
            return Vec::new();
        };
        let Some(ani) = (self.fs.read(EFFECT_ANI)).and_then(|b| Ani::parse(&b).ok()) else {
            return Vec::new();
        };
        let Some(frames) = ani.get(section) else {
            return Vec::new();
        };
        (0..frames.frames.len())
            .filter_map(|i| self.load_image(&frames.frame_str(i)?))
            .collect()
    }

    pub fn item_icon(&self, kind: u32, color: u32) -> Option<Image> {
        let ani = (self.item_icons)
            .get_or_init(|| Ani::parse(&self.fs.read(ITEM_ICON_ANI)?).ok())
            .as_ref()?;
        let coloured = matches!(kind / 10_000, 11 | 13 | 90..=99);
        let keys = match coloured {
            true => {
                let color = if color == 0 { 3 } else { color };
                vec![color * 1_000_000 + kind, 3_000_000 + kind, kind]
            }
            false => vec![kind],
        };
        let section = (keys.iter())
            .find_map(|k| ani.get(&format!("Item{k}")))
            .or_else(|| ani.get("ItemDefault"))?;
        self.load_image(&section.frame_str(0)?)
    }

    pub fn radar_mark(&self, kind: &str) -> Option<Image> {
        let (_, file, section) = RADAR_MARKS.iter().find(|(k, _, _)| *k == kind)?;
        let ani = Ani::parse(&self.fs.read(file)?).ok()?;
        self.load_image(&ani.get(section)?.frame_str(0)?)
    }

    pub fn minimap(&self, map_ids: &[u32]) -> Option<Image> {
        let ani = Ani::parse(&self.fs.read(MINIMAP_ANI)?).ok()?;
        let section = map_ids.iter().find_map(|id| ani.get(&id.to_string()))?;
        let parts: Vec<Image> = (0..section.frames.len())
            .filter_map(|i| self.load_image(&section.frame_str(i)?))
            .collect();
        match parts.len() {
            4 => Some(quarters(&parts)),
            _ => parts.into_iter().next(),
        }
    }
}

impl c3_render::SceneAssets for Content {
    fn sprite_set(&self, path: &str) -> Option<SpriteSet> {
        self.load_sprite_set(path)
    }

    fn prefab(&self, path: &str) -> Option<Prefab> {
        self.load_prefab(path)
    }

    fn tile_layer(&self, path: &str) -> Option<c3_scene::TileLayer> {
        scene_io::load_tile_layer(&self.fs, path).ok()
    }

    fn image(&self, path: &str) -> Option<Image> {
        c3_play::builtin::image(path).or_else(|| self.load_image(path))
    }

    fn effect(&self, name: &str) -> Result<Effect, String> {
        self.load_effect(name)
    }
}

/// The client's minimaps, by map id.
const MINIMAP_ANI: &str = "ani/MiniMap.Ani";
const ITEM_ICON_ANI: &str = "ani/ItemMinIcon.Ani";
const EFFECT_ANI: &str = "ani/effect.ani";

/// CDlgMpcGoods::GetQualityEffect: the light under an item type's icon (effect.ani).
fn quality_light(kind: u32) -> Option<&'static str> {
    if kind / 100_000 < 10 && !(700_000..=800_000).contains(&kind) {
        match kind % 10 {
            6 => return Some("RoyalBlueLight"),
            7 => return Some("BlueLight"),
            8 => return Some("RedLight"),
            9 => return Some("FireLight"),
            _ => {}
        }
    }
    match kind {
        700_001..=700_127 => Some("GoldenLight"),
        1_088_000..=1_088_002 | 1_100_003 | 1_100_006 | 1_100_009 => Some("YellowLight"),
        _ => None,
    }
}
const SERVER_DAT: &str = "Server.dat";
const SERVER_ANI: &str = "ani/Server.ani";
/// The radar's marks, as CRadar draws them: the hero (MiniMap.ani), players and team
/// leaders (common.ani).
const RADAR_MARKS: [(&str, &str, &str); 3] = [
    ("hero", MINIMAP_ANI, "hero"),
    ("player", "ani/common.ani", "PlayerFlag"),
    ("leader", "ani/common.ani", "TeamLeader"),
];

/// Four images as the quarters of one (top-left, top-right, bottom-left, bottom-right).
fn quarters(parts: &[Image]) -> Image {
    let (w, h) = (parts[0].width, parts[0].height);
    let mut out = Image::new(w * 2, h * 2, vec![0; (w * h * 16) as usize]);
    for (i, part) in parts.iter().enumerate() {
        let (ox, oy) = ((i as u32 % 2) * w, (i as u32 / 2) * h);
        for y in 0..part.height.min(h) {
            let from = (y * part.width * 4) as usize;
            let to = (((oy + y) * out.width + ox) * 4) as usize;
            let row = (part.width.min(w) * 4) as usize;
            out.rgba[to..to + row].copy_from_slice(&part.rgba[from..from + row]);
        }
    }
    out
}

#[cfg(test)]
mod glow_tests {
    #[test]
    fn lights_follow_the_quality_digit_gems_and_balls() {
        assert_eq!(super::quality_light(410_009), Some("FireLight"));
        assert_eq!(super::quality_light(130_006), Some("RoyalBlueLight"));
        assert_eq!(super::quality_light(410_003), None);
        assert_eq!(super::quality_light(700_011), Some("GoldenLight"));
        assert_eq!(super::quality_light(1_088_000), Some("YellowLight"));
    }
}
