//! Regions of a map, addressed by id: adding, changing and removing them.

use super::{Change, Edit, SceneDoc};
use crate::EditError;
use c3_scene::Region;

impl SceneDoc {
    pub(super) fn add_region(&mut self, region: &Region) -> Edit {
        check_area(region)?;
        let mut region = region.clone();
        if region.id == 0 {
            let ids = || self.scene.regions.iter().map(|r| r.id);
            region.id = (1..).find(|i| !ids().any(|id| id == *i)).unwrap_or(1);
        } else if self.region_index(region.id).is_ok() {
            return Err(region_error(format!("region id {} is taken", region.id)));
        }
        let summary = format!("Added region {}", region.id);
        Ok((summary, Some(self.edit_regions(|r| r.push(region)))))
    }

    pub(super) fn set_region(&mut self, region: &Region) -> Edit {
        let i = self.region_index(region.id)?;
        check_area(region)?;
        if &self.scene.regions[i] == region {
            return Ok(("Nothing changed".into(), None));
        }
        let summary = format!("Changed region {}", region.id);
        Ok((summary, Some(self.edit_regions(|r| r[i] = region.clone()))))
    }

    pub(super) fn remove_region(&mut self, id: u32) -> Edit {
        let i = self.region_index(id)?;
        let change = self.edit_regions(|r| {
            r.remove(i);
        });
        Ok((format!("Deleted region {id}"), Some(change)))
    }

    fn region_index(&self, id: u32) -> Result<usize, EditError> {
        (self.scene.regions.iter())
            .position(|r| r.id == id)
            .ok_or_else(|| region_error(format!("there is no region {id}")))
    }

    fn edit_regions(&mut self, edit: impl FnOnce(&mut Vec<Region>)) -> Change {
        let before = self.scene.regions.clone();
        edit(&mut self.scene.regions);
        Change::Regions {
            before,
            after: self.scene.regions.clone(),
        }
    }
}

fn check_area(region: &Region) -> Result<(), EditError> {
    if region.max.0 < region.min.0 || region.max.1 < region.min.1 {
        return Err(EditError::InvalidArea(
            "the region's max is before its min".into(),
        ));
    }
    Ok(())
}

fn region_error(why: String) -> EditError {
    EditError::InvalidArea(why)
}

#[cfg(test)]
mod tests {
    use crate::SceneCommand;
    use crate::scene::tests::doc;
    use c3_scene::Region;

    #[test]
    fn regions_get_ids_change_and_undo() {
        let mut d = doc();
        let safe = Region {
            min: (1, 1),
            max: (3, 3),
            safe: true,
            ..Region::default()
        };
        d.execute(&SceneCommand::AddRegion {
            region: safe.clone(),
        })
        .unwrap();
        d.execute(&SceneCommand::AddRegion {
            region: safe.clone(),
        })
        .unwrap();
        let ids: Vec<u32> = d.scene().regions.iter().map(|r| r.id).collect();
        assert_eq!(ids, [1, 2]);
        let mut market = d.scene().regions[1].clone();
        market.booth = true;
        d.execute(&SceneCommand::SetRegion { region: market })
            .unwrap();
        assert!(d.scene().regions[1].booth);
        let backwards = Region {
            id: 2,
            min: (5, 5),
            max: (1, 1),
            ..Region::default()
        };
        assert!(
            d.execute(&SceneCommand::SetRegion { region: backwards })
                .is_err()
        );
        d.execute(&SceneCommand::RemoveRegion { id: 1 }).unwrap();
        d.undo();
        assert_eq!(d.scene().regions.len(), 2);
    }
}
