//! A whole map drawn small, for radars and map windows: the ground and objects, rendered
//! a piece at a time (each piece's art loaded, drawn and freed) so even the largest maps fit
//! in memory.

use crate::scene_view::{Layers, SceneView};
use crate::{Renderer, SceneAssets, View};
use c3_assets::Image;
use c3_scene::Scene;

/// World pixels along a side of one piece (about 8 × 8 classic ground tiles).
const PIECE: f32 = 2048.0;
/// Most prepare passes a piece gets for its art to stream in.
const MAX_PASSES: usize = 4000;

/// The part of the world the map covers: its ground, or else its cells,
/// `[left, top, right, bottom]`.
pub fn scene_bounds(scene: &Scene) -> [f32; 4] {
    if let Some(g) = scene.ground.as_ref().filter(|g| g.width > 0) {
        let (w, h) = g.world_size();
        let (x, y) = (g.origin.0 as f32, g.origin.1 as f32);
        return [x, y, x + w as f32, y + h as f32];
    }
    let (w, h) = (scene.grid.width as i32, scene.grid.height as i32);
    let corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)].map(|c| scene.cell_to_world(c));
    let xs = corners.map(|c| c.0);
    let ys = corners.map(|c| c.1);
    let min = |v: [f32; 4]| v.into_iter().fold(f32::MAX, f32::min);
    let max = |v: [f32; 4]| v.into_iter().fold(f32::MIN, f32::max);
    [min(xs), min(ys), max(xs), max(ys)]
}

/// `scene` drawn whole, its longer side `max_side` pixels; it covers [`scene_bounds`].
pub fn render_minimap(
    renderer: &mut Renderer,
    scene: &Scene,
    assets: &dyn SceneAssets,
    max_side: u32,
) -> Image {
    let [left, top, right, bottom] = scene_bounds(scene);
    let (world_w, world_h) = ((right - left).max(1.0), (bottom - top).max(1.0));
    let zoom = max_side as f32 / world_w.max(world_h);
    let size = [
        ((world_w * zoom).round() as u32).max(1),
        ((world_h * zoom).round() as u32).max(1),
    ];
    let mut out = Image::new(size[0], size[1], vec![0; (size[0] * size[1] * 4) as usize]);
    let mut view = SceneView::<()>::new(scene, assets, 0.0, 0);
    view.layers = Layers {
        sky: false,
        effects: false,
        ..Layers::default()
    };
    // Pieces meet on whole output pixels, so they tile without gaps.
    let piece_px = ((PIECE * zoom).floor() as u32).max(16);
    for py in (0..size[1]).step_by(piece_px as usize) {
        for px in (0..size[0]).step_by(piece_px as usize) {
            let w = piece_px.min(size[0] - px);
            let h = piece_px.min(size[1] - py);
            let rect = [
                left + px as f32 / zoom,
                top + py as f32 / zoom,
                left + (px + w) as f32 / zoom,
                top + (py + h) as f32 / zoom,
            ];
            let piece = render_piece(renderer, scene, assets, &mut view, rect, zoom, [w, h]);
            blit(&mut out, &piece, px, py);
            view.release(renderer);
        }
    }
    out
}

/// The world `rect` drawn at `zoom` into an image of `size`.
fn render_piece(
    renderer: &mut Renderer,
    scene: &Scene,
    assets: &dyn SceneAssets,
    view: &mut SceneView<()>,
    rect: [f32; 4],
    zoom: f32,
    size: [u32; 2],
) -> Image {
    let camera = View {
        rect,
        center: ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0),
        zoom,
        time_ms: 0,
    };
    for _ in 0..MAX_PASSES {
        view.prepare(scene, assets, renderer, &camera, &[], |_, _| {});
        if !view.streaming() {
            break;
        }
    }
    let mut target = None;
    renderer.ensure_target(&mut target, size);
    let target = target.expect("ensure_target makes a target");
    let meshes = view.mesh_draws(|_| &[]);
    renderer.render(&target, &view.frame(&meshes, rect, [0.0, 0.0, 0.0, 1.0]));
    renderer.read_target(&target)
}

/// Copies `src` into `dst` with its top-left corner at `(x, y)`.
fn blit(dst: &mut Image, src: &Image, x: u32, y: u32) {
    let row = (src.width * 4) as usize;
    for sy in 0..src.height {
        let from = (sy * src.width * 4) as usize;
        let to = (((y + sy) * dst.width + x) * 4) as usize;
        dst.rgba[to..to + row].copy_from_slice(&src.rgba[from..from + row]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c3_scene::{Grid, IsoProjection};

    #[test]
    fn bounds_cover_the_cells_without_ground() {
        let grid = Grid::new(10, 10);
        let scene = Scene {
            projection: IsoProjection::for_grid(64.0, 32.0, grid.width),
            grid,
            ground: None,
            portals: Vec::new(),
            entities: Vec::new(),
            layers: Vec::new(),
            npcs: Vec::new(),
            spawns: Vec::new(),
            regions: Vec::new(),
            meta: Default::default(),
        };
        let [l, t, r, b] = scene_bounds(&scene);
        assert!(l < r && t < b);
        assert_eq!(r - l, 64.0 * 9.0);
    }
}
