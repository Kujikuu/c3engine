//! c3-anim: skeletal animation clips, sampling and skinning.
//!
//! Matrices are `glam::Mat4` (column vectors). D3D-style row-major/row-vector matrices have
//! the same memory layout, so importers load them with `Mat4::from_cols_array` unchanged.

use glam::{Mat4, Vec3};

/// Bone transforms at one key frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub frame: u32,
    /// One matrix per bone.
    pub bones: Vec<Mat4>,
}

/// A skeletal animation: absolute bone matrices at (possibly sparse) key frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub bone_count: usize,
    /// Length in frames; sampling wraps at this value.
    pub frame_count: u32,
    /// Sorted by frame.
    pub keys: Vec<Key>,
}

impl Clip {
    /// Bone matrix at a (fractional) frame: linear blend of the surrounding keys,
    /// matching the classic C3 runtime (`Motion_GetMatrixAtFrame`).
    pub fn sample_bone(&self, bone: usize, frame: f32) -> Mat4 {
        if self.keys.is_empty() || bone >= self.bone_count {
            return Mat4::IDENTITY;
        }
        let frame = if self.frame_count > 0 {
            frame.rem_euclid(self.frame_count as f32)
        } else {
            frame
        };
        let next = self.keys.partition_point(|k| k.frame as f32 <= frame);
        match (
            next.checked_sub(1).map(|i| &self.keys[i]),
            self.keys.get(next),
        ) {
            (Some(a), Some(b)) => {
                let t = (frame - a.frame as f32) / (b.frame as f32 - a.frame as f32);
                if t <= 0.0 {
                    a.bones[bone]
                } else {
                    lerp(&a.bones[bone], &b.bones[bone], t)
                }
            }
            (Some(a), None) => a.bones[bone],
            (None, Some(b)) => b.bones[bone],
            (None, None) => Mat4::IDENTITY,
        }
    }

    /// All bone matrices at a frame.
    pub fn sample(&self, frame: f32) -> Vec<Mat4> {
        (0..self.bone_count)
            .map(|b| self.sample_bone(b, frame))
            .collect()
    }
}

fn lerp(a: &Mat4, b: &Mat4, t: f32) -> Mat4 {
    *a + (*b - *a) * t
}

/// Skins one position by blending bone matrices by weight (C3 `Phy_Calculate`).
/// Zero-weight or out-of-range bones are skipped; if none apply, bone 0 is used.
pub fn skin_position(pos: Vec3, bones: [u32; 2], weights: [f32; 2], pose: &[Mat4]) -> Vec3 {
    let mut blend = Mat4::ZERO;
    let mut any = false;
    for (&b, &w) in bones.iter().zip(&weights) {
        if w != 0.0 && (b as usize) < pose.len() {
            blend += pose[b as usize] * w;
            any = true;
        }
    }
    if !any {
        match pose.first() {
            Some(m) => blend = *m,
            None => return pos,
        }
    }
    blend.project_point3(pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip() -> Clip {
        let t = |x: f32| Mat4::from_translation(Vec3::new(x, 0.0, 0.0));
        Clip {
            bone_count: 1,
            frame_count: 10,
            keys: vec![
                Key {
                    frame: 0,
                    bones: vec![t(0.0)],
                },
                Key {
                    frame: 4,
                    bones: vec![t(8.0)],
                },
            ],
        }
    }

    #[test]
    fn samples_exact_interpolated_held_and_wrapped() {
        let c = clip();
        let x = |f: f32| c.sample_bone(0, f).w_axis.x;
        assert_eq!(x(0.0), 0.0);
        assert_eq!(x(2.0), 4.0);
        assert_eq!(x(4.0), 8.0);
        assert_eq!(x(7.0), 8.0, "holds after the last key");
        assert_eq!(x(12.0), 4.0, "wraps at frame_count");
    }

    #[test]
    fn skinning_blends_by_weight_and_falls_back_to_bone0() {
        let pose = [
            Mat4::from_translation(Vec3::X * 2.0),
            Mat4::from_translation(Vec3::Y * 4.0),
        ];
        let p = skin_position(Vec3::ZERO, [0, 1], [0.5, 0.5], &pose);
        assert_eq!(p, Vec3::new(1.0, 2.0, 0.0));
        let p = skin_position(Vec3::ZERO, [7, 9], [1.0, 0.0], &pose);
        assert_eq!(p, Vec3::new(2.0, 0.0, 0.0));
    }
}
