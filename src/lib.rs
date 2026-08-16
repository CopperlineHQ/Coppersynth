// The Copperline-facing layer: the engine, the MT-32 -> GM translation
// and the front panel. The synthesis core lives in `synth`, Coppersynth's
// derivative of RustySynth -- public so the tests can reach it, but its
// surface is this crate's own business.

pub mod engine;
pub mod mt32;
pub mod panel;
pub mod synth;

/// The engine's own version, for logs and About lines.
pub fn version() -> String {
    format!(
        "Coppersynth {} (RustySynth-derived core)",
        env!("CARGO_PKG_VERSION")
    )
}
