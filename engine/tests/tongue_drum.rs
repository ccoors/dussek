//! The tongue drum (`projects/tongue_drum`): nearly pure tongues, the shell's own mode at 758 Hz on
//! every key, the other tongues ringing along (`halo`), no dampers. Needs the exported instrument
//! (skipped without).

use dussek::{Engine, Event, Instrument, Params, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/tongue_drum");
const SR: f64 = 48000.0;
/// the drum's 15 tongues: C major, E3–E5
const TONGUES: [u8; 15] = [52, 53, 55, 57, 59, 60, 62, 64, 65, 67, 69, 71, 72, 74, 76];
const SHELL_HZ: f64 = 758.0;

fn pitch(key: f64) -> f64 {
    440.0 * 2f64.powf((key - 69.0) / 12.0)
}

#[test]
fn a_tongue_rings_with_the_shell_and_the_other_tongues() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.body() == dussek::mic::Body::TongueDrum && !inst.motor);
    for key in [52u8, 60, 67, 76] {
        let n = &inst.notes[key as usize - 21];
        let st = &n.spec.string;
        // tuned within 7 cents of its name, as recorded
        assert!((1200.0 * (st.f0 / pitch(key as f64)).log2()).abs() < 10.0, "key {key}: {:.2} Hz", st.f0);
        assert!(!n.spec.has_damper, "key {key}: a tongue drum has no dampers");
        let m = st.modes.as_ref().expect("a tongue's modes");
        let own = &m.ratio[..m.halo_from];
        assert!(own.iter().any(|r| (r * st.f0 / SHELL_HZ - 1.0).abs() < 0.01), "key {key}: no shell mode in {own:?}");
        // the ringing tongues (from `halo_from`, then their onset partners: each strike rises from
        // zero over 1 ms) at the other tongues' pitches
        let h = &m.ratio[m.halo_from..];
        assert!(h.len() % 2 == 0 && h[h.len() / 2..] == h[..h.len() / 2], "key {key}: the ringing tongues' partners");
        let halo: Vec<f64> = h[..h.len() / 2].iter().map(|r| r * st.f0).collect();
        assert!(!halo.is_empty() && halo.len() < TONGUES.len(), "key {key}: {} ringing tongues", halo.len());
        for f in &halo {
            assert!(TONGUES.iter().any(|&t| (1200.0 * (f / pitch(t as f64)).log2()).abs() < 15.0), "key {key}: {f:.1} Hz is no tongue");
        }
    }
}

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

#[test]
fn a_released_tongue_rings_on() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let level = |hold: u64| -> f64 {
        let mut e = Engine::with_tuning(Instrument::load(DIR).unwrap(), SR, Tuning::default()).unwrap();
        e.set_params(Params { room: 0.0, ..Params::default() });
        e.queue(0, Event::NoteOn { key: 60, speed: 1.0 });
        e.queue(hold, Event::KeyUp { key: 60 });
        let x = render(&mut e, (1.5 * SR) as usize);
        (x[(1.0 * SR) as usize..].iter().map(|&v| (v as f64).powi(2)).sum::<f64>()).sqrt()
    };
    let (held, released) = (level((1.5 * SR) as u64), level((0.1 * SR) as u64));
    assert!((20.0 * (released / held).log10()).abs() < 0.5, "held {held:.3e}, released {released:.3e}");
}
