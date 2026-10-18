//! Reports whether paths exist in the client's archives and/or as loose files.
//! Usage: find_packed <client_dir> <path>...
use c3_formats_tq::wdf::Wdf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((dir, paths)) = args.split_first() else {
        eprintln!("usage: find_packed <client_dir> <path>...");
        std::process::exit(2);
    };
    let archives: Vec<(String, Wdf)> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wdf")))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                Wdf::open(&p).unwrap(),
            )
        })
        .collect();
    for path in paths {
        let loose = std::path::Path::new(dir).join(path);
        let loose = std::fs::metadata(&loose)
            .map(|m| format!("{} bytes", m.len()))
            .unwrap_or_else(|_| "absent".into());
        let packed: Vec<String> = archives
            .iter()
            .filter_map(|(n, a)| a.find(path).map(|e| format!("{n} ({} bytes)", e.size)))
            .collect();
        println!("{path}: loose {loose}; packed {packed:?}");
    }
}
