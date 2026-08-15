// The Copperline-facing layer: the MT-32 -> GM translation and the device
// API grow here. The synth core stays in the forked `rustysynth` crate so
// changes there remain upstreamable.

pub mod engine;
pub mod mt32;

/// The engine's own version, for logs and About lines.
pub fn version() -> String {
    format!(
        "Coppersynth {} (rustysynth {} fork)",
        env!("CARGO_PKG_VERSION"),
        "1.3.6"
    )
}
