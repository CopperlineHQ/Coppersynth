use std::process::Command;

// The splash screen dates itself from the built commit, so a release
// carries its own day without anyone maintaining a string. Falls back
// to empty (and the splash omits it) outside a git checkout.
fn main() {
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
