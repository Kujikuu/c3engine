//! Tries item icon paths in a client folder (for finding how icons are named).
//! Usage: cargo run -p game-conquer --example icon_probe -- <client folder> <item type>...

fn main() {
    let mut args = std::env::args().skip(1);
    let content = game_conquer::Content::open(args.next().expect("client folder")).unwrap();
    for kind in args {
        let kind: u32 = kind.parse().unwrap();
        for k in [
            kind,
            kind / 10 * 10,
            kind / 1000 * 1000,
            kind / 100 * 100 + 5,
            kind / 10 * 10 + 5,
        ] {
            for dir in ["data/ItemMinIcon", "data/itemminicon", "data/MapItemIcon"] {
                let path = format!("{dir}/{k}.dds");
                if content.load_image(&path).is_some() {
                    println!("{kind}: {path}");
                }
            }
        }
    }
}
