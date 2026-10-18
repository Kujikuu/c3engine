//! Prints a character's parts, clips and socket transforms, to debug attachments.
//! Usage: char_debug [look] [armor] [r_weapon]
use game_conquer::Content;
use game_conquer::character::{RoleLook, action};

fn main() {
    let a: Vec<u32> = std::env::args()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    let look = RoleLook {
        look: *a.first().unwrap_or(&3),
        hair: 310,
        armor: *a.get(1).unwrap_or(&130003),
        r_weapon: *a.get(2).unwrap_or(&410003),
        ..Default::default()
    };
    let c = Content::open(r"D:\Conquer2\Tools\eosource\client5065").unwrap();
    let ch = c.assemble_character(&look, action::STANDBY).unwrap();
    println!("motion {:?}", ch.motion_id);
    for (i, p) in ch.body.model.parts.iter().enumerate() {
        let clip = p.clip.as_ref().map(|c| {
            format!(
                "{} bones {} frames {} keys",
                c.bone_count,
                c.frame_count,
                c.keys.len()
            )
        });
        println!(
            "body[{i}] {:12} visible={} verts={} {:?}",
            p.mesh.name,
            p.visible,
            p.mesh.vertices.len(),
            clip
        );
    }
    for s in ["v_r_weapon", "v_l_weapon", "v_armet"] {
        if let Some(m) = ch.body.model.socket_transform(s, 0.0) {
            println!(
                "{s} bone0 @0: translation {:?}\n   cols {:?}",
                m.w_axis,
                [m.x_axis, m.y_axis, m.z_axis]
            );
        }
        if let Some(p) = ch.body.model.part(s) {
            let (lo, hi) = p.mesh.bounds;
            let posed = p.mesh.posed_positions(p.clip.as_ref(), 0.0);
            let c = posed.iter().copied().sum::<glam::Vec3>() / posed.len().max(1) as f32;
            println!(
                "   socket mesh bind bbox {lo:?}..{hi:?}, posed centre {c:?}, bind {:?}",
                p.mesh.bind_pose.w_axis
            );
        }
    }
    for a in &ch.attachments {
        for p in &a.part.model.parts {
            let posed = p.mesh.posed_positions(p.clip.as_ref(), 0.0);
            let c = posed.iter().copied().sum::<glam::Vec3>() / posed.len().max(1) as f32;
            let clip = p.clip.as_ref().map(|c| (c.bone_count, c.frame_count));
            println!(
                "attach {} part {} verts={} clip={clip:?} posed centre {c:?}",
                a.socket,
                p.mesh.name,
                p.mesh.vertices.len()
            );
        }
    }
}
