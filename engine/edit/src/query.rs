//! Read-only questions about a scene, answered as serializable results.

use crate::{EditError, SceneDoc};
use c3_scene::{CellPos, EntityKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Largest region [`SceneQuery::Walkability`] returns at once.
pub const MAX_WALKABILITY_CELLS: usize = 128 * 128;

/// A question about a scene. Serialized as `{"query": "summary"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "query", rename_all = "snake_case")]
pub enum SceneQuery {
    /// Size, walkable share, portals and placed objects at a glance.
    Summary,
    /// One cell: walkability, surface, height, portal.
    Cell { cell: CellPos },
    /// Every portal.
    Portals,
    /// Walkability of the cells from `min` to `max` (inclusive) as text rows,
    /// `.` walkable, `#` blocked, `P` portal; at most 16384 cells.
    Walkability { min: CellPos, max: CellPos },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneSummary {
    pub width: u32,
    pub height: u32,
    pub walkable_percent: f32,
    pub portals: usize,
    /// Placed objects by kind (sprite, prefab, sound, effect).
    pub entities: BTreeMap<String, usize>,
    pub parallax_layers: usize,
    pub has_ground: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CellInfo {
    pub cell: CellPos,
    pub blocked: bool,
    pub surface: u16,
    pub height: i16,
    pub portal: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortalInfo {
    pub id: i32,
    pub cell: CellPos,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WalkabilityMap {
    pub min: CellPos,
    pub max: CellPos,
    /// One string per row from `min.1` to `max.1`.
    pub rows: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum QueryResult {
    Summary(SceneSummary),
    Cell(CellInfo),
    Portals(Vec<PortalInfo>),
    Walkability(WalkabilityMap),
}

impl SceneDoc {
    /// Answers `query` without changing anything.
    pub fn query(&self, query: &SceneQuery) -> Result<QueryResult, EditError> {
        let scene = self.scene();
        Ok(match query {
            SceneQuery::Summary => {
                let mut entities = BTreeMap::new();
                for e in &scene.entities {
                    let kind = match e.kind {
                        EntityKind::Sprite { .. } => "sprite",
                        EntityKind::Prefab { .. } => "prefab",
                        EntityKind::Sound { .. } => "sound",
                        EntityKind::Effect { .. } => "effect",
                    };
                    *entities.entry(kind.to_owned()).or_insert(0) += 1;
                }
                QueryResult::Summary(SceneSummary {
                    width: scene.grid.width,
                    height: scene.grid.height,
                    walkable_percent: (scene.grid.walkable_ratio() * 1000.0).round() / 10.0,
                    portals: scene.portals.len(),
                    entities,
                    parallax_layers: scene.layers.len(),
                    has_ground: scene.ground.is_some(),
                })
            }
            SceneQuery::Cell { cell } => {
                let c = scene.grid.get(*cell).ok_or(EditError::OutOfBounds(*cell))?;
                QueryResult::Cell(CellInfo {
                    cell: *cell,
                    blocked: c.is_blocked(),
                    surface: c.surface,
                    height: c.height,
                    portal: scene.portal_at(*cell).map(|i| scene.portals[i].id),
                })
            }
            SceneQuery::Portals => QueryResult::Portals(
                scene
                    .portals
                    .iter()
                    .map(|p| PortalInfo {
                        id: p.id,
                        cell: p.cell,
                    })
                    .collect(),
            ),
            SceneQuery::Walkability { min, max } => {
                if max.0 < min.0 || max.1 < min.1 {
                    return Err(EditError::InvalidArea("max is before min".into()));
                }
                let count = (max.0 - min.0 + 1) as usize * (max.1 - min.1 + 1) as usize;
                if count > MAX_WALKABILITY_CELLS {
                    return Err(EditError::AreaTooLarge(count));
                }
                let rows = (min.1..=max.1)
                    .map(|y| {
                        (min.0..=max.0)
                            .map(|x| match scene.grid.get((x, y)) {
                                None => ' ',
                                Some(_) if scene.portal_at((x, y)).is_some() => 'P',
                                Some(c) if c.is_blocked() => '#',
                                Some(_) => '.',
                            })
                            .collect()
                    })
                    .collect();
                QueryResult::Walkability(WalkabilityMap {
                    min: *min,
                    max: *max,
                    rows,
                })
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Area, SceneCommand};
    use c3_scene::{Grid, IsoProjection, Scene};

    #[test]
    fn answers_summary_cells_and_walkability() {
        let mut d = SceneDoc::new(Scene {
            grid: Grid::new(4, 3),
            projection: IsoProjection::for_grid(64.0, 32.0, 4),
            ground: None,
            portals: Vec::new(),
            entities: Vec::new(),
            layers: Vec::new(),
            meta: Default::default(),
        });
        d.execute(&SceneCommand::SetBlocked {
            area: Area::Cells {
                cells: vec![(1, 1), (2, 1)],
            },
            blocked: true,
        })
        .unwrap();
        d.execute(&SceneCommand::AddPortal {
            cell: (0, 0),
            id: Some(7),
        })
        .unwrap();
        let QueryResult::Walkability(w) = d
            .query(&SceneQuery::Walkability {
                min: (0, 0),
                max: (3, 2),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(w.rows, ["P...", ".##.", "...."]);
        let QueryResult::Summary(s) = d.query(&SceneQuery::Summary).unwrap() else {
            panic!()
        };
        assert_eq!((s.width, s.portals, s.walkable_percent), (4, 1, 83.3));
        let json =
            serde_json::to_string(&d.query(&SceneQuery::Cell { cell: (0, 0) }).unwrap()).unwrap();
        assert!(json.contains(r#""portal":7"#), "{json}");
        let q: SceneQuery = serde_json::from_str(r#"{"query":"portals"}"#).unwrap();
        assert_eq!(q, SceneQuery::Portals);
    }
}
