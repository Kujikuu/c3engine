//! Map > Generate Minimap: the open map drawn small into `maps/<name>.minimap.png`, the
//! image the game's radar shows under its dots.

use crate::app::Studio;

/// Pixels along the longer side of a minimap.
const MINIMAP_SIDE: u32 = 1024;

impl Studio {
    /// Draws the open map (as saved or not) into its minimap file.
    pub(crate) fn generate_minimap(&mut self) {
        let (Some(doc), Some(game), Some(gpu), Some(project)) =
            (&self.doc, &self.game, &mut self.gpu, &self.project)
        else {
            return;
        };
        let image = c3_render::minimap::render_minimap(
            &mut gpu.renderer,
            doc.scene(),
            game.assets(),
            MINIMAP_SIDE,
        );
        let file = project.minimap_file(&doc.map.key());
        let saved = image::RgbaImage::from_raw(image.width, image.height, image.rgba)
            .ok_or_else(|| "the drawing has the wrong size".to_owned())
            .and_then(|img| img.save(&file).map_err(|e| e.to_string()));
        match saved {
            Ok(()) => {
                let name = file.file_name().map(|n| n.to_string_lossy().into_owned());
                self.notify(format!("Saved the minimap as {}", name.unwrap_or_default()));
                self.refresh_assets();
            }
            Err(e) => self.error = Some(format!("Can't save the minimap: {e}")),
        }
    }
}
