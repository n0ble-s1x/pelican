fn main() -> Result<(), Box<dyn std::error::Error>> {
    let src = std::env::args().nth(1).expect("path arg");
    let p = std::path::Path::new(&src);
    let t = krypteia_mtp_sync::transcode::normalize(p)?;
    println!("Output: {}", t.path.display());
    let out = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_format", &t.path.display().to_string()])
        .output()?;
    print!("{}", String::from_utf8_lossy(&out.stdout));
    // Don't drop — keep the file for inspection
    std::mem::forget(t);
    Ok(())
}
