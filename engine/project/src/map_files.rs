//! The project's maps as files: copying, renaming, and moving them to the trash and back.
//! A map is its `.scene.ron` and the `.scene.bin` beside it; renaming one also renames its
//! entry in the map list (Project Settings > Maps), so it keeps its id.

use crate::assets::replace_references;
use crate::{Error, Project, Result};
use std::fs;
use std::path::{Path, PathBuf};

impl Project {
    /// The files of map `name` (whether they exist or not).
    fn map_parts(&self, name: &str) -> [PathBuf; 2] {
        let scene = self.map_file(name);
        let data = c3_scene::native::data_path(&scene);
        [scene, data]
    }

    fn check_free(&self, name: &str) -> Result<()> {
        if self.map_file(name).exists() {
            return Err(Error::Asset(format!("a map called \"{name}\" exists")));
        }
        Ok(())
    }

    fn check_map(&self, name: &str) -> Result<()> {
        if !self.map_file(name).exists() {
            return Err(Error::Asset(format!("the project has no map \"{name}\"")));
        }
        Ok(())
    }

    /// Copies map `from` as a new map `to`.
    pub fn copy_map(&self, from: &str, to: &str) -> Result<()> {
        self.check_map(from)?;
        self.check_free(to)?;
        for (src, dst) in self.map_parts(from).iter().zip(self.map_parts(to)) {
            if src.exists() {
                fs::copy(src, &dst).map_err(io(src))?;
            }
        }
        Ok(())
    }

    /// Renames map `from` to `to`; the map list's entry follows.
    pub fn rename_map(&self, from: &str, to: &str) -> Result<()> {
        self.check_map(from)?;
        self.check_free(to)?;
        for (src, dst) in self.map_parts(from).iter().zip(self.map_parts(to)) {
            if src.exists() {
                fs::rename(src, &dst).map_err(io(src))?;
            }
        }
        let settings = self.settings_file();
        if let Ok(text) = fs::read_to_string(&settings) {
            let next = replace_references(&text, from, to);
            if next != text {
                fs::write(&settings, next).map_err(io(&settings))?;
            }
        }
        Ok(())
    }

    /// Moves map `name` to the trash; returns where it went, for [`Project::restore_map`].
    pub fn trash_map(&self, name: &str) -> Result<PathBuf> {
        self.check_map(name)?;
        let trash = self.trash_dir();
        let slot = (0..)
            .map(|n| trash.join(format!("map{n}")))
            .find(|p| !p.exists())
            .unwrap_or_else(|| trash.join("map"));
        fs::create_dir_all(&slot).map_err(io(&slot))?;
        for src in self.map_parts(name) {
            if let Some(file) = src.file_name().filter(|_| src.exists()) {
                fs::rename(&src, slot.join(file)).map_err(io(&src))?;
            }
        }
        Ok(slot)
    }

    /// Puts back map `name` moved to the trash slot `slot`.
    pub fn restore_map(&self, name: &str, slot: &Path) -> Result<()> {
        self.check_free(name)?;
        for dst in self.map_parts(name) {
            let Some(file) = dst.file_name() else {
                continue;
            };
            let src = slot.join(file);
            if src.exists() {
                fs::rename(&src, &dst).map_err(io(&src))?;
            }
        }
        let _ = fs::remove_dir_all(slot);
        Ok(())
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |e| Error::Io(path.to_path_buf(), e)
}

#[cfg(test)]
mod tests {
    use crate::{Manifest, Project};

    #[test]
    fn maps_copy_rename_and_come_back_from_the_trash() {
        let dir = std::env::temp_dir().join(format!("c3-map-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = Project::create(&dir, Manifest::new("Test")).unwrap();
        std::fs::create_dir_all(project.maps_dir()).unwrap();
        let parts = project.map_parts("A");
        std::fs::write(&parts[0], "scene").unwrap();
        std::fs::write(&parts[1], "cells").unwrap();
        std::fs::write(project.settings_file(), "[[maps]]\nkey = \"A\"\nid = 7\n").unwrap();
        project.copy_map("A", "B").unwrap();
        assert!(project.copy_map("A", "B").is_err());
        project.rename_map("A", "C").unwrap();
        let settings = std::fs::read_to_string(project.settings_file()).unwrap();
        assert!(settings.contains("key = \"C\""));
        let slot = project.trash_map("C").unwrap();
        assert_eq!(project.map_names(), ["B"]);
        project.restore_map("C", &slot).unwrap();
        assert_eq!(project.map_names(), ["B", "C"]);
        assert_eq!(
            std::fs::read_to_string(&project.map_parts("C")[1]).unwrap(),
            "cells"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
