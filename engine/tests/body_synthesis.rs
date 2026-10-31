//! The engine's body synthesis (mic.rs `synth_body`, for the microphone positions) reproduces the
//! exported response at the recording's position (`proto/board.py` `impulse_response`): the
//! board's modes at flat weights, then the transfer H as one minimum-phase filter. Needs the
//! exports (skipped without).

use pianoengine::Instrument;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/");

#[test]
fn the_synthesis_reproduces_the_exported_body_response() {
    for name in ["grand_a", "upright_a", "harpsichord", "harp", "guitar"] {
        let Ok(inst) = Instrument::load(format!("{ROOT}{name}")) else { continue };
        let reference = &inst.radiation_v.body.ir;
        let len = reference[0].len();
        let synth = pianoengine::mic::synth_reference(&inst.globals.board, &inst.body_h, inst.reference_sr, len);
        for (c, (r, s)) in reference.iter().zip(&synth).enumerate() {
            let dot: f64 = r.iter().zip(s).map(|(&a, &b)| a as f64 * b).sum();
            let (er, es) = (r.iter().map(|&a| (a as f64).powi(2)).sum::<f64>(), s.iter().map(|b| b * b).sum::<f64>());
            let corr = dot / (er * es).sqrt();
            assert!(corr > 0.999, "{name} channel {c}: correlation {corr}");
        }
    }
}
