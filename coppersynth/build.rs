use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

// Two stamps at build time: the built commit's date for the splash
// screen, and the bundled soundfont -- GeneralUser GS, fetched from its
// own repository so a fresh build carries the current release, zipped
// with its licence text into the library. Delete the cached copy in
// assets/ to pick up a newer release.
fn main() {
    release_date();
    bundle_soundfont();
}

fn release_date() {
    let date = Command::new("git")
        .args(["log", "-1", "--format=%cs"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=COPPERSYNTH_RELEASE_DATE={date}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
}

const FONT_URL: &str =
    "https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/main/GeneralUser-GS.sf2";
const LICENCE_URL: &str =
    "https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/main/documentation/LICENSE.txt";

fn bundle_soundfont() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bundled-bank.zip");
    let assets = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .join("assets");
    let font = assets.join("GeneralUser-GS.sf2");
    let licence = assets.join("GeneralUser-GS.LICENSE.txt");
    println!("cargo:rerun-if-changed={}", font.display());

    let _ = std::fs::create_dir_all(&assets);
    if !font.is_file() {
        fetch(FONT_URL, &font);
    }
    if !licence.is_file() {
        fetch(LICENCE_URL, &licence);
    }

    let Ok(data) = std::fs::read(&font) else {
        // No cache and no network: the library still builds, without
        // its bundled bank, and says so at runtime.
        println!(
            "cargo:warning=no GeneralUser-GS.sf2 (offline?); building without the bundled bank"
        );
        std::fs::write(&out, []).unwrap();
        return;
    };
    let file = std::fs::File::create(&out).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    zip.start_file("GeneralUser-GS.sf2", options).unwrap();
    zip.write_all(&data).unwrap();
    if let Ok(text) = std::fs::read(&licence) {
        zip.start_file("GeneralUser-GS.LICENSE.txt", options)
            .unwrap();
        zip.write_all(&text).unwrap();
    }
    zip.finish().unwrap();
}

fn fetch(url: &str, to: &Path) {
    let tmp = to.with_extension("part");
    let ok = Command::new("curl")
        .args(["-LfsS", "-o"])
        .arg(&tmp)
        .arg(url)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        let _ = std::fs::rename(&tmp, to);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}
