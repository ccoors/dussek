//! Standard MIDI files as timed engine events, for the offline `midi2wav` renderer.
//!
//! Follows what the plugin does with a MIDI stream: omni channels, note-on with velocity 0
//! is a note-off, CC64 is the continuous sustain pedal, CC66 (≥ 64) the sostenuto pedal,
//! CC67 the continuous soft pedal, CC120/123 release everything; the mod wheel (CC1), channel
//! and key pressure are the clavichord's Bebung.
//! Tempo changes (from any track) and SMPTE time bases are honored.

use midly::{Smf, Timing, TrackEventKind, MetaMessage, MidiMessage};

/// What happens at a time (seconds from the start of the file).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    NoteOn { key: u8, velocity: u8 },
    /// `velocity`: the release velocity (0: not known, as for a note-on with velocity 0)
    KeyUp { key: u8, velocity: u8 },
    /// sustain pedal position 0 … 1
    Pedal { depth: f32 },
    Sostenuto { down: bool },
    /// soft pedal position 0 … 1
    Soft { depth: f32 },
    /// all sound off (CC120), after the releases it implies
    SoundOff,
    /// the mod wheel (CC1), 0 … 1: the clavichord's Bebung
    ModWheel { value: f32 },
    /// channel pressure (aftertouch), 0 … 1
    ChannelPressure { value: f32 },
    /// polyphonic key pressure, 0 … 1
    KeyPressure { key: u8, value: f32 },
    /// the pitch wheel, −1 … 1 (re its middle; the đàn tranh's bend)
    PitchBend { value: f32 },
}

#[derive(Debug, PartialEq)]
pub struct Score {
    /// in time order; at equal times as in the file
    pub events: Vec<(f64, Action)>,
    /// time of the last event
    pub length: f64,
}

/// Piano range of the engine (A0 … C8).
const LOW: u8 = 21;
const HIGH: u8 = 108;

/// Parse a Standard MIDI File.
pub fn parse(bytes: &[u8]) -> Result<Score, String> {
    let smf = Smf::parse(bytes).map_err(|e| format!("MIDI file: {e}"))?;
    // all events of all tracks on one tick axis (stable: track order at equal ticks)
    let mut raw: Vec<(u64, usize, TrackEventKind)> = Vec::new();
    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0u64;
        for ev in track {
            tick += ev.delta.as_int() as u64;
            raw.push((tick, ti, ev.kind));
        }
    }
    raw.sort_by_key(|&(t, ti, _)| (t, ti));

    // seconds per tick follows the tempo events; SMPTE timing is fixed
    let (ticks_per_beat, smpte_tick) = match smf.header.timing {
        Timing::Metrical(t) => (t.as_int() as f64, None),
        Timing::Timecode(fps, sub) => (1.0, Some(1.0 / (fps.as_f32() as f64 * sub as f64))),
    };
    let mut tempo = 500_000.0; // µs per beat
    let (mut t_s, mut last_tick) = (0.0f64, 0u64);

    let mut events = Vec::new();
    let mut down = [false; 128];
    let mut pedal = 0.0f32;
    let mut sostenuto = false;
    let mut soft = 0.0f32;
    let mut wheel = 0.0f32;
    let mut bend = 0.0f32;
    let mut chan = 0.0f32;
    for (tick, _, kind) in raw {
        let dt = (tick - last_tick) as f64;
        t_s += dt * smpte_tick.unwrap_or(tempo * 1e-6 / ticks_per_beat);
        last_tick = tick;
        match kind {
            TrackEventKind::Meta(MetaMessage::Tempo(t)) => tempo = t.as_int() as f64,
            TrackEventKind::Midi { message, .. } => match message {
                MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                    let k = key.as_int();
                    if (LOW..=HIGH).contains(&k) {
                        // a key struck again while held is released first, as a finger would
                        if down[k as usize] {
                            events.push((t_s, Action::KeyUp { key: k, velocity: 0 }));
                        }
                        down[k as usize] = true;
                        events.push((t_s, Action::NoteOn { key: k, velocity: vel.as_int() }));
                    }
                }
                MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                    let k = key.as_int();
                    // a note-off carries a release velocity; a note-on with velocity 0 does not
                    let velocity = if let MidiMessage::NoteOff { vel, .. } = message { vel.as_int() } else { 0 };
                    if (LOW..=HIGH).contains(&k) && down[k as usize] {
                        down[k as usize] = false;
                        events.push((t_s, Action::KeyUp { key: k, velocity }));
                    }
                }
                MidiMessage::PitchBend { bend: b } => {
                    let value = (b.as_int() as f32 / 8192.0).clamp(-1.0, 1.0);
                    if value != bend {
                        bend = value;
                        events.push((t_s, Action::PitchBend { value }));
                    }
                }
                MidiMessage::ChannelAftertouch { vel } => {
                    let value = vel.as_int() as f32 / 127.0;
                    if value != chan {
                        chan = value;
                        events.push((t_s, Action::ChannelPressure { value }));
                    }
                }
                MidiMessage::Aftertouch { key, vel } => {
                    let k = key.as_int();
                    if (LOW..=HIGH).contains(&k) {
                        events.push((t_s, Action::KeyPressure { key: k, value: vel.as_int() as f32 / 127.0 }));
                    }
                }
                MidiMessage::Controller { controller, value } => match controller.as_int() {
                    1 => {
                        let v = value.as_int() as f32 / 127.0;
                        if v != wheel {
                            wheel = v;
                            events.push((t_s, Action::ModWheel { value: v }));
                        }
                    }
                    64 => {
                        let depth = value.as_int() as f32 / 127.0;
                        if depth != pedal {
                            pedal = depth;
                            events.push((t_s, Action::Pedal { depth }));
                        }
                    }
                    66 => {
                        let down = value.as_int() >= 64;
                        if down != sostenuto {
                            sostenuto = down;
                            events.push((t_s, Action::Sostenuto { down }));
                        }
                    }
                    67 => {
                        let depth = value.as_int() as f32 / 127.0;
                        if depth != soft {
                            soft = depth;
                            events.push((t_s, Action::Soft { depth }));
                        }
                    }
                    120 | 123 => {
                        for k in LOW..=HIGH {
                            if down[k as usize] {
                                down[k as usize] = false;
                                events.push((t_s, Action::KeyUp { key: k, velocity: 0 }));
                            }
                        }
                        if pedal != 0.0 {
                            pedal = 0.0;
                            events.push((t_s, Action::Pedal { depth: 0.0 }));
                        }
                        if sostenuto {
                            sostenuto = false;
                            events.push((t_s, Action::Sostenuto { down: false }));
                        }
                        if soft != 0.0 {
                            soft = 0.0;
                            events.push((t_s, Action::Soft { depth: 0.0 }));
                        }
                        // CC120 all sound off: silence at once; CC123 all notes off: the releases alone
                        if controller.as_int() == 120 {
                            events.push((t_s, Action::SoundOff));
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }
    }
    let length = events.last().map_or(0.0, |e| e.0);
    Ok(Score { events, length })
}

#[cfg(test)]
mod tests {
    use super::*;
    use midly::{num::*, Format, Header, MetaMessage, Track, TrackEvent};

    fn ev(delta: u32, kind: TrackEventKind<'static>) -> TrackEvent<'static> {
        TrackEvent { delta: u28::new(delta), kind }
    }
    fn midi(message: MidiMessage) -> TrackEventKind<'static> {
        TrackEventKind::Midi { channel: u4::new(0), message }
    }
    fn on(key: u8, vel: u8) -> TrackEventKind<'static> {
        midi(MidiMessage::NoteOn { key: u7::new(key), vel: u7::new(vel) })
    }
    fn cc(c: u8, v: u8) -> TrackEventKind<'static> {
        midi(MidiMessage::Controller { controller: u7::new(c), value: u7::new(v) })
    }
    fn file(tracks: Vec<Track<'static>>, tpb: u16) -> Vec<u8> {
        let smf = Smf { header: Header::new(Format::Parallel, Timing::Metrical(u15::new(tpb))), tracks };
        let mut out = Vec::new();
        smf.write_std(&mut out).unwrap();
        out
    }

    #[test]
    fn tempo_changes_apply_across_tracks() {
        // 100 ticks per beat; 120 bpm (0.5 s/beat) for the first beat, then 60 bpm (1 s/beat)
        let conductor = vec![
            ev(0, TrackEventKind::Meta(MetaMessage::Tempo(u24::new(500_000)))),
            ev(100, TrackEventKind::Meta(MetaMessage::Tempo(u24::new(1_000_000)))),
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let notes = vec![ev(0, on(60, 90)), ev(200, on(60, 0)), ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack))];
        let s = parse(&file(vec![conductor, notes], 100)).unwrap();
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0], (0.0, Action::NoteOn { key: 60, velocity: 90 }));
        // 100 ticks at 0.5 s + 100 ticks at 1 s
        assert!((s.events[1].0 - 1.5).abs() < 1e-9);
        assert_eq!(s.events[1].1, Action::KeyUp { key: 60, velocity: 0 });
    }

    #[test]
    fn pedal_is_continuous_and_deduplicated() {
        let t = vec![
            ev(0, cc(64, 127)),
            ev(1, cc(64, 127)),
            ev(1, cc(64, 64)),
            ev(1, cc(64, 0)),
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let s = parse(&file(vec![t], 480)).unwrap();
        let depths: Vec<f32> = s.events.iter().map(|e| if let Action::Pedal { depth } = e.1 { depth } else { -1.0 }).collect();
        assert_eq!(depths, vec![1.0, 64.0 / 127.0, 0.0]);
    }

    #[test]
    fn sostenuto_is_on_off_at_64_and_panic_releases_it() {
        let t = vec![
            ev(0, cc(66, 63)),
            ev(1, cc(66, 64)),
            ev(1, cc(66, 127)),
            ev(1, cc(66, 10)),
            ev(1, cc(66, 100)),
            ev(1, cc(123, 0)),
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let s = parse(&file(vec![t], 480)).unwrap();
        let a: Vec<Action> = s.events.iter().map(|e| e.1).collect();
        let (d, u) = (Action::Sostenuto { down: true }, Action::Sostenuto { down: false });
        assert_eq!(a, vec![d, u, d, u]);
    }

    #[test]
    fn all_sound_off_silences_after_the_releases_all_notes_off_only_releases() {
        for (c, last) in [(120u8, Some(Action::SoundOff)), (123, None)] {
            let t = vec![ev(0, cc(64, 127)), ev(1, cc(c, 0)), ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack))];
            let s = parse(&file(vec![t], 480)).unwrap();
            let a: Vec<Action> = s.events.iter().map(|e| e.1).collect();
            let mut want = vec![Action::Pedal { depth: 1.0 }, Action::Pedal { depth: 0.0 }];
            want.extend(last);
            assert_eq!(a, want, "CC{c}");
        }
    }

    #[test]
    fn soft_pedal_is_continuous_deduplicated_and_reset_by_a_panic() {
        let t = vec![
            ev(0, cc(67, 127)),
            ev(1, cc(67, 127)),
            ev(1, cc(67, 64)),
            ev(1, cc(67, 100)),
            ev(1, cc(123, 0)),
            ev(1, cc(67, 0)), // already 0 after the panic: no event
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let s = parse(&file(vec![t], 480)).unwrap();
        let depths: Vec<f32> = s.events.iter().map(|e| if let Action::Soft { depth } = e.1 { depth } else { -1.0 }).collect();
        assert_eq!(depths, vec![1.0, 64.0 / 127.0, 100.0 / 127.0, 0.0]);
    }

    #[test]
    fn the_pitch_wheel_is_read_re_its_middle() {
        let pb = |v: i16| midi(MidiMessage::PitchBend { bend: midly::PitchBend::from_int(v) });
        let t = vec![
            ev(0, pb(8191)),
            ev(1, pb(8191)), // unchanged: no event
            ev(1, pb(-8192)),
            ev(1, pb(0)),
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let s = parse(&file(vec![t], 480)).unwrap();
        let a: Vec<Action> = s.events.iter().map(|e| e.1).collect();
        assert_eq!(
            a,
            vec![Action::PitchBend { value: 8191.0 / 8192.0 }, Action::PitchBend { value: -1.0 }, Action::PitchBend { value: 0.0 }]
        );
    }

    #[test]
    fn the_mod_wheel_and_aftertouch_are_the_bebungs_pressure() {
        let chan = |v: u8| midi(MidiMessage::ChannelAftertouch { vel: u7::new(v) });
        let poly = |k: u8, v: u8| midi(MidiMessage::Aftertouch { key: u7::new(k), vel: u7::new(v) });
        let t = vec![
            ev(0, cc(1, 127)),
            ev(1, cc(1, 127)), // unchanged: no event
            ev(1, cc(1, 0)),
            ev(1, chan(64)),
            ev(1, poly(69, 127)),
            ev(1, poly(10, 127)), // outside the keyboard: none
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let s = parse(&file(vec![t], 480)).unwrap();
        let a: Vec<Action> = s.events.iter().map(|e| e.1).collect();
        assert_eq!(
            a,
            vec![
                Action::ModWheel { value: 1.0 },
                Action::ModWheel { value: 0.0 },
                Action::ChannelPressure { value: 64.0 / 127.0 },
                Action::KeyPressure { key: 69, value: 1.0 },
            ]
        );
    }

    #[test]
    fn a_note_off_carries_its_release_velocity() {
        let off = |key: u8, vel: u8| midi(MidiMessage::NoteOff { key: u7::new(key), vel: u7::new(vel) });
        let t = vec![
            ev(0, on(60, 80)),
            ev(0, on(64, 80)),
            ev(10, off(60, 100)),
            ev(10, off(64, 0)), // some devices send 0: not known
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let s = parse(&file(vec![t], 480)).unwrap();
        let a: Vec<Action> = s.events.iter().map(|e| e.1).collect();
        assert_eq!(a[2], Action::KeyUp { key: 60, velocity: 100 });
        assert_eq!(a[3], Action::KeyUp { key: 64, velocity: 0 });
    }

    #[test]
    fn restrike_releases_first_and_panic_releases_all() {
        let t = vec![
            ev(0, on(60, 80)),
            ev(10, on(60, 80)),
            ev(0, on(64, 80)),
            ev(10, cc(123, 0)),
            ev(0, on(10, 80)), // out of range: dropped
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let s = parse(&file(vec![t], 480)).unwrap();
        let a: Vec<Action> = s.events.iter().map(|e| e.1).collect();
        assert_eq!(
            a,
            vec![
                Action::NoteOn { key: 60, velocity: 80 },
                Action::KeyUp { key: 60, velocity: 0 },
                Action::NoteOn { key: 60, velocity: 80 },
                Action::NoteOn { key: 64, velocity: 80 },
                Action::KeyUp { key: 60, velocity: 0 },
                Action::KeyUp { key: 64, velocity: 0 },
            ]
        );
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(parse(b"not a midi file").is_err());
    }
}
