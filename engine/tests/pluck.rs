//! The pluck excitation on a real instrument (Salamander's C4 strings, excited by a pluck
//! at β = 0.2): the pluck point's notch, repeats, extreme velocities, the hammer
//! instruments' loading and the handover. Needs the exported instruments (skipped without).

use dussek::physics::{Excitation, PluckSpec};
use dussek::{Engine, Event, Instrument, Tuning};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/grand_a");
const SR: f64 = 48000.0;
const PLUCK: PluckSpec = PluckSpec::string(0.2, 5232.0, 1.5e-3, 3.0, 0.5);

fn plucked(t: Tuning) -> Option<Engine> {
    let mut inst = Instrument::load(DIR).ok()?;
    inst.notes[39].spec.excitation = Excitation::Pluck(PLUCK);
    // the notes were compiled at load: compile the changed spec
    inst.compile(SR, t);
    Some(Engine::with_tuning(inst, SR, t).expect("engine"))
}

fn render(e: &mut Engine, n: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        e.process(cl, cr);
    }
    l
}

fn level(x: &[f32], f: f64) -> f64 {
    let n = x.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n).cos();
        let ph = 2.0 * std::f64::consts::PI * f * i as f64 / SR;
        re += w * v as f64 * ph.cos();
        im -= w * v as f64 * ph.sin();
    }
    20.0 * (re.hypot(im) + 1e-30).log10()
}

#[test]
fn hammer_exports_load_as_hammer() {
    let Ok(inst) = Instrument::load(DIR) else { return };
    assert!(inst.notes.iter().all(|n| matches!(n.spec.excitation, Excitation::Hammer(_))));
}

#[test]
fn pluck_point_notch_in_the_output() {
    let Some(mut e) = plucked(Tuning::default()) else { return };
    e.queue(0, Event::NoteOn { key: 60, speed: 1.0 });
    let x = render(&mut e, 72000);
    assert!(x.iter().all(|v| v.is_finite()));
    let f = e.instrument().notes[39].f_mean.clone();
    let seg = &x[14400..62400];
    let lv: Vec<f64> = [4usize, 5, 6].iter().map(|&k| level(seg, f[k - 1])).collect();
    assert!(lv[1] < lv[0].min(lv[2]) - 15.0, "{lv:?} (f0 {})", f[0]);
}

#[test]
fn repluck_and_extreme_velocities_stay_bounded() {
    if Instrument::load(DIR).is_err() {
        return;
    }
    let peak = |events: &[(u64, f64)]| -> f32 {
        let mut e = plucked(Tuning::default()).expect("engine");
        for &(t, speed) in events {
            e.queue(t, Event::NoteOn { key: 60, speed });
        }
        let x = render(&mut e, 24000);
        assert!(x.iter().all(|v| v.is_finite()));
        x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
    };
    let one = peak(&[(0, 1.0)]);
    assert!(peak(&[(0, 1.0), (480, 1.0)]) < 2.2 * one);
    let soft = peak(&[(0, 1e-9)]);
    assert!(soft > 0.0 && soft < peak(&[(0, 0.5)]));
}

#[test]
fn plucked_notes_survive_a_rebuild() {
    let (Some(mut a), Some(mut b)) = (plucked(Tuning::default()), plucked(Tuning { pitch_ratio: 1.01, ..Tuning::default() })) else { return };
    a.queue(0, Event::NoteOn { key: 60, speed: 1.0 });
    let before = render(&mut a, 9600);
    assert!(b.adopt(&mut a));
    let after = render(&mut b, 9600);
    let rms = |x: &[f32]| (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
    assert!(rms(&after[..2400]) > 0.3 * rms(&before[7200..]), "gap after the handover");
}

const HARPSICHORD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../instruments/harpsichord");
/// the first output sample after a pluck is m = 2 after the release (prototype:
/// string_model.render skips two free-decay samples; engine: Modes::add)
const OFFSET: f64 = 2.0;

/// Engine vs prototype for a plucked note of the real (format-5) export: the voice's
/// vertical bridge force equals the closed-form modal sum the prototype renders,
/// Σ_j bridge_j·a_j·Re(e^{λ_j (n + 2) dt}) (single strings: V = 1), up to the pruning of
/// tiny modes and float32 running.
#[test]
fn harpsichord_pluck_matches_the_closed_form() {
    use dussek::kernel::{self, LANES};
    use dussek::physics::pluck_amplitude;
    use dussek::voice::Voice;
    let Ok(mut inst) = Instrument::load(HARPSICHORD) else { return };
    inst.compile(SR, Tuning::default());
    assert!(inst.notes.iter().all(|n| matches!(n.spec.excitation, Excitation::Pluck(p) if (0.04..=0.5).contains(&p.beta))));
    let note = &inst.notes[39];
    let p = note.pluck.expect("pluck");
    let mut v = Voice::new(note, SR, inst.reference_sr, inst.contact_max_s, inst.contact_separation_s, 64);
    v.set_engage(0.0);
    v.pluck(note, 1.0, 1.0);
    let (mut av, mut ah, mut at) = (vec![[0.0f32; LANES]; 64], vec![[0.0f32; LANES]; 64], vec![[0.0f32; LANES]; 64]);
    let (mut ov, mut oh) = (vec![0.0; 64], vec![0.0; 64]);
    v.render(note, &mut av, &mut ah, &mut at, &mut ov, &mut oh);
    kernel::lane_sums(&av, &mut ov);
    let dt = 1.0 / SR;
    let kn = note.f_mean.iter().take_while(|&&f| f < 0.45 * SR).count();
    let expected: Vec<f64> = (0..64)
        .map(|n| {
            (0..kn)
                .map(|j| {
                    let a = pluck_amplitude((j + 1) as f64, note.f_mean[j], &p, 0.0, 1.0);
                    note.bridge[j] * a * (note.vert.lam[j] * ((n as f64 + OFFSET) * dt)).exp().re
                })
                .sum()
        })
        .collect();
    let scale = expected.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let err = ov.iter().zip(&expected).fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
    assert!(err < 1e-3 * scale, "max deviation {:.2e} of {:.2e}", err, scale);
}
