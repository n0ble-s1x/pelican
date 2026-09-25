//! Does the tag reader agree with what these files actually contain?
use pelican_core::transcode::tags::Tags;
use pelican_core::transfer;

fn main() {
    let root = std::env::args().nth(1).expect("usage: check_tags <dir>");
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .expect("read dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| pelican_core::transcode::is_audio(p))
        .collect();
    files.sort();
    println!("{} audio files in {root}\n", files.len());
    for p in files.iter().take(12) {
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        match Tags::read(p) {
            Ok(t) => println!(
                "  {:<44} title={:<28} artist={:<22} playable={} would_flag={}",
                truncate(&name, 44),
                truncate(&t.title.clone().unwrap_or_else(|| "<NONE>".into()), 28),
                truncate(&t.artist.clone().unwrap_or_else(|| "<NONE>".into()), 22),
                t.playable_in_library(),
                !transfer::has_required_tags(p),
            ),
            Err(e) => println!("  {name}: READ ERROR {e}"),
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
