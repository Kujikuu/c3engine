//! The auto-path menu (TQ's CDlgAutoRun5065): the game's destinations on the hero's map, some
//! with places under them; a destination picked is walked to.

use crate::GameView;
use c3_scene::CellPos;

/// A place the hero may be walked to (one of TQ's MapDestination.dat sections), or a heading
/// whose places are those naming it their `parent`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Destination {
    pub id: u32,
    /// The heading it is under (0: none).
    pub parent: u32,
    /// The map it is on (a heading's places take its map).
    pub map: u32,
    /// Where to walk, and how near is there; (0, 0) for a heading.
    pub cell: CellPos,
    pub radius: u32,
    /// The NPC there (0: none).
    pub npc: u32,
    pub title: String,
    /// Its tip, its lines apart.
    pub mark: String,
}

/// What a row picked does: a heading opens its places (or closes them), a place is walked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pick {
    Open(u32),
    Walk(CellPos),
}

/// The destinations heading the hero's map's menu.
pub(crate) fn roots<'a>(view: &GameView<'a>) -> Vec<&'a Destination> {
    let map = view.sim.map_ids().first().copied().unwrap_or(0);
    (view.destinations.iter())
        .filter(|d| d.parent == 0 && d.map == map)
        .collect()
}

/// The places under heading `parent`.
pub(crate) fn children<'a>(view: &GameView<'a>, parent: u32) -> Vec<&'a Destination> {
    (view.destinations.iter())
        .filter(|d| d.parent == parent)
        .collect()
}

/// What row `index` of list `list` (`autorun.rows`, or `autorun.children` of heading `open`)
/// does when picked.
pub(crate) fn pick(view: &GameView, list: &str, index: usize, open: Option<u32>) -> Option<Pick> {
    let rows = match list {
        "autorun.rows" => roots(view),
        "autorun.children" => children(view, open?),
        _ => return None,
    };
    let d = rows.get(index)?;
    if !children(view, d.id).is_empty() {
        return Some(Pick::Open(d.id));
    }
    (d.cell != (0, 0)).then_some(Pick::Walk(d.cell))
}
