//! Microphone positions, step 1: distance and the main reflections.
//!
//! The fitted radiation (`instrument::RadiationParts`: the soundboard's response per
//! channel, plus the diffuse room) is the sound at the recording's microphones, taken
//! as direct sound from REF_DISTANCE. A position moves the two microphones in a simple
//! model of a grand on a floor with its lid open on full stick. Each note drives the
//! board at its own bridge point (step 2: bridge zones, `zones`); per zone and
//! microphone the board's response for that excitation arrives along
//!   - the direct path: level REF_DISTANCE/R, delay R/c, air absorption,
//!   - the floor reflection (mirror source under the floor, FLOOR_REFLECTION),
//!   - the lid reflection (mirror source in the lid's plane, only if the reflection
//!     point lies on the lid, LID_REFLECTION),
//!
//! (close positions: the pair under the open lid; a microphone above the case gets no
//! floor reflection)
//! while the diffuse room stays as it is (it is the same everywhere in a room). So the
//! direct-to-room ratio follows the distance; the total level is kept, so switching
//! positions does not jump in loudness. The soundboard radiates the same in every
//! direction here (directivity is a later step), so all paths carry its response.
//!
//! Coordinates (m): origin at the soundboard's center, x towards the treble / the
//! curved side (where the audience sits), y towards the tail, z up.
//!
//! The upright (`Body::Upright`): the board stands in the x–z plane against a wall,
//! radiating from its back face (lowpass 4 kHz: ribs and back posts); its front face's
//! sound leaves through the open top (unobstructed) and the upper front panel (lowpass
//! 2 kHz), each −6 dB;
//! one-sided sources reach their back side by diffraction around the case (−10 dB,
//! lowpass 500 Hz); reflectors floor, wall (0.15 m behind the board, 1.2 m for close 2)
//! and the top lid leaning back.
//!
//! The harp (`Body::Harp`): no lid, no case around the board. The soundboard is the
//! soundbox's front, a long strip leaning back from the floor towards the player (−y) by
//! H_TILT_DEG; the strings drive it along its center line, the bass at its foot, the
//! treble at its top. Its face towards the strings (+y, up) radiates into the room; its
//! inner face into the soundbox, whose sound holes in the back face the player (−6 dB,
//! lowpass 600 Hz: the box's air carries the low end). Reflector: the floor.
//!
//! The vibraphone, the marimba, the xylophone, the bells and the crotales (`Body::Vibraphone`,
//! `Body::Marimba`, `Body::Xylophone`, `Body::Bells`, `Body::Crotales`, `Body::Chimes`, their `BarFrame`):
//! bars on a frame at hip height, the naturals in the row nearer the player (−y), the
//! accidentals behind them, the bass to the player's left; the resonator tubes hang below
//! each bar (the bells have none: a shallow case; the crotales are discs on a bar; the chimes'
//! "bars" are tubes hanging in a rack, the plane at their middles and their lower halves below it). A bar radiates from both faces, the resonator's mouth just under it (one source
//! at the bar, two-sided). Reflector: the floor.
//!
//! The guitar (`Body::Guitar`): a classical guitar held by a seated player, its top upright
//! in the x–z plane facing the audience (+y), the neck to the player's left (−x), the bass
//! strings on top. The strings drive the top at the bridge (one point per string); the top
//! radiates from its face, the sound hole (towards the neck) the body's air (−3 dB, lowpass
//! 500 Hz). Reflector: the floor.

use realfft::RealFftPlanner;
use realfft::num_complex::Complex64;

#[cfg(test)]
use crate::instrument::Radiation;

/// distance of the recording's microphones the fitted sound stands for (m)
const REF_DISTANCE: f64 = 1.0;
const SPEED_OF_SOUND: f64 = 343.0;
/// closest microphone distance from the board's center (m): outside the case
const MIN_DISTANCE: f64 = 1.0;
/// soundboard above the floor (m)
const BOARD_HEIGHT: f64 = 0.75;
const FLOOR_REFLECTION: f64 = 0.8;
/// lid: hinged along the spine (x, z of the hinge), opened by LID_ANGLE, LID_WIDTH wide,
/// from LID_Y.0 to LID_Y.1 along the piano
const LID_HINGE: (f64, f64) = (-0.72, 0.1);
const LID_ANGLE_DEG: f64 = 40.0;
const LID_WIDTH: f64 = 1.45;
const LID_Y: (f64, f64) = (-0.9, 1.0);
const LID_REFLECTION: f64 = 0.9;
/// the case's footprint (x, y ranges): a microphone above it does not get the floor
/// reflection (the case is in the way)
const CASE_X: (f64, f64) = (-0.75, 0.75);
const CASE_Y: (f64, f64) = (-0.95, 1.05);
/// a bridge zone radiates from an area, not a point: within about this radius its
/// level stops growing as 1/R (close positions)
const ZONE_RADIUS: f64 = 0.2;
/// close positions: the pair's half spacing at most (within the case), and the
/// microphones' height above the soundboard (m): at least, and below the lid by
const CLOSE_HALF_SPACING: f64 = 0.55;
const CLOSE_MIN_HEIGHT: f64 = 0.08;
const CLOSE_LID_CLEARANCE: f64 = 0.05;

// ---- the upright (assumed geometry; coordinates as for the grand: origin at the
// board's center, x towards the treble, y towards the back / the wall, z up). The
// board stands in the x–z plane and radiates from its back face; its front face
// radiates into the case, and that sound leaves through the open top.
/// board center above the floor (m)
const U_BOARD_HEIGHT: f64 = 0.75;
/// the case: x and y ranges (keys in front, back posts behind the board), its top
/// above the board center (m)
const U_CASE_X: (f64, f64) = (-0.75, 0.75);
const U_CASE_Y: (f64, f64) = (-0.65, 0.10);
const U_CASE_TOP: f64 = 0.55;
/// the back face's lowpass: its ribs and the back posts are on that side (Hz)
const U_BACK_LOWPASS_HZ: f64 = 4000.0;
/// the wall behind the board (y, m): against the wall, and pulled out for Close 2: back
const U_WALL: f64 = 0.15;
const U_WALL_BACK: f64 = 1.2;
const U_WALL_REFLECTION: f64 = 0.8;
/// the front face's sound leaves the case two ways, each −6 dB: through the open top
/// (a source in front of the board (y), radiating up, unobstructed) and through the
/// upper front panel above the keys (radiating forwards, lowpassed: panel and keyboard
/// in the way; its center's height above the board center)
const U_FRONT_Y: f64 = -0.10;
const U_FRONT_GAIN: f64 = 0.501_187_233_627_272_3;
const U_FRONT_LOWPASS_HZ: f64 = 2000.0;
const U_PANEL_Z: f64 = 0.25;
/// the top lid: hinged along the case top's back edge, opened U_LID_ANGLE_DEG from
/// closed (closed it lies towards the front), so at 95° it leans back towards the wall
const U_LID_ANGLE_DEG: f64 = 95.0;
const U_LID_DEPTH: f64 = 0.45;
const U_LID_REFLECTION: f64 = 0.9;
/// the open-top pair: above the hammer line (y), its height above the opening (m)
const U_HAMMER_Y: f64 = -0.15;
const U_TOP_HEIGHT: (f64, f64) = (0.08, 1.0);
/// pairs within the case width
const U_HALF_SPACING: f64 = 0.65;
/// the audience pair's height above the board center: 1.35 m above the floor, clearly
/// above the open top's plane (on it, the blocking rule would be a knife edge)
const U_AUDIENCE_Z: f64 = U_CASE_TOP + 0.05;

// ---- the harp (assumed geometry, a concert pedal harp: 1.88 m high, the soundbox 1.4 m
// long). x across (the player's left −), y away from the player (the audience +), z up;
// origin at the soundboard's center.
/// the board's lean from vertical, back towards the player (degrees), its length, and its
/// center above the floor (m): its foot 0.2 m up on the base
const H_TILT_DEG: f64 = 30.0;
const H_BOARD_LEN: f64 = 1.40;
const H_BOARD_HEIGHT: f64 = 0.81;
/// along the board (re its center, m): C1 near the foot, G7 near the top
const H_BRIDGE_S: (f64, f64) = (-0.60, 0.60);
const H_BRIDGE_NOTES: (f64, f64) = (24.0, 103.0);
/// the soundbox: its depth behind the board and its half width, at the foot and the top (m)
const H_DEPTH: (f64, f64) = (0.30, 0.10);
const H_HALF_WIDTH: (f64, f64) = (0.22, 0.06);
/// the sound holes along the back (s range along the board), level and lowpass
const H_HOLES_S: (f64, f64) = (-0.40, 0.30);
const H_HOLE_GAIN: f64 = 0.501_187_233_627_272_3;
const H_HOLE_LOWPASS_HZ: f64 = 600.0;
/// the column at the front (y) and the frame's top (z above the board center)
const H_COLUMN_Y: f64 = 0.62;
const H_TOP_Z: f64 = 1.07;
/// the player's ears: left of the soundbox's top, behind it (x, y; m), seated: 1.25 m above the floor
const H_EAR: (f64, f64) = (-0.20, -0.55);
const H_EAR_HEIGHT: f64 = 1.25;
/// close positions: the pair in front of the board (Close 1) or behind the sound holes (Close 2),
/// spread along the board, at most this far apart (m)
const H_CLOSE_SPACING_MAX: f64 = 1.2;
const H_CLEARANCE: f64 = 0.05;

// ---- the guitar (assumed geometry, a classical guitar, 650 mm scale, held by a seated
// player). x across (the player's left −: the neck), y away from the player (the audience +),
// z up; origin at the bridge on the top.
/// the bridge above the floor (m)
const G_HEIGHT: f64 = 0.62;
/// the body's outline on the top: (x, half width) from the bottom end to the neck joint (m)
const G_OUTLINE: [(f64, f64); 8] = [(0.165, 0.0), (0.15, 0.12), (0.08, 0.18), (-0.03, 0.18), (-0.12, 0.125), (-0.22, 0.14), (-0.29, 0.12), (-0.325, 0.06)];
/// the body's depth behind the top (m)
const G_DEPTH: f64 = 0.10;
/// the strings on the bridge: string 6 (low E) at +z … string 1 at −z, this far apart (m)
const G_STRING_SPACING: f64 = 0.0116;
/// the sound hole: its center (x), radius, level and lowpass
const G_HOLE_X: f64 = -0.165;
const G_HOLE_R: f64 = 0.043;
const G_HOLE_GAIN: f64 = 0.707_945_784_384_137_9;
const G_HOLE_LOWPASS_HZ: f64 = 500.0;
/// the neck from the joint to the nut, and the head (x), its half width (m)
const G_NECK_X: (f64, f64) = (-0.325, -0.65);
const G_HEAD_X: f64 = -0.83;
const G_NECK_HALF: f64 = 0.026;
/// the player's ears: above the upper bout, behind the top (m)
const G_EAR: [f64; 3] = [-0.15, -0.30, 0.50];
/// a seated listener's ears above the bridge (1.2 m above the floor)
const G_AUDIENCE_Z: f64 = 0.58;
/// close positions in front of the top: Close 1 at the sound hole and the 12th fret, Close 2
/// at the lower bout (x), the pair along the strings at most this far apart (m)
const G_CLOSE_X: (f64, f64) = (-0.25, 0.03);
const G_CLOSE_SPACING_MAX: f64 = 0.4;
const G_CLEARANCE: f64 = 0.03;

/// A guitar-like body's size: the classical guitar's outline and hole scaled (`kx` along, `kz`
/// across), its depth, its strings on the bridge (bass first: the first note each plays, first
/// position: the highest string whose first note is at or below the note) and its neck. Heights
/// (the bridge over the floor, the player's ears, the audience) are the guitar's.
#[derive(Clone, Copy, Debug)]
pub struct GuitarDims {
    pub kx: f64,
    pub kz: f64,
    pub depth: f64,
    pub strings: &'static [f64],
    /// the keys along the strings (the zones'): the bass string's first note to the top string's
    /// open one (the notes above it are on the top string)
    pub notes: (f64, f64),
    pub spacing: f64,
    pub neck_x: (f64, f64),
    pub head_x: f64,
    pub neck_half: f64,
}

impl GuitarDims {
    /// the classical guitar (`G_*`)
    pub const CLASSICAL: GuitarDims = GuitarDims {
        kx: 1.0,
        kz: 1.0,
        depth: G_DEPTH,
        strings: &[40.0, 45.0, 50.0, 55.0, 59.0, 64.0],
        notes: (40.0, 64.0),
        spacing: G_STRING_SPACING,
        neck_x: G_NECK_X,
        head_x: G_HEAD_X,
        neck_half: G_NECK_HALF,
    };
    /// the strumstick (a three-string fretted dulcimer on the lap, assumed): a body 0.37 m long
    /// and 0.15 m wide (the guitar's outline at 0.75 along, 0.42 across), 6 cm deep, its
    /// strings D3, A3, D4 (the bass string to G3, the middle one to D♯4, as proto/scale.py's
    /// strumstick_string), a 0.6 m scale: the nut 0.6 m from the bridge
    pub const STRUMSTICK: GuitarDims = GuitarDims {
        kx: 0.75,
        kz: 0.42,
        depth: 0.06,
        strings: &[0.0, 57.0, 64.0],
        notes: (50.0, 64.0),
        spacing: 0.012,
        neck_x: (-0.245, -0.60),
        head_x: -0.72,
        neck_half: 0.016,
    };

    /// A note's string, 0 the bass one.
    fn string(&self, note: f64) -> usize {
        self.strings.iter().rposition(|&o| o <= note).unwrap_or(0)
    }

    /// A note's point on the bridge (z): its string's, the bass string on top.
    pub fn bridge_z(&self, note: f64) -> f64 {
        ((self.strings.len() as f64 - 1.0) / 2.0 - self.string(note) as f64) * self.spacing
    }

    /// The body's outline on the top: (x, half width) from the bottom end to the neck joint.
    pub fn outline(&self) -> [(f64, f64); 8] {
        G_OUTLINE.map(|(x, w)| (x * self.kx, w * self.kz))
    }

    /// The body's half width (z) at x on the top, 0 beyond it.
    pub fn half_width(&self, x: f64) -> f64 {
        let o = self.outline();
        if x > o[0].0 || x < o[o.len() - 1].0 {
            return 0.0;
        }
        let i = o.iter().position(|&(ox, _)| ox <= x).unwrap_or(o.len() - 1).max(1);
        let ((x0, w0), (x1, w1)) = (o[i - 1], o[i]);
        if x0 == x1 { w1 } else { w0 + (x - x0) / (x1 - x0) * (w1 - w0) }
    }

    /// The sound hole's center (x) and radius.
    pub fn hole(&self) -> (f64, f64) {
        (G_HOLE_X * self.kx, G_HOLE_R * self.kz)
    }

    /// Close 1 and close 2 along the top (x), and the pair's widest spacing.
    pub fn close(&self) -> ((f64, f64), f64) {
        ((G_CLOSE_X.0 * self.kx, G_CLOSE_X.1 * self.kx), G_CLOSE_SPACING_MAX * self.kx)
    }
}

// ---- zithers (assumed geometries): a flat board on a table, its strings across the top, the
// player seated behind it (−y). x across (the player's left −), y away from the player, z up;
// origin at the board's center on its top.
#[derive(Clone, Copy, Debug)]
pub struct ZitherDims {
    /// the top's outline from above (x, y), at most 24 points
    pub outline: &'static [[f64; 2]],
    /// the strings' drive points on the board (the bridges): the lowest and the highest note's
    /// (x, y), the others between them by note
    pub bass: [f64; 2],
    pub treble: [f64; 2],
    pub notes: (f64, f64),
    /// a rose in the top (its center; none: the sound leaves through the board alone)
    pub rose: Option<[f64; 2]>,
    /// the frame the bars' placements and clamping take (the board's extent, its depth as the
    /// "tubes", the table's height, the player's ears)
    pub frame: BarFrame,
}

impl ZitherDims {
    /// VCSL's plucked psaltery (A♯4–F♯6, 11 strings): a triangle 0.5 m wide at its base near the
    /// player and 0.4 m deep, 5 cm thick, flat on a table at 0.75 m; the strings' bridges along
    /// the base, the bass to the left; a rose in the middle
    pub const PSALTERY: ZitherDims = ZitherDims {
        outline: &[[-0.25, -0.15], [0.25, -0.15], [0.0, 0.25]],
        bass: [-0.16, -0.12],
        treble: [0.16, -0.12],
        notes: (70.0, 90.0),
        rose: Some([0.0, 0.0]),
        frame: BarFrame {
            height: 0.75,
            bar_x: (-0.16, 0.16),
            notes: (70.0, 90.0),
            natural_y: -0.12,
            accidental: (-0.12, 0.0),
            half_depth: 0.2,
            half_width: 0.25,
            tubes: (0.05, 0.05),
            ear: (-0.40, 1.25),
            close_spacing_max: 0.4,
            clearance: 0.03,
        },
    };
    /// VCSL's đàn tranh (B2–B5, 16 strings): a long box 1.1 m on a stand at 0.6 m, 0.26 m wide at
    /// the head (the player's right) and 0.16 m at the tail, 8 cm deep; the strings run along it
    /// from the fixed bridge near the head (x 0.45) over the movable bridges, which lie on a
    /// diagonal (the speaking lengths, B2 0.85 m to B5 0.25 m, as proto/scale.py's), the bass
    /// farthest from the player; the sound holes underneath
    pub const DANTRANH: ZitherDims = ZitherDims {
        outline: &[[-0.55, -0.08], [0.55, -0.13], [0.55, 0.13], [-0.55, 0.08]],
        bass: [-0.40, 0.08],
        treble: [0.20, -0.09],
        notes: (47.0, 83.0),
        rose: None,
        frame: BarFrame {
            height: 0.60,
            bar_x: (-0.40, 0.20),
            notes: (47.0, 83.0),
            natural_y: 0.0,
            accidental: (0.0, 0.0),
            half_depth: 0.13,
            half_width: 0.55,
            tubes: (0.08, 0.08),
            ear: (-0.45, 1.15),
            close_spacing_max: 0.8,
            clearance: 0.03,
        },
    };

    /// A note's drive point (x, y) on the top.
    pub fn bridge_point(&self, note: f64) -> (f64, f64) {
        let t = ((note - self.notes.0) / (self.notes.1 - self.notes.0)).clamp(0.0, 1.0);
        (self.bass[0] + t * (self.treble[0] - self.bass[0]), self.bass[1] + t * (self.treble[1] - self.bass[1]))
    }
}

// ---- bars on a frame: the vibraphone and the marimba (assumed geometries). Origin at the
// middle of the bars' plane.
#[derive(Clone, Copy, Debug)]
pub struct BarFrame {
    /// the bars above the floor (m)
    pub height: f64,
    /// the bars' extent along x (the lowest and highest natural's center) and their keys
    pub bar_x: (f64, f64),
    pub notes: (f64, f64),
    /// the rows: naturals nearer the player (y), accidentals behind them (y) and higher (z)
    pub natural_y: f64,
    pub accidental: (f64, f64),
    /// the frame: half depth (y) and half width (x); the tubes' lengths below the bars at
    /// the bass and the treble end (m)
    pub half_depth: f64,
    pub half_width: f64,
    pub tubes: (f64, f64),
    /// the player's ears: behind the bars' center (y) and above the floor (m)
    pub ear: (f64, f64),
    /// close pairs at most this far apart; free microphones this far from the frame (m)
    pub close_spacing_max: f64,
    pub clearance: f64,
}

/// A 3-octave F3–F6 vibraphone: 37 bars on a 1.4 m frame, the bars 0.85 m above the floor.
pub const VIBRAPHONE: BarFrame = BarFrame {
    height: 0.85,
    bar_x: (-0.66, 0.66),
    notes: (53.0, 89.0),
    natural_y: -0.09,
    accidental: (0.10, 0.02),
    half_depth: 0.24,
    half_width: 0.74,
    tubes: (0.62, 0.16),
    ear: (-0.50, 1.60),
    close_spacing_max: 1.2,
    clearance: 0.05,
};

/// A 5-octave C2–C7 marimba: 61 bars on a 2.5 m frame at 0.9 m, the bass bars ~0.5 m
/// long (a deep keyboard), the bass resonators folded to ~0.85 m.
pub const MARIMBA: BarFrame = BarFrame {
    height: 0.90,
    bar_x: (-1.14, 1.14),
    notes: (36.0, 96.0),
    natural_y: -0.15,
    accidental: (0.17, 0.03),
    half_depth: 0.42,
    half_width: 1.25,
    tubes: (0.85, 0.06),
    ear: (-0.65, 1.60),
    close_spacing_max: 1.6,
    clearance: 0.05,
};

/// A 3½-octave F4–C8 xylophone: 44 narrow bars on a 1.65 m frame at 0.85 m, short resonators
/// (the bass's 0.3 m).
pub const XYLOPHONE: BarFrame = BarFrame {
    height: 0.85,
    bar_x: (-0.74, 0.74),
    notes: (65.0, 108.0),
    natural_y: -0.08,
    accidental: (0.10, 0.03),
    half_depth: 0.26,
    half_width: 0.83,
    tubes: (0.30, 0.04),
    ear: (-0.55, 1.60),
    close_spacing_max: 1.2,
    clearance: 0.05,
};

/// Orchestra bells (a glockenspiel) C5–E8: 41 small steel bars in a 0.9 m case on a stand at
/// 0.9 m, no resonators (the case 8 cm deep under the bars).
pub const BELLS: BarFrame = BarFrame {
    height: 0.90,
    bar_x: (-0.36, 0.36),
    notes: (72.0, 112.0),
    natural_y: -0.05,
    accidental: (0.06, 0.02),
    half_depth: 0.16,
    half_width: 0.45,
    tubes: (0.08, 0.08),
    ear: (-0.45, 1.60),
    close_spacing_max: 0.8,
    clearance: 0.05,
};

/// Crotales C6–C8: 25 bronze discs on a 0.84 m bar in two rows (the naturals in front) on a
/// stand at 0.9 m; nothing under them but the bar (6 cm).
pub const CROTALES: BarFrame = BarFrame {
    height: 0.90,
    bar_x: (-0.42, 0.42),
    notes: (84.0, 108.0),
    natural_y: -0.06,
    accidental: (0.07, 0.02),
    half_depth: 0.16,
    half_width: 0.50,
    tubes: (0.06, 0.06),
    ear: (-0.45, 1.60),
    close_spacing_max: 0.8,
    clearance: 0.05,
};

/// Tubular bells (chimes) C4–F5: 18 brass tubes of 38 mm hanging in two rows (the naturals in
/// front) from a rack 1.3 m wide, the caps at ~1.9 m; the plane at the tubes' middles (1.3 m,
/// each a source radiating all round), their lower halves below it (0.75 m in the bass, 0.5 m in
/// the treble). Assumed.
pub const CHIMES: BarFrame = BarFrame {
    height: 1.30,
    bar_x: (-0.50, 0.50),
    notes: (60.0, 77.0),
    natural_y: -0.05,
    accidental: (0.06, 0.0),
    half_depth: 0.15,
    half_width: 0.65,
    tubes: (0.75, 0.50),
    ear: (-0.60, 1.65),
    close_spacing_max: 1.0,
    clearance: 0.05,
};

/// Hand chimes C4–C7: 19 tubes a whole tone apart, rung at shoulder height by a row of ringers
/// behind a padded table (2.4 m); the plane at the tubes' tines (1.2 m), the tubes below it (0.3 m
/// in the bass, 0.1 m in the treble). Assumed.
pub const HANDCHIMES: BarFrame = BarFrame {
    height: 1.20,
    bar_x: (-1.10, 1.10),
    notes: (60.0, 96.0),
    natural_y: -0.05,
    accidental: (0.06, 0.0),
    half_depth: 0.30,
    half_width: 1.25,
    tubes: (0.30, 0.10),
    ear: (-0.40, 1.60),
    close_spacing_max: 1.2,
    clearance: 0.05,
};

/// A kalimba G2–C♯7: 27 steel tines over a bridge on a board 0.18 m wide, held in both hands by a
/// seated player at 1.0 m (the tines spread over 0.15 m; on the instrument the bass in the middle,
/// here bass left, as the other bars; one row, the accidentals' tines a little further out), the
/// board 2 cm under them. Assumed.
pub const KALIMBA: BarFrame = BarFrame {
    height: 1.00,
    bar_x: (-0.075, 0.075),
    notes: (43.0, 97.0),
    natural_y: -0.005,
    accidental: (0.005, 0.0),
    half_depth: 0.09,
    half_width: 0.10,
    tubes: (0.02, 0.02),
    ear: (-0.30, 1.25),
    close_spacing_max: 0.3,
    clearance: 0.03,
};

/// A balafon C4–F6 (recorded from C♯4): wooden bars in one row on a low frame 1.1 m wide at 0.6 m
/// (played seated or kneeling), a gourd under each bar (0.35 m in the bass, 0.12 m in the treble).
/// Assumed.
pub const BALAFON: BarFrame = BarFrame {
    height: 0.60,
    bar_x: (-0.50, 0.50),
    notes: (60.0, 89.0),
    natural_y: -0.02,
    accidental: (0.02, 0.0),
    half_depth: 0.20,
    half_width: 0.58,
    tubes: (0.35, 0.12),
    ear: (-0.45, 1.10),
    close_spacing_max: 1.0,
    clearance: 0.05,
};

/// A natural's index from C(−1) on, fractional for the accidentals (between their
/// naturals); and whether the key is an accidental.
fn bar_key(note: f64) -> (f64, bool) {
    let n = note.round() as i64;
    let pc = n.rem_euclid(12);
    let oct = n.div_euclid(12);
    // naturals per pitch class: C D E F G A B → 0…6; accidentals between
    let (w, acc) = match pc {
        0 => (0.0, false), 1 => (0.5, true), 2 => (1.0, false), 3 => (1.5, true), 4 => (2.0, false), 5 => (3.0, false),
        6 => (3.5, true), 7 => (4.0, false), 8 => (4.5, true), 9 => (5.0, false), 10 => (5.5, true), _ => (6.0, false),
    };
    (7.0 * oct as f64 + w, acc)
}

impl BarFrame {
    /// Where a note's bar lies (x, y) on the frame: keys outside its range just past its ends.
    fn bar_point(&self, note: f64) -> (f64, f64) {
        let (lo, hi) = (bar_key(self.notes.0).0, bar_key(self.notes.1).0);
        let n = note.clamp(self.notes.0 - 3.0, self.notes.1 + 3.0);
        let (w, acc) = bar_key(n);
        let x = self.bar_x.0 + (w - lo) / (hi - lo) * (self.bar_x.1 - self.bar_x.0);
        (x, if acc { self.accidental.0 } else { self.natural_y })
    }

    /// The resonators' depth below the bars at x (m): long tubes at the bass end.
    fn tube(&self, x: f64) -> f64 {
        let t = ((x - self.bar_x.0) / (self.bar_x.1 - self.bar_x.0)).clamp(0.0, 1.0);
        self.tubes.0 + t * (self.tubes.1 - self.tubes.0)
    }
}

/// The harp's board directions: up along the board (towards the top, leaning back) and its
/// outward normal (towards the strings and the audience, up).
fn h_axes() -> ([f64; 3], [f64; 3]) {
    let t = H_TILT_DEG.to_radians();
    ([0.0, -t.sin(), t.cos()], [0.0, t.cos(), t.sin()])
}

/// A harp's size: the concert harp's geometry (`H_*`) scaled by `k` (the lever harp: smaller
/// throughout, the board's lean kept), its bridge running from key `notes.0` at the board's
/// foot to `notes.1` at its top. The player's ears stay at a seated 1.25 m.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HarpDims {
    pub k: f64,
    pub notes: (f64, f64),
}

impl HarpDims {
    pub const CONCERT: HarpDims = HarpDims { k: 1.0, notes: H_BRIDGE_NOTES };
    /// the lever harp (Harp B: a 34-string Celtic harp, C2–A6), assumed 3/4 of the concert harp
    pub const LEVER: HarpDims = HarpDims { k: 0.75, notes: (36.0, 93.0) };

    fn board_len(self) -> f64 {
        self.k * H_BOARD_LEN
    }
    fn board_height(self) -> f64 {
        self.k * H_BOARD_HEIGHT
    }
    fn bridge_range(self) -> (f64, f64) {
        (self.k * H_BRIDGE_S.0, self.k * H_BRIDGE_S.1)
    }
    fn holes_s(self) -> (f64, f64) {
        (self.k * H_HOLES_S.0, self.k * H_HOLES_S.1)
    }
    fn half_width(self) -> (f64, f64) {
        (self.k * H_HALF_WIDTH.0, self.k * H_HALF_WIDTH.1)
    }
    fn column_y(self) -> f64 {
        self.k * H_COLUMN_Y
    }
    fn top_z(self) -> f64 {
        self.k * H_TOP_Z
    }
    fn ear(self) -> (f64, f64) {
        (self.k * H_EAR.0, self.k * H_EAR.1)
    }
    /// the ears' height above the board's center (a seated player: 1.25 m above the floor)
    fn ear_z(self) -> f64 {
        H_EAR_HEIGHT - self.board_height()
    }
    fn close_spacing_max(self) -> f64 {
        self.k * H_CLOSE_SPACING_MAX
    }

    /// position along the board (m re its center) → (soundbox depth, half width) there
    fn box_at(self, s: f64) -> (f64, f64) {
        let t = ((s / self.board_len()) + 0.5).clamp(0.0, 1.0);
        let (w0, w1) = self.half_width();
        (self.k * (H_DEPTH.0 + t * (H_DEPTH.1 - H_DEPTH.0)), w0 + t * (w1 - w0))
    }

    /// Where a note drives the board: along its center line (s, m re the center; 0 across).
    fn bridge_s(self, note: f64) -> f64 {
        let t = ((note - self.notes.0) / (self.notes.1 - self.notes.0)).clamp(-0.05, 1.05);
        let (a, b) = self.bridge_range();
        a + t * (b - a)
    }

    /// A point on the board (s along it, re its center), and the same point on the soundbox's back.
    fn on_board(self, s: f64) -> [f64; 3] {
        let (u, _) = h_axes();
        [0.0, s * u[1], s * u[2]]
    }

    fn on_back(self, s: f64) -> [f64; 3] {
        let (_, n) = h_axes();
        let p = self.on_board(s);
        let d = self.box_at(s).0;
        [p[0] - d * n[0], p[1] - d * n[1], p[2] - d * n[2]]
    }
}

/// the plugin's ranges: distance (m) and spacing (m)
pub const MAX_DISTANCE: f64 = 20.0;
/// Custom microphones: clearance above the floor, the highest point (above the board's
/// center), the clearance of the upright's wall and above its open top, the wall's range (m)
const FLOOR_CLEARANCE: f64 = 0.05;
const MAX_MIC_HEIGHT: f64 = 3.0;
const WALL_CLEARANCE: f64 = 0.05;
const U_TOP_CLEARANCE: f64 = 0.05;
pub const MAX_WALL: f64 = 2.0;
pub const MAX_SPACING: f64 = 10.0;

/// Where a position puts the pair, and how its distance and spacing read and move (the
/// UI's drawing and dragging): distance = (pair center − origin)·axis, spacing along
/// `spread` (left → right); `range`/`spacing_max` the limits the engine applies;
/// `height`: the distance is a height (shown and dragged from the front).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub mics: [[f64; 3]; 2],
    pub axis: [f64; 3],
    pub origin: [f64; 3],
    pub distance: f64,
    pub range: (f64, f64),
    pub spread: [f64; 3],
    pub spacing: f64,
    pub spacing_max: f64,
    pub height: bool,
}

/// One microphone: where it is and how it listens. `pattern`: 0 omni … 0.5 cardioid …
/// 0.75 hypercardioid … 1 figure-8; the aim is `yaw` (degrees in the x–y plane, 0 along +y,
/// positive towards +x) and `pitch` (degrees above the horizon).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mic {
    pub pos: [f64; 3],
    pub pattern: f64,
    pub yaw: f64,
    pub pitch: f64,
}

impl Mic {
    /// An omni microphone at `pos`, aimed at the board's center.
    pub fn toward_board(pos: [f64; 3]) -> Mic {
        let v = [-pos[0], -pos[1], -pos[2]];
        let len = dot(v, v).sqrt();
        if len < 1e-9 {
            return Mic { pos, pattern: 0.0, yaw: 0.0, pitch: 0.0 };
        }
        Mic { pos, pattern: 0.0, yaw: v[0].atan2(v[1]).to_degrees(), pitch: (v[2] / len).clamp(-1.0, 1.0).asin().to_degrees() }
    }
}

/// The pair (left, right) and, for the upright, the wall behind it (its y, m; the grand
/// ignores it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MicSetup {
    pub mics: [Mic; 2],
    pub wall: f64,
    /// how far the lid is open: 0 closed … 1 as modeled, `LID_OFF` taken off
    pub lid: f64,
}

impl Default for MicSetup {
    /// a pair in front of the keyboard, 0.5 m apart at ear height (what Custom starts as)
    fn default() -> Self {
        MicSetup { mics: [Mic::toward_board([-0.25, -1.25, 0.45]), Mic::toward_board([0.25, -1.25, 0.45])], wall: U_WALL, lid: 1.0 }
    }
}

const X_AXIS: [f64; 3] = [1.0, 0.0, 0.0];
const Y_AXIS: [f64; 3] = [0.0, 1.0, 0.0];
const Z_AXIS: [f64; 3] = [0.0, 0.0, 1.0];
const FRONT: [f64; 3] = [0.0, -1.0, 0.0];

/// The wall's y for a position.
fn u_wall(pos: MicPosition) -> f64 {
    if pos == MicPosition::Tail { U_WALL_BACK } else { U_WALL }
}

/// Where a note drives the upright's board (x, z): the bass bridge low on the left
/// (A0 … C2), the treble bridge rising from low left of center (C#2) to the upper
/// right (C8).
fn u_bridge_point(note: f64) -> (f64, f64) {
    let lerp = |a: (f64, f64), b: (f64, f64), t: f64| (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
    if note < 36.5 {
        lerp((-0.55, -0.30), (-0.30, -0.05), ((note - 21.0) / 15.5).clamp(0.0, 1.0))
    } else {
        lerp((-0.15, -0.40), (0.55, 0.40), ((note - 37.0) / 71.0).clamp(0.0, 1.0))
    }
}

/// A radiating surface of one bridge zone: position, outward unit normal, amplitude
/// gain, a first-order lowpass (Hz) or none, and whether it radiates to its front
/// side only (a one-sided source reaches the other side by diffraction around the
/// case, `DIFFRACTION_*`).
#[derive(Clone, Copy, Debug)]
pub struct Source {
    pub pos: [f64; 3],
    pub normal: [f64; 3],
    pub gain: f64,
    pub lowpass_hz: Option<f64>,
    pub one_sided: bool,
    /// the sound this source radiates leaves through the lid
    pub through_lid: bool,
}

/// Where a reflector ends: nowhere (floor, wall) or a rectangle spanned from its origin
/// by the unit vectors u and w over the given ranges (m).
#[derive(Clone, Copy, Debug)]
pub enum Extent {
    Unbounded,
    Rect { u: [f64; 3], u_range: (f64, f64), w: [f64; 3], w_range: (f64, f64) },
}

/// A flat reflector: the plane through `origin` with unit normal `normal`, its
/// reflection factor and extent; `shielded_by_case`: a microphone above the case does
/// not get it (the case is in the way: the floor).
#[derive(Clone, Copy, Debug)]
pub struct Reflector {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub gain: f64,
    pub extent: Extent,
    pub shielded_by_case: bool,
}

/// Reflector indices in `Body::reflectors` (a path's `via`; read by the tests): the
/// grand's floor and lid, the upright's floor, wall and top lid. The lid comes last (taken
/// off, the list ends before it).
#[cfg(test)]
const GRAND_FLOOR: usize = 0;
const GRAND_LID: usize = 1;
#[cfg(test)]
const U_FLOOR: usize = 0;
#[cfg(test)]
const U_WALL_REFL: usize = 1;
const U_LID: usize = 2;

/// The bodies with a `BarFrame` (`Body::is_bars`), as one pattern: a new one goes here and in
/// `Body::frame`.
macro_rules! bar_bodies {
    () => {
        Body::Vibraphone | Body::Marimba | Body::Xylophone | Body::Bells | Body::Crotales | Body::Chimes | Body::HandChimes | Body::Kalimba | Body::Balafon
    };
}

/// The geometry of an instrument type: bridge points, radiating sources, reflectors,
/// case and microphone placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Body {
    /// a grand on a floor, lid on full stick (the positions above)
    #[default]
    Grand,
    /// an upright against a wall, top lid open (`U_*`)
    Upright,
    /// a concert harp: the soundbox leaning back towards the player, no lid (`H_*`)
    Harp,
    /// a lever harp: the concert harp's geometry at 3/4 (`HarpDims::LEVER`)
    LeverHarp,
    /// a vibraphone: bars on a frame, resonators below, no lid (`VIBRAPHONE`)
    Vibraphone,
    /// a marimba: the same on a larger frame (`MARIMBA`)
    Marimba,
    /// a xylophone: the same on a smaller one (`XYLOPHONE`)
    Xylophone,
    /// orchestra bells: small bars in a case, no resonators (`BELLS`)
    Bells,
    /// crotales: discs in two rows on a bar (`CROTALES`)
    Crotales,
    /// tubular bells: tubes hanging in two rows from a rack (`CHIMES`)
    Chimes,
    /// hand chimes: tubes rung in a row behind a table (`HANDCHIMES`)
    HandChimes,
    /// a kalimba: tines on a small board held by the player (`KALIMBA`)
    Kalimba,
    /// a balafon: wooden bars over gourds on a low frame (`BALAFON`)
    Balafon,
    /// a classical guitar: its top upright, facing the audience (`G_*`)
    Guitar,
    /// a psaltery: a triangular box flat on a table (`ZitherDims::PSALTERY`)
    Psaltery,
    /// a đàn tranh: a long box on a stand (`ZitherDims::DANTRANH`)
    DanTranh,
    /// a strumstick: held on the lap as the guitar, a smaller body (`GuitarDims::STRUMSTICK`)
    Strumstick,
}

/// The bodies by the C interface's number (`pianoengine.h` pe_mic_geometry): 0 grand, 1 upright,
/// 2 harp, 3 lever harp, 4 vibraphone, 5 marimba, 6 xylophone, 7 bells, 8 crotales, 9 guitar,
/// 10 chimes, 11 hand chimes, 12 kalimba, 13 balafon, 14 psaltery, 15 đàn tranh, 16 strumstick.
pub const BODIES: [Body; 17] = [
    Body::Grand,
    Body::Upright,
    Body::Harp,
    Body::LeverHarp,
    Body::Vibraphone,
    Body::Marimba,
    Body::Xylophone,
    Body::Bells,
    Body::Crotales,
    Body::Guitar,
    Body::Chimes,
    Body::HandChimes,
    Body::Kalimba,
    Body::Balafon,
    Body::Psaltery,
    Body::DanTranh,
    Body::Strumstick,
];

impl Body {
    /// the C interface's number for this body (its place in `BODIES`)
    pub fn index(self) -> u32 {
        BODIES.iter().position(|&b| b == self).unwrap_or(0) as u32
    }
}

impl Body {
    /// The body an instrument.json names (`"body"`; none or another name: a grand).
    pub fn from_name(name: Option<&str>) -> Body {
        match name {
            Some("upright") => Body::Upright,
            Some("harp") => Body::Harp,
            Some("leverharp") => Body::LeverHarp,
            Some("vibraphone") => Body::Vibraphone,
            Some("marimba") => Body::Marimba,
            Some("xylophone") => Body::Xylophone,
            Some("bells") => Body::Bells,
            Some("crotales") => Body::Crotales,
            Some("chimes") => Body::Chimes,
            Some("handchimes") => Body::HandChimes,
            Some("kalimba") => Body::Kalimba,
            Some("balafon") => Body::Balafon,
            Some("guitar") => Body::Guitar,
            Some("psaltery") => Body::Psaltery,
            Some("dantranh") => Body::DanTranh,
            Some("strumstick") => Body::Strumstick,
            _ => Body::Grand,
        }
    }

    /// The harp's size (the concert harp's for the other bodies: unused there).
    pub fn harp_dims(self) -> HarpDims {
        if self == Body::LeverHarp { HarpDims::LEVER } else { HarpDims::CONCERT }
    }

    /// The guitar-like body's size (the classical guitar's for the other bodies: unused there).
    pub fn guitar_dims(self) -> GuitarDims {
        if self == Body::Strumstick { GuitarDims::STRUMSTICK } else { GuitarDims::CLASSICAL }
    }

    /// The zither's size (the psaltery's for the other bodies: unused there).
    pub fn zither_dims(self) -> ZitherDims {
        if self == Body::DanTranh { ZitherDims::DANTRANH } else { ZitherDims::PSALTERY }
    }

    /// A zither (a flat board on a table, the strings across its top).
    pub fn is_zither(self) -> bool {
        matches!(self, Body::Psaltery | Body::DanTranh)
    }

    /// A harp (no lid, no case around the board).
    pub fn is_harp(self) -> bool {
        matches!(self, Body::Harp | Body::LeverHarp)
    }

    /// The bars' frame (the vibraphone's for the other bodies: unused there).
    pub fn frame(self) -> BarFrame {
        match self {
            Body::Marimba => MARIMBA,
            Body::Xylophone => XYLOPHONE,
            Body::Bells => BELLS,
            Body::Crotales => CROTALES,
            Body::Chimes => CHIMES,
            Body::HandChimes => HANDCHIMES,
            Body::Kalimba => KALIMBA,
            Body::Balafon => BALAFON,
            _ => VIBRAPHONE,
        }
    }

    /// Bars on a frame (vibraphone, marimba).
    pub fn is_bars(self) -> bool {
        matches!(self, bar_bodies!())
    }

    /// The body has a lid (grand, upright).
    pub fn has_lid(self) -> bool {
        matches!(self, Body::Grand | Body::Upright)
    }

    /// A note's drive point in the board's own coordinates (the mode shapes'): (x, y)
    /// for the grand's horizontal board.
    pub fn bridge_point(self, note: f64) -> (f64, f64) {
        let hp = self.harp_dims();
        match self {
            Body::Grand => bridge_point(note),
            Body::Upright => u_bridge_point(note),
            Body::Harp | Body::LeverHarp => (hp.bridge_s(note), 0.0),
            bar_bodies!() => self.frame().bar_point(note),
            Body::Guitar | Body::Strumstick => (0.0, self.guitar_dims().bridge_z(note)),
            Body::Psaltery | Body::DanTranh => self.zither_dims().bridge_point(note),
        }
    }

    /// The sources a note's bridge zone radiates from (3-D positions).
    pub fn sources(self, note: f64) -> Vec<Source> {
        let hp = self.harp_dims();
        match self {
            Body::Grand => {
                let (x, y) = bridge_point(note);
                // both faces radiate: the top directly, the underside via the floor
                vec![Source { pos: [x, y, 0.0], normal: [0.0, 0.0, 1.0], gain: 1.0, lowpass_hz: None, one_sided: false, through_lid: true }]
            }
            Body::Upright => {
                let (x, z) = u_bridge_point(note);
                vec![
                    // the back face, at the bridge point (behind its ribs and posts)
                    Source { pos: [x, 0.0, z], normal: [0.0, 1.0, 0.0], gain: 1.0, lowpass_hz: Some(U_BACK_LOWPASS_HZ), one_sided: true, through_lid: false },
                    // the front face's sound through the open top above the zone
                    Source { pos: [x, U_FRONT_Y, U_CASE_TOP], normal: [0.0, 0.0, 1.0], gain: U_FRONT_GAIN, lowpass_hz: None, one_sided: true, through_lid: true },
                    // … and through the upper front panel
                    Source {
                        pos: [x, U_CASE_Y.0, U_PANEL_Z],
                        normal: [0.0, -1.0, 0.0],
                        gain: U_FRONT_GAIN,
                        lowpass_hz: Some(U_FRONT_LOWPASS_HZ),
                        one_sided: true,
                        through_lid: false,
                    },
                ]
            }
            Body::Harp | Body::LeverHarp => {
                let (_, n) = h_axes();
                let s = hp.bridge_s(note);
                vec![
                    // the board's outer face at the bridge point
                    Source { pos: hp.on_board(s), normal: n, gain: 1.0, lowpass_hz: None, one_sided: true, through_lid: false },
                    // its inner face's sound out of the sound holes behind it, towards the player
                    Source {
                        pos: hp.on_back(s.clamp(hp.holes_s().0, hp.holes_s().1)),
                        normal: [-n[0], -n[1], -n[2]],
                        gain: H_HOLE_GAIN,
                        lowpass_hz: Some(H_HOLE_LOWPASS_HZ),
                        one_sided: true,
                        through_lid: false,
                    },
                ]
            }
            bar_bodies!() => {
                let fr = self.frame();
                let (x, y) = fr.bar_point(note);
                let z = if bar_key(note).1 { fr.accidental.1 } else { 0.0 };
                vec![Source { pos: [x, y, z], normal: [0.0, 0.0, 1.0], gain: 1.0, lowpass_hz: None, one_sided: false, through_lid: false }]
            }
            Body::Guitar | Body::Strumstick => {
                let gd = self.guitar_dims();
                vec![
                    // the top at the string's point on the bridge
                    Source { pos: [0.0, 0.0, gd.bridge_z(note)], normal: Y_AXIS, gain: 1.0, lowpass_hz: None, one_sided: true, through_lid: false },
                    // the body's air out of the sound hole
                    Source { pos: [gd.hole().0, 0.0, 0.0], normal: Y_AXIS, gain: G_HOLE_GAIN, lowpass_hz: Some(G_HOLE_LOWPASS_HZ), one_sided: true, through_lid: false },
                ]
            }
            Body::Psaltery | Body::DanTranh => {
                let zd = self.zither_dims();
                let (x, y) = zd.bridge_point(note);
                // the top at the string's bridge (its underside on the table), the body's air out
                // of the rose (the guitar's hole's level and lowpass)
                let mut out = vec![Source { pos: [x, y, 0.0], normal: [0.0, 0.0, 1.0], gain: 1.0, lowpass_hz: None, one_sided: true, through_lid: false }];
                if let Some([rx, ry]) = zd.rose {
                    out.push(Source { pos: [rx, ry, 0.0], normal: [0.0, 0.0, 1.0], gain: G_HOLE_GAIN, lowpass_hz: Some(G_HOLE_LOWPASS_HZ), one_sided: true, through_lid: false });
                }
                out
            }
        }
    }

    /// The reflectors around the instrument for a wall position, in path order (indices
    /// `GRAND_*`, `U_FLOOR`, `U_WALL_REFL`, `U_LID`).
    pub fn reflectors(self, wall: f64, lid: f64) -> Vec<Reflector> {
        let hp = self.harp_dims();
        match self {
            Body::Grand => {
                let a = lid_angle(LID_ANGLE_DEG, lid);
                let mut r = vec![
                    Reflector {
                        origin: [0.0, 0.0, -BOARD_HEIGHT],
                        normal: [0.0, 0.0, 1.0],
                        gain: FLOOR_REFLECTION,
                        extent: Extent::Unbounded,
                        shielded_by_case: true,
                    },
                    Reflector {
                        origin: [LID_HINGE.0, 0.0, LID_HINGE.1],
                        normal: [-a.sin(), 0.0, a.cos()],
                        gain: LID_REFLECTION,
                        extent: Extent::Rect { u: [a.cos(), 0.0, a.sin()], u_range: (0.0, LID_WIDTH), w: [0.0, 1.0, 0.0], w_range: LID_Y },
                        shielded_by_case: false,
                    },
                ];
                r.truncate(if lid_off(lid) { GRAND_LID } else { r.len() });
                r
            }
            Body::Upright => {
                let a = lid_angle(U_LID_ANGLE_DEG, lid);
                let mut r = vec![
                    Reflector {
                        origin: [0.0, 0.0, -U_BOARD_HEIGHT],
                        normal: [0.0, 0.0, 1.0],
                        gain: FLOOR_REFLECTION,
                        extent: Extent::Unbounded,
                        shielded_by_case: true,
                    },
                    Reflector {
                        origin: [0.0, wall, 0.0],
                        normal: [0.0, 1.0, 0.0],
                        gain: U_WALL_REFLECTION,
                        extent: Extent::Unbounded,
                        shielded_by_case: false,
                    },
                    Reflector {
                        origin: [0.0, U_CASE_Y.1, U_CASE_TOP],
                        normal: [0.0, a.sin(), a.cos()],
                        gain: U_LID_REFLECTION,
                        extent: Extent::Rect {
                            u: [0.0, -a.cos(), a.sin()],
                            u_range: (0.0, U_LID_DEPTH),
                            w: [1.0, 0.0, 0.0],
                            w_range: U_CASE_X,
                        },
                        shielded_by_case: false,
                    },
                ];
                r.truncate(if lid_off(lid) { U_LID } else { r.len() });
                r
            }
            Body::Harp | Body::LeverHarp => vec![Reflector {
                origin: [0.0, 0.0, -hp.board_height()],
                normal: [0.0, 0.0, 1.0],
                gain: FLOOR_REFLECTION,
                extent: Extent::Unbounded,
                // a narrow instrument: nothing over the floor's reflection point to speak of
                shielded_by_case: false,
            }],
            bar_bodies!() => vec![Reflector {
                origin: [0.0, 0.0, -self.frame().height],
                normal: [0.0, 0.0, 1.0],
                gain: FLOOR_REFLECTION,
                extent: Extent::Unbounded,
                // an open frame: the sound reaches the floor between the tubes
                shielded_by_case: false,
            }],
            Body::Guitar | Body::Strumstick => vec![Reflector {
                origin: [0.0, 0.0, -G_HEIGHT],
                normal: [0.0, 0.0, 1.0],
                gain: FLOOR_REFLECTION,
                extent: Extent::Unbounded,
                // a small body: nothing over the floor's reflection point to speak of
                shielded_by_case: false,
            }],
            Body::Psaltery | Body::DanTranh => vec![Reflector {
                origin: [0.0, 0.0, -self.zither_dims().frame.height],
                normal: [0.0, 0.0, 1.0],
                gain: FLOOR_REFLECTION,
                extent: Extent::Unbounded,
                // a box on a table: its top radiates up, the floor's reflection under the table
                shielded_by_case: false,
            }],
        }
    }

    /// Whether the closed lid is between the board's top-face sound and a microphone: the grand's
    /// microphones outside the case (not those under the lid); the upright's, always.
    pub fn lid_shields(self, mic: [f64; 3]) -> bool {
        match self {
            Body::Grand => !self.over_case(mic),
            Body::Upright => true,
            Body::Harp | Body::LeverHarp | bar_bodies!() | Body::Guitar | Body::Psaltery | Body::DanTranh | Body::Strumstick => false, // no lid
        }
    }

    /// A point above the case's footprint.
    pub fn over_case(self, p: [f64; 3]) -> bool {
        match self {
            Body::Grand => (CASE_X.0..=CASE_X.1).contains(&p[0]) && (CASE_Y.0..=CASE_Y.1).contains(&p[1]),
            Body::Upright => (U_CASE_X.0..=U_CASE_X.1).contains(&p[0]) && (U_CASE_Y.0..=U_CASE_Y.1).contains(&p[1]),
            Body::Harp | Body::LeverHarp | bar_bodies!() | Body::Guitar | Body::Psaltery | Body::DanTranh | Body::Strumstick => false,
        }
    }

    /// The wall's y a preset puts behind the upright (the grand ignores it).
    pub fn wall_of(self, pos: MicPosition) -> f64 {
        match self {
            Body::Grand | Body::Harp | Body::LeverHarp | bar_bodies!() | Body::Guitar | Body::Psaltery | Body::DanTranh | Body::Strumstick => U_WALL,
            Body::Upright => u_wall(pos),
        }
    }

    /// The microphones a preset stands for (omni, aimed at the board), or None for the
    /// recording's position.
    pub fn preset_setup(self, pos: MicPosition, spacing: f64, distance: f64, lid: f64) -> Option<MicSetup> {
        let lid = if self.has_lid() { lid } else { 1.0 }; // no lid: one value (the radiation cache's key)
        let p = self.placement(pos, spacing, distance, lid)?;
        Some(MicSetup { mics: [Mic::toward_board(p.mics[0]), Mic::toward_board(p.mics[1])], wall: self.wall_of(pos), lid })
    }

    /// (left, right) microphone positions, or None for the recording's position.
    pub fn mics(self, pos: MicPosition, spacing: f64, distance: f64, lid: f64) -> Option<[[f64; 3]; 2]> {
        self.placement(pos, spacing, distance, lid).map(|p| p.mics)
    }

    /// The pair with its limits and drag axes, or None for the recording's position.
    pub fn placement(self, pos: MicPosition, spacing: f64, distance: f64, lid: f64) -> Option<Placement> {
        match self {
            Body::Grand => pos.grand_placement(spacing, distance, lid),
            Body::Upright => pos.upright_placement(spacing, distance, lid),
            Body::Harp | Body::LeverHarp => pos.harp_placement(self.harp_dims(), spacing, distance),
            bar_bodies!() => pos.bars_placement(self.frame(), spacing, distance),
            Body::Guitar | Body::Strumstick => pos.guitar_placement(self.guitar_dims(), spacing, distance),
            Body::Psaltery | Body::DanTranh => pos.bars_placement(self.zither_dims().frame, spacing, distance),
        }
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let n = dot(v, v).sqrt();
    if n < 1e-12 { [0.0, 0.0, 1.0] } else { [v[0] / n, v[1] / n, v[2] / n] }
}

impl Mic {
    /// The unit vector the microphone points along.
    pub fn aim(&self) -> [f64; 3] {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        [y.sin() * p.cos(), y.cos() * p.cos(), p.sin()]
    }

    /// Sensitivity to sound arriving from the unit direction `arrive` (from the microphone
    /// towards the source): 1 on axis for every pattern; a figure-8's rear lobe is negative.
    pub fn gain(&self, arrive: [f64; 3]) -> f64 {
        (1.0 - self.pattern) + self.pattern * dot(self.aim(), arrive)
    }

    /// Sensitivity to a diffuse field, re an omni microphone.
    pub fn diffuse_sensitivity(&self) -> f64 {
        ((1.0 - self.pattern).powi(2) + self.pattern * self.pattern / 3.0).sqrt()
    }
}

impl MicSetup {
    /// What the diffuse room does to the pair: each channel's sensitivity, and the factor on
    /// the two channels' coherence (exact for a coincident pair, an approximation for a
    /// spaced one): 1 for omni, 0 for a Blumlein pair, −1 for back-to-back figure-8s.
    pub fn diffuse(&self) -> ([f64; 2], f64) {
        let [a, b] = self.mics;
        let (sa, sb) = (a.diffuse_sensitivity(), b.diffuse_sensitivity());
        let cos = dot(a.aim(), b.aim());
        let c = ((1.0 - a.pattern) * (1.0 - b.pattern) + a.pattern * b.pattern * cos / 3.0) / (sa * sb);
        ([sa, sb], c.clamp(-1.0, 1.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MicPosition {
    /// the fitted sound as it is
    #[default]
    Recording,
    /// the pianist's ears at the keyboard
    Player,
    /// grand: a pair in front of the open lid, on the curved side; upright: a pair in
    /// front of the piano, to the treble side
    Audience,
    /// close 1. Grand: under the open lid, a pair above the strings along the hammer
    /// line; upright: a pair above the open top, along the hammer line
    Hammers,
    /// close 2. Grand: under the open lid, a pair over the rear of the soundboard,
    /// towards the tail; upright: a pair behind the board, the piano pulled from the wall
    Tail,
    /// free microphones (`Tuning::mic_setup`)
    Custom,
}

impl MicPosition {
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => MicPosition::Player,
            2 => MicPosition::Audience,
            3 => MicPosition::Hammers,
            4 => MicPosition::Tail,
            5 => MicPosition::Custom,
            _ => MicPosition::Recording,
        }
    }

    /// The grand's default distance (m): horizontal from the soundboard's center, or for
    /// the close positions the height above the soundboard (the upright's defaults are
    /// in `upright_placement`).
    pub fn default_distance(self) -> f64 {
        match self {
            MicPosition::Player => 1.25,
            MicPosition::Hammers => 0.3,
            MicPosition::Tail => 0.45,
            _ => 3.5,
        }
    }

    /// The grand's close positions: inside the case, under the lid.
    pub fn close(self) -> bool {
        matches!(self, MicPosition::Hammers | MicPosition::Tail)
    }

    /// The upright's (left, right) microphones (`U_*`): player at the keyboard (ears
    /// 1.2 m above the floor; distance from the board, default 1.0 m, at least 0.8),
    /// audience in front, to the treble side (default 3.5 m, at least 1), close 1 above
    /// the open top along the hammer line (distance = height above the opening,
    /// default 0.3 m, 0.08 … 1), close 2 behind the board (distance from it, default
    /// 0.4 m, from 5 cm behind the case to 0.1 m off the pulled-out wall). Pairs across
    /// the line of sight; close pairs within the case width.
    fn upright_placement(self, spacing: f64, distance: f64, lid: f64) -> Option<Placement> {
        let h = spacing / 2.0;
        let pick = |default: f64, lo: f64, hi: f64| if distance > 0.0 { distance.clamp(lo, hi) } else { default };
        let open = |mics, axis, origin, distance, range| Placement {
            mics, axis, origin, distance, range, spread: X_AXIS, spacing, spacing_max: MAX_SPACING, height: false,
        };
        match self {
            MicPosition::Recording | MicPosition::Custom => None,
            MicPosition::Player => {
                let d = pick(1.0, 0.8, MAX_DISTANCE);
                Some(open([[-h, -d, 0.45], [h, -d, 0.45]], FRONT, [0.0; 3], d, (0.8, MAX_DISTANCE)))
            }
            MicPosition::Audience => {
                let d = pick(3.5, 1.0, MAX_DISTANCE);
                Some(open([[0.5 - h, -d, U_AUDIENCE_Z], [0.5 + h, -d, U_AUDIENCE_Z]], FRONT, [0.0; 3], d, (1.0, MAX_DISTANCE)))
            }
            MicPosition::Hammers => {
                let (h, top) = (h.min(U_HALF_SPACING), pick(0.3, U_TOP_HEIGHT.0, U_TOP_HEIGHT.1));
                // the closing lid rises through the pair's height: it is lifted above the lid's surface
                let top = top.max(u_lid_lift(U_HAMMER_Y, lid) - U_CASE_TOP);
                let z = U_CASE_TOP + top;
                Some(Placement {
                    mics: [[-h, U_HAMMER_Y, z], [h, U_HAMMER_Y, z]],
                    axis: Z_AXIS,
                    origin: [0.0, 0.0, U_CASE_TOP],
                    distance: top,
                    range: U_TOP_HEIGHT,
                    spread: X_AXIS,
                    spacing: 2.0 * h,
                    spacing_max: 2.0 * U_HALF_SPACING,
                    height: true,
                })
            }
            MicPosition::Tail => {
                let range = (U_CASE_Y.1 + 0.05, U_WALL_BACK - 0.1);
                let (h, d) = (h.min(U_HALF_SPACING), pick(0.4, range.0, range.1));
                Some(Placement {
                    mics: [[-h, d, 0.0], [h, d, 0.0]],
                    axis: Y_AXIS,
                    origin: [0.0; 3],
                    distance: d,
                    range,
                    spread: X_AXIS,
                    spacing: 2.0 * h,
                    spacing_max: 2.0 * U_HALF_SPACING,
                    height: false,
                })
            }
        }
    }

    /// The harp's (left, right) microphones (`H_*`): player at the harpist's ears, left of the
    /// soundbox's top and behind it (distance back from the board's center line, default
    /// 0.55 m, at least 0.4), audience in front (default 3.5 m, at least 1), close 1 in front
    /// of the board's middle (distance off the board, default 0.4 m, 0.15 … 1.5) and close 2
    /// behind the sound holes (level, back from the soundbox's back, default 0.5 m, 0.1 … 1: at
    /// 0.3 m the holes' low end put it 3.4 dB over the recording's position), both pairs
    /// spread along the board (the left one towards its foot: the bass).
    fn harp_placement(self, hp: HarpDims, spacing: f64, distance: f64) -> Option<Placement> {
        let h = spacing / 2.0;
        let pick = |default: f64, lo: f64, hi: f64| if distance > 0.0 { distance.clamp(lo, hi) } else { default };
        let open = |mics, axis, origin, distance, range, spread| Placement {
            mics, axis, origin, distance, range, spread, spacing, spacing_max: MAX_SPACING, height: false,
        };
        let (u, n) = h_axes();
        let along = |c: [f64; 3], k: f64| [c[0] + k * u[0], c[1] + k * u[1], c[2] + k * u[2]];
        let off = |c: [f64; 3], v: [f64; 3], d: f64| [c[0] + d * v[0], c[1] + d * v[1], c[2] + d * v[2]];
        match self {
            MicPosition::Recording | MicPosition::Custom => None,
            MicPosition::Player => {
                let d = pick(-hp.ear().1, 0.4, MAX_DISTANCE);
                let origin = [hp.ear().0, 0.0, hp.ear_z()];
                Some(open([[hp.ear().0 - h, -d, hp.ear_z()], [hp.ear().0 + h, -d, hp.ear_z()]], FRONT, origin, d, (0.4, MAX_DISTANCE), X_AXIS))
            }
            MicPosition::Audience => {
                let d = pick(3.5, 1.0, MAX_DISTANCE);
                Some(open([[0.5 - h, d, hp.ear_z()], [0.5 + h, d, hp.ear_z()]], Y_AXIS, [0.0; 3], d, (1.0, MAX_DISTANCE), X_AXIS))
            }
            MicPosition::Hammers | MicPosition::Tail => {
                let h = h.min(hp.close_spacing_max() / 2.0);
                let (origin, axis, d, range) = if self == MicPosition::Hammers {
                    let range = (0.15, 1.5);
                    (hp.on_board(0.0), n, pick(0.4, range.0, range.1), range)
                } else {
                    // straight back from the holes (along the box's normal it would go into the floor)
                    let range = (0.1, 1.0);
                    (hp.on_back(0.0), FRONT, pick(0.5, range.0, range.1), range)
                };
                let c = off(origin, axis, d);
                Some(Placement {
                    mics: [along(c, -h), along(c, h)],
                    axis,
                    origin,
                    distance: d,
                    range,
                    spread: u,
                    spacing: 2.0 * h,
                    spacing_max: hp.close_spacing_max(),
                    height: false,
                })
            }
        }
    }

    /// Bars' (left, right) microphones (`BarFrame`): player at the player's ears,
    /// standing behind the low row (distance back from the bars' center, default 0.5 m, at
    /// least 0.3), audience in front (default 3.5 m, at least 1, at a seated listener's
    /// height), close 1 above the bars (height above them, default 0.3 m, 0.08 … 1.5) and close
    /// 2 in front of the resonators (distance from the frame's front, default 0.4 m, 0.1 … 1.5,
    /// halfway down the tubes); pairs along the bars (the left one at the bass).
    fn bars_placement(self, fr: BarFrame, spacing: f64, distance: f64) -> Option<Placement> {
        let h = spacing / 2.0;
        let pick = |default: f64, lo: f64, hi: f64| if distance > 0.0 { distance.clamp(lo, hi) } else { default };
        let ear_z = fr.ear.1 - fr.height;
        let open = |mics, axis, origin, distance, range| Placement {
            mics, axis, origin, distance, range, spread: X_AXIS, spacing, spacing_max: MAX_SPACING, height: false,
        };
        match self {
            MicPosition::Recording | MicPosition::Custom => None,
            MicPosition::Player => {
                let d = pick(-fr.ear.0, 0.3, MAX_DISTANCE);
                Some(open([[-h, -d, ear_z], [h, -d, ear_z]], FRONT, [0.0; 3], d, (0.3, MAX_DISTANCE)))
            }
            MicPosition::Audience => {
                let d = pick(3.5, 1.0, MAX_DISTANCE);
                Some(open([[-h, d, 0.35], [h, d, 0.35]], Y_AXIS, [0.0; 3], d, (1.0, MAX_DISTANCE)))
            }
            MicPosition::Hammers | MicPosition::Tail => {
                let h = h.min(fr.close_spacing_max / 2.0);
                let (mics, axis, origin, d, range, height) = if self == MicPosition::Hammers {
                    let range = (0.08, 1.5);
                    let d = pick(0.3, range.0, range.1);
                    ([[-h, 0.0, d], [h, 0.0, d]], Z_AXIS, [0.0; 3], d, range, true)
                } else {
                    let range = (0.1, 1.5);
                    let d = pick(0.4, range.0, range.1);
                    let z = -0.5 * fr.tube(0.0);
                    ([[-h, fr.half_depth + d, z], [h, fr.half_depth + d, z]], Y_AXIS, [0.0, fr.half_depth, z], d, range, false)
                };
                Some(Placement { mics, axis, origin, distance: d, range, spread: X_AXIS, spacing: 2.0 * h, spacing_max: fr.close_spacing_max, height })
            }
        }
    }

    /// The guitar's (left, right) microphones (`G_*`): player at the guitarist's ears, above the
    /// upper bout and behind the top (distance back from the top, default 0.3 m, at least 0.15),
    /// audience in front (default 3.5 m, at least 1, at a seated listener's height), close 1 in
    /// front of the sound hole and the 12th fret, close 2 in front of the lower bout (distance
    /// off the top, default 0.3 m, 0.08 … 1.5), pairs along the strings (the left one towards the neck).
    fn guitar_placement(self, gd: GuitarDims, spacing: f64, distance: f64) -> Option<Placement> {
        let h = spacing / 2.0;
        let pick = |default: f64, lo: f64, hi: f64| if distance > 0.0 { distance.clamp(lo, hi) } else { default };
        let open = |mics, axis, origin, distance, range| Placement {
            mics, axis, origin, distance, range, spread: X_AXIS, spacing, spacing_max: MAX_SPACING, height: false,
        };
        match self {
            MicPosition::Recording | MicPosition::Custom => None,
            MicPosition::Player => {
                let [ex, ey, ez] = G_EAR;
                let d = pick(-ey, 0.15, MAX_DISTANCE);
                Some(open([[ex - h, -d, ez], [ex + h, -d, ez]], FRONT, [ex, 0.0, ez], d, (0.15, MAX_DISTANCE)))
            }
            MicPosition::Audience => {
                let d = pick(3.5, 1.0, MAX_DISTANCE);
                Some(open([[-h, d, G_AUDIENCE_Z], [h, d, G_AUDIENCE_Z]], Y_AXIS, [0.0; 3], d, (1.0, MAX_DISTANCE)))
            }
            MicPosition::Hammers | MicPosition::Tail => {
                let (close_x, close_max) = gd.close();
                let h = h.min(close_max / 2.0);
                let x = if self == MicPosition::Hammers { close_x.0 } else { close_x.1 };
                let range = (0.08, 1.5);
                let d = pick(0.3, range.0, range.1);
                Some(Placement {
                    mics: [[x - h, d, 0.0], [x + h, d, 0.0]],
                    axis: Y_AXIS,
                    origin: [x, 0.0, 0.0],
                    distance: d,
                    range,
                    spread: X_AXIS,
                    spacing: 2.0 * h,
                    spacing_max: close_max,
                    height: false,
                })
            }
        }
    }

    /// The grand's (left, right) microphone positions: a pair `spacing` m apart, centered on the
    /// position at `distance` m from the board's center (0: the default; at least
    /// MIN_DISTANCE, outside the case), across the line of sight. Close positions: the
    /// pair across the piano (spacing at most 2·CLOSE_HALF_SPACING) at the height
    /// `distance` above the soundboard (0: the default), kept under the lid.
    fn grand_placement(self, spacing: f64, distance: f64, lid: f64) -> Option<Placement> {
        let h = spacing / 2.0;
        if self.close() {
            let h = h.min(CLOSE_HALF_SPACING);
            let height = if distance > 0.0 { distance } else { self.default_distance() };
            let (cx, cy) = if self == MicPosition::Hammers { (0.0, -0.55) } else { (-0.15, 0.40) };
            let at = |x: f64| [x, cy, height.min(lid_height(x, lid) - CLOSE_LID_CLEARANCE).max(CLOSE_MIN_HEIGHT)];
            let mics = [at(cx - h), at(cx + h)];
            let top = |x: f64| (lid_height(x, lid) - CLOSE_LID_CLEARANCE).max(CLOSE_MIN_HEIGHT);
            return Some(Placement {
                mics,
                axis: Z_AXIS,
                origin: [0.0; 3],
                // the lid rises across the pair and stops the bass-side microphone first: the
                // distance is the treble-side one's height (the parameter's value while it is
                // free), the limit its top
                distance: mics[1][2],
                range: (CLOSE_MIN_HEIGHT, top(cx + h)),
                spread: X_AXIS,
                spacing: 2.0 * h,
                spacing_max: 2.0 * CLOSE_HALF_SPACING,
                height: true,
            });
        }
        let d = if distance > 0.0 { distance.max(MIN_DISTANCE) } else { self.default_distance() };
        let (mics, axis, spread) = match self {
            // centered in front of the keyboard, side by side
            MicPosition::Player => ([[-h, -d, 0.45], [h, -d, 0.45]], FRONT, X_AXIS),
            // facing the piano from its curved side: left = towards the keyboard
            MicPosition::Audience => ([[d, -0.30 - h, 0.45], [d, -0.30 + h, 0.45]], X_AXIS, Y_AXIS),
            _ => return None,
        };
        Some(Placement { mics, axis, origin: [0.0; 3], distance: d, range: (MIN_DISTANCE, MAX_DISTANCE), spread, spacing, spacing_max: MAX_SPACING, height: false })
    }
}

/// What the UI draws (m; x across, y along, z up from the board): the case from above
/// (an illustrative outline within the engine's footprint) and from the front, the
/// keys, the open lid seen from the front, the floor, the upright's wall for a
/// position, and each position's pair center (from above, default distance).
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub case_top: Vec<[f64; 2]>,
    pub keys_top: [f64; 4],
    pub case_front: [f64; 4],
    pub keys_front: [f64; 4],
    pub lid_front: [[f64; 2]; 2],
    pub floor_z: f64,
    pub wall_y: Option<f64>,
    pub markers: [[f64; 2]; 5],
    /// the second view's horizontal axis: 0 x (from the front), 1 y (from the side: the harp)
    pub front_axis: usize,
    /// the second view's outline in its own (a, z), polygons separated by a NaN point; empty:
    /// the case_front rectangle (grand, upright)
    pub outline_front: Vec<[f64; 2]>,
}

impl Body {
    /// The nearest allowed point for a free microphone: inside ±MAX_DISTANCE, above the
    /// floor, and not through the instrument. Grand: over the case's footprint between the
    /// soundboard and the lid (never above the lid). Upright: not inside the case (above
    /// its open top is fine, but above the lid's surface while the lid is partly closed) and in front of
    /// the wall.
    pub fn clamp_mic(self, wall: f64, lid: f64, p: [f64; 3]) -> [f64; 3] {
        let hp = self.harp_dims();
        let floor = match self {
            Body::Grand => -BOARD_HEIGHT,
            Body::Upright => -U_BOARD_HEIGHT,
            Body::Harp | Body::LeverHarp => -hp.board_height(),
            bar_bodies!() => -self.frame().height,
            Body::Guitar | Body::Strumstick => -G_HEIGHT,
            Body::Psaltery | Body::DanTranh => -self.zither_dims().frame.height,
        };
        let mut q = [
            p[0].clamp(-MAX_DISTANCE, MAX_DISTANCE),
            p[1].clamp(-MAX_DISTANCE, MAX_DISTANCE),
            p[2].clamp(floor + FLOOR_CLEARANCE, MAX_MIC_HEIGHT),
        ];
        match self {
            Body::Grand => {
                if self.over_case(q) {
                    let top = if lid_off(lid) { MAX_MIC_HEIGHT } else { (lid_height(q[0], lid) - CLOSE_LID_CLEARANCE).max(CLOSE_MIN_HEIGHT) };
                    q[2] = q[2].clamp(CLOSE_MIN_HEIGHT, top);
                }
            }
            Body::Upright => {
                q[1] = q[1].min(wall - WALL_CLEARANCE);
                let roof = U_CASE_TOP + U_TOP_CLEARANCE;
                if self.over_case(q) && q[2] < roof {
                    // out through the nearest face: top, front, either side, back (if the wall leaves room)
                    let mut best = ([q[0], q[1], roof], roof - q[2]);
                    let mut try_face = |to: [f64; 3], d: f64| {
                        if d < best.1 {
                            best = (to, d);
                        }
                    };
                    try_face([q[0], U_CASE_Y.0 - WALL_CLEARANCE, q[2]], q[1] - (U_CASE_Y.0 - WALL_CLEARANCE));
                    try_face([U_CASE_X.0 - WALL_CLEARANCE, q[1], q[2]], q[0] - (U_CASE_X.0 - WALL_CLEARANCE));
                    try_face([U_CASE_X.1 + WALL_CLEARANCE, q[1], q[2]], (U_CASE_X.1 + WALL_CLEARANCE) - q[0]);
                    if U_CASE_Y.1 + WALL_CLEARANCE <= wall - WALL_CLEARANCE {
                        try_face([q[0], U_CASE_Y.1 + WALL_CLEARANCE, q[2]], (U_CASE_Y.1 + WALL_CLEARANCE) - q[1]);
                    }
                    q = best.0;
                }
                // never inside the wedge under a partly closed lid: above its surface
                if (U_CASE_X.0..=U_CASE_X.1).contains(&q[0]) {
                    if let Some(s) = u_lid_surface(q[1], lid) {
                        q[2] = q[2].max(s + U_TOP_CLEARANCE);
                    }
                }
            }
            Body::Harp | Body::LeverHarp => {
                // not inside the soundbox: out through its front (the board) or its back, whichever is nearer
                let (u, n) = h_axes();
                let (s, t) = (dot(q, u), dot(q, n)); // along the board, off its face (+ in front)
                let (depth, half) = hp.box_at(s);
                let inside = s.abs() <= hp.board_len() / 2.0 && q[0].abs() < half + H_CLEARANCE && t < H_CLEARANCE && t > -depth - H_CLEARANCE;
                if inside {
                    let to = if H_CLEARANCE - t <= t + depth + H_CLEARANCE { H_CLEARANCE } else { -depth - H_CLEARANCE };
                    q = [q[0] + (to - t) * n[0], q[1] + (to - t) * n[1], q[2] + (to - t) * n[2]];
                }
            }
            bar_bodies!() | Body::Psaltery | Body::DanTranh => {
                // not among the bars and tubes (in the zither's box): out through the nearest face
                // (top, front, back, the ends)
                let fr = if self.is_zither() { self.zither_dims().frame } else { self.frame() };
                let c = fr.clearance;
                let (xh, yh) = (fr.half_width + c, fr.half_depth + c);
                let bottom = -fr.tube(q[0]) - c;
                if q[0].abs() < xh && q[1].abs() < yh && q[2] < c && q[2] > bottom {
                    let faces = [
                        ([q[0], q[1], c], c - q[2]),
                        ([q[0], -yh, q[2]], q[1] + yh),
                        ([q[0], yh, q[2]], yh - q[1]),
                        ([-xh, q[1], q[2]], q[0] + xh),
                        ([xh, q[1], q[2]], xh - q[0]),
                    ];
                    q = faces.iter().min_by(|a, b| a.1.total_cmp(&b.1)).map_or(q, |f| f.0);
                }
            }
            Body::Guitar | Body::Strumstick => {
                // not inside the body: out through the nearest face (the top, the back, its rim)
                let (c, gd) = (G_CLEARANCE, self.guitar_dims());
                let half = gd.half_width(q[0]);
                if half > 0.0 && q[2].abs() < half + c && q[1] < c && q[1] > -gd.depth - c {
                    let rim = (half + c).copysign(if q[2] == 0.0 { 1.0 } else { q[2] });
                    let faces = [([q[0], c, q[2]], c - q[1]), ([q[0], -gd.depth - c, q[2]], q[1] + gd.depth + c), ([q[0], q[1], rim], half + c - q[2].abs())];
                    q = faces.iter().min_by(|a, b| a.1.total_cmp(&b.1)).map_or(q, |f| f.0);
                }
            }
        }
        q
    }

    /// A setup with every microphone clamped, patterns in 0…1, pitch in ±90°, the wall in range
    /// (the grand's wall is fixed). Non-finite values become the defaults first.
    pub fn clamp_setup(self, s: MicSetup) -> MicSetup {
        let d = MicSetup::default();
        let f = |v: f64, d: f64| if v.is_finite() { v } else { d };
        // the grand ignores the wall: one value, so it never changes the radiation cache key
        let wall = match self {
            Body::Grand | Body::Harp | Body::LeverHarp | bar_bodies!() | Body::Guitar | Body::Psaltery | Body::DanTranh | Body::Strumstick => U_WALL,
            Body::Upright => f(s.wall, d.wall).clamp(U_WALL, MAX_WALL),
        };
        // … and the harp and the vibraphone the lid (they have none)
        let lid = if self.has_lid() { lid_value(s.lid) } else { 1.0 };
        let fix = |m: Mic, d: Mic| Mic {
            pos: self.clamp_mic(wall, lid, [f(m.pos[0], d.pos[0]), f(m.pos[1], d.pos[1]), f(m.pos[2], d.pos[2])]),
            pattern: f(m.pattern, d.pattern).clamp(0.0, 1.0),
            yaw: f(m.yaw, d.yaw),
            pitch: f(m.pitch, d.pitch).clamp(-90.0, 90.0),
        };
        MicSetup { mics: [fix(s.mics[0], d.mics[0]), fix(s.mics[1], d.mics[1])], wall, lid }
    }

    pub fn drawing(self, wall: f64, lid: f64) -> Drawing {
        let hp = self.harp_dims();
        let mut markers = [[f64::NAN; 2]; 5];
        for (i, p) in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail].into_iter().enumerate() {
            let [a, b] = self.placement(p, 0.5, 0.0, lid).expect("a pair").mics;
            markers[i + 1] = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        }
        match self {
            Body::Grand => {
                let a = lid_angle(LID_ANGLE_DEG, lid);
                Drawing {
                    // straight spine on the bass side, the bent side curving to the tail
                    case_top: vec![
                        [CASE_X.0, -0.80], [CASE_X.1, -0.80], [CASE_X.1, -0.10], [0.62, 0.18], [0.42, 0.42],
                        [0.25, 0.68], [0.12, 0.90], [-0.10, CASE_Y.1], [CASE_X.0, CASE_Y.1],
                    ],
                    keys_top: [CASE_X.0, CASE_Y.0, CASE_X.1, -0.80],
                    case_front: [CASE_X.0, -0.25, CASE_X.1, LID_HINGE.1],
                    keys_front: [CASE_X.0, -0.08, CASE_X.1, -0.02],
                    lid_front: if lid_off(lid) {
                        [[f64::NAN; 2]; 2]
                    } else {
                        [[LID_HINGE.0, LID_HINGE.1], [LID_HINGE.0 + LID_WIDTH * a.cos(), LID_HINGE.1 + LID_WIDTH * a.sin()]]
                    },
                    floor_z: -BOARD_HEIGHT,
                    wall_y: None,
                    markers,
                    front_axis: 0,
                    outline_front: Vec::new(),
                }
            }
            Body::Upright => {
                let a = lid_angle(U_LID_ANGLE_DEG, lid);
                Drawing {
                    case_top: vec![[U_CASE_X.0, -0.45], [U_CASE_X.1, -0.45], [U_CASE_X.1, U_CASE_Y.1], [U_CASE_X.0, U_CASE_Y.1]],
                    keys_top: [U_CASE_X.0, U_CASE_Y.0, U_CASE_X.1, -0.45],
                    case_front: [U_CASE_X.0, -U_BOARD_HEIGHT, U_CASE_X.1, U_CASE_TOP],
                    keys_front: [U_CASE_X.0, -0.05, U_CASE_X.1, 0.0],
                    lid_front: if lid_off(lid) {
                        [[f64::NAN; 2]; 2]
                    } else {
                        [[U_CASE_X.0, U_CASE_TOP], [U_CASE_X.1, U_CASE_TOP + U_LID_DEPTH * a.sin()]]
                    },
                    floor_z: -U_BOARD_HEIGHT,
                    wall_y: Some(wall),
                    markers,
                    front_axis: 0,
                    outline_front: Vec::new(),
                }
            }
            Body::Harp | Body::LeverHarp => {
                let half = hp.board_len() / 2.0;
                let (foot, top) = (hp.on_board(-half), hp.on_board(half));
                let (bfoot, btop) = (hp.on_back(-half), hp.on_back(half));
                let (hw0, hw1) = hp.half_width();
                let floor = -hp.board_height();
                let nan = [f64::NAN; 2];
                Drawing {
                    // from above: the soundbox (wide at the foot, in front; narrow at the top, by the
                    // player), the column and the base in front of it
                    case_top: vec![
                        [-hw0, foot[1]], [-hw1, top[1]], [-hw1, btop[1]], [hw1, btop[1]], [hw1, top[1]], [hw0, foot[1]],
                        [0.08, hp.column_y() + 0.06], [-0.08, hp.column_y() + 0.06],
                    ],
                    // the pedals in the base (a harp's "keys")
                    keys_top: [-0.20, foot[1] + 0.05, 0.20, foot[1] + 0.20],
                    case_front: [-hw0, floor, hw0, hp.top_z()],
                    keys_front: [f64::NAN; 4],
                    lid_front: [nan, nan],
                    floor_z: floor,
                    wall_y: None,
                    markers,
                    front_axis: 1,
                    // from the side (y, z): the soundbox; the base; the column and the neck
                    outline_front: vec![
                        [foot[1], foot[2]], [top[1], top[2]], [btop[1], btop[2]], [bfoot[1], bfoot[2]],
                        nan,
                        [bfoot[1], floor], [hp.column_y() + 0.08, floor], [hp.column_y() + 0.08, foot[2]], [bfoot[1], foot[2] - 0.05],
                        nan,
                        [hp.column_y(), foot[2]], [hp.column_y() + 0.07, foot[2]], [hp.column_y() + 0.07, hp.top_z()],
                        [0.30, hp.top_z() - 0.10], [top[1] - 0.05, top[2] + 0.12], [top[1], top[2]],
                        [top[1] + 0.06, top[2] + 0.02], [0.30, hp.top_z() - 0.18], [hp.column_y(), hp.top_z() - 0.10],
                    ],
                }
            }
            bar_bodies!() => {
                if self == Body::Kalimba {
                    // held in the hands, no stand: from above the board (its corners cut) and the
                    // tines over it towards the player (−y); from the front the player, the board,
                    // the tines' row (the bass's long ones left, as the engine lays the keys) and the
                    // two hands
                    let nan = [f64::NAN; 2];
                    let fr = self.frame();
                    let (xw, yd, ht, bx) = (fr.half_width, fr.half_depth, fr.height, fr.bar_x);
                    let c = 0.025;
                    return Drawing {
                        case_top: vec![
                            [-xw + c, -yd], [xw - c, -yd], [xw, -yd + c], [xw, yd - c], [xw - c, yd], [-xw + c, yd], [-xw, yd - c], [-xw, -yd + c],
                        ],
                        keys_top: [bx.0, -yd + 0.02, bx.1, 0.03],
                        case_front: [-xw, -0.05, xw, 0.0],
                        keys_front: [f64::NAN; 4],
                        lid_front: [nan, nan],
                        floor_z: -ht,
                        wall_y: None,
                        markers,
                        front_axis: 0,
                        // the seated player behind it first (its torso from the seat at 0.45 m to the
                        // shoulders, the head at the ear's height), then the board in front
                        outline_front: vec![
                            [-0.17, -0.55], [0.17, -0.55], [0.20, 0.12], [-0.20, 0.12],
                            nan,
                            [-0.07, 0.17], [0.07, 0.17], [0.09, 0.27], [0.06, 0.37], [-0.06, 0.37], [-0.09, 0.27],
                            nan,
                            [-xw, -0.05], [xw, -0.05], [xw, -0.02], [-xw, -0.02],
                            nan,
                            [bx.0, -0.02], [bx.1, -0.02], [bx.1, -0.008], [bx.0, 0.006],
                            nan,
                            [-xw - 0.035, -0.08], [-xw + 0.005, -0.08], [-xw + 0.005, 0.005], [-xw - 0.035, 0.005],
                            nan,
                            [xw - 0.005, -0.08], [xw + 0.035, -0.08], [xw + 0.035, 0.005], [xw - 0.005, 0.005],
                        ],
                    };
                }
                let nan = [f64::NAN; 2];
                let fr = self.frame();
                let (xw, yd, ht, bx) = (fr.half_width, fr.half_depth, fr.height, fr.bar_x);
                Drawing {
                    // from above: the frame; the bars' two rows as the "keys"
                    case_top: vec![[-xw, -yd], [xw, -yd], [xw, yd], [-xw, yd]],
                    keys_top: [bx.0 - 0.03, fr.natural_y - 0.08, bx.1 + 0.03, fr.accidental.0 + 0.07],
                    case_front: [-xw, -ht, xw, 0.0],
                    keys_front: [bx.0 - 0.03, -0.01, bx.1 + 0.03, fr.accidental.1 + 0.01],
                    lid_front: [nan, nan],
                    floor_z: -ht,
                    wall_y: None,
                    markers,
                    front_axis: 0,
                    // from the front (x, z): the resonators' row, long at the bass, then the legs
                    outline_front: vec![
                        [bx.0, -0.03], [bx.1, -0.03], [bx.1, -fr.tube(bx.1)], [bx.0, -fr.tube(bx.0)],
                        nan,
                        [-xw + 0.04, -0.02], [-xw + 0.06, -0.02], [-xw + 0.06, -ht], [-xw + 0.04, -ht],
                        nan,
                        [xw - 0.06, -0.02], [xw - 0.04, -0.02], [xw - 0.04, -ht], [xw - 0.06, -ht],
                    ],
                }
            }
            Body::Guitar | Body::Strumstick => {
                let nan = [f64::NAN; 2];
                let gd = self.guitar_dims();
                let o = gd.outline();
                let (x0, x1) = (o[o.len() - 1].0, o[0].0);
                let (hx, hr) = gd.hole();
                // from the front (x, z): the top's outline (upper half, then mirrored), the
                // sound hole, the neck and the head
                let mut front: Vec<[f64; 2]> = o.iter().map(|&(x, w)| [x, w]).collect();
                front.extend(o.iter().rev().skip(1).take(o.len() - 2).map(|&(x, w)| [x, -w]));
                front.push(nan);
                front.extend((0..8).map(|i| {
                    let a = i as f64 * std::f64::consts::FRAC_PI_4;
                    [hx + hr * a.cos(), hr * a.sin()]
                }));
                front.push(nan);
                let nh = gd.neck_half;
                front.extend([[gd.neck_x.0, nh], [gd.head_x, nh + 0.012], [gd.head_x, -nh - 0.012], [gd.neck_x.0, -nh]]);
                Drawing {
                    // from above: the body, its depth behind the top
                    case_top: vec![[x0, -gd.depth], [x1, -gd.depth], [x1, 0.0], [x0, 0.0]],
                    // the neck from the nut to the joint (the "keys": the frets)
                    keys_top: [gd.neck_x.1, -0.03, gd.neck_x.0, 0.0],
                    case_front: [gd.head_x, -G_HEIGHT, x1, gd.half_width(-0.03)],
                    keys_front: [f64::NAN; 4],
                    lid_front: [nan, nan],
                    floor_z: -G_HEIGHT,
                    wall_y: None,
                    markers,
                    front_axis: 0,
                    outline_front: front,
                }
            }
            Body::Psaltery | Body::DanTranh => {
                // from above the box and its strings' bridges (the "keys"); from the front the box
                // on its table (its top and two legs)
                let nan = [f64::NAN; 2];
                let zd = self.zither_dims();
                let fr = zd.frame;
                let (xw, ht, dz) = (fr.half_width, fr.height, fr.tubes.0);
                let (bx, by) = (zd.bass, zd.treble);
                let (tw, tt) = (xw + 0.06, 0.03);
                Drawing {
                    case_top: zd.outline.to_vec(),
                    keys_top: [bx[0].min(by[0]) - 0.01, bx[1].min(by[1]) - 0.01, bx[0].max(by[0]) + 0.01, bx[1].max(by[1]) + 0.01],
                    case_front: [-xw, -dz, xw, 0.0],
                    keys_front: [bx[0], -0.006, by[0], 0.004],
                    lid_front: [nan, nan],
                    floor_z: -ht,
                    wall_y: None,
                    markers,
                    front_axis: 0,
                    outline_front: vec![
                        [-xw, -dz], [xw, -dz], [xw, 0.0], [-xw, 0.0],
                        nan,
                        [-tw, -dz - tt], [tw, -dz - tt], [tw, -dz], [-tw, -dz],
                        nan,
                        [-tw + 0.02, -ht], [-tw + 0.05, -ht], [-tw + 0.05, -dz - tt], [-tw + 0.02, -dz - tt],
                        nan,
                        [tw - 0.05, -ht], [tw - 0.02, -ht], [tw - 0.02, -dz - tt], [tw - 0.05, -dz - tt],
                    ],
                }
            }
        }
    }
}

/// The upright's top lid surface height at y (m) for the opening `lid`, where the lid is: opened
/// less than 90 degrees (it leans forward over the case) and y within its extent. None elsewhere
/// (open past 90 degrees it leans back behind the case).
fn u_lid_surface(y: f64, lid: f64) -> Option<f64> {
    let a = lid_angle(U_LID_ANGLE_DEG, lid);
    (a < std::f64::consts::FRAC_PI_2 && y <= U_CASE_Y.1 && y >= U_CASE_Y.1 - U_LID_DEPTH * a.cos())
        .then(|| U_CASE_TOP + (U_CASE_Y.1 - y) * a.tan())
}

/// The height the Hammers pair is lifted to at y: the lid surface plus clearance while the lid is overhead, easing
/// back to nothing over `U_LIFT_EASE` m of the free edge's travel once the lid opens past y, so the pair does not
/// jump (0 where the lid leans back behind the case).
fn u_lid_lift(y: f64, lid: f64) -> f64 {
    const U_LIFT_EASE: f64 = 0.15;
    let a = lid_angle(U_LID_ANGLE_DEG, lid);
    if a >= std::f64::consts::FRAC_PI_2 || y > U_CASE_Y.1 {
        return 0.0;
    }
    let past = (U_CASE_Y.1 - U_LID_DEPTH * a.cos() - y).max(0.0); // how far the free edge has moved beyond y
    let t = (1.0 - past / U_LIFT_EASE).clamp(0.0, 1.0);
    (U_CASE_TOP + (U_CASE_Y.1 - y) * a.tan() + U_TOP_CLEARANCE) * t * t * (3.0 - 2.0 * t)
}

/// The lid taken off (`MicSetup::lid`): no lid reflection, no closed lid's loss, nothing over a
/// close pair; the positions stay those of the open lid. Any lid above 1 is this.
pub const LID_OFF: f64 = 2.0;

/// A lid value in its range: 0 closed … 1 open, `LID_OFF` above 1, the open lid if not finite.
pub fn lid_value(lid: f64) -> f64 {
    if !lid.is_finite() {
        1.0
    } else if lid > 1.0 {
        LID_OFF
    } else {
        lid.max(0.0)
    }
}

fn lid_off(lid: f64) -> bool {
    lid > 1.0
}

/// The lid's opening angle (rad) for the full angle (degrees) and the opening `lid` (0 closed … 1 as modeled;
/// taken off, the open lid's).
fn lid_angle(full_deg: f64, lid: f64) -> f64 {
    (full_deg * lid.min(1.0)).to_radians()
}

/// The lid's height above the soundboard at x (m) for the opening `lid` (0 closed … 1 as modeled).
fn lid_height(x: f64, lid: f64) -> f64 {
    LID_HINGE.1 + (x - LID_HINGE.0) * lid_angle(LID_ANGLE_DEG, lid).tan()
}

/// A sound path: length (m), amplitude factor (source gain × reflection factor ×
/// diffraction), cosine of the departure angle from the source face's normal, up to
/// three first-order lowpasses (source, diffraction, lid; Hz), a first-order highpass
/// (a bounded reflector's size, `edge_hz`), the reflector it went via.
#[derive(Clone, Copy)]
struct Path {
    length: f64,
    gain: f64,
    cos_theta: f64,
    lowpass: [Option<f64>; 3],
    highpass: Option<f64>,
    /// the sound left through the lid (not via the underside)
    lid_path: bool,
    #[cfg_attr(not(test), allow(dead_code))] // which path is which: the tests
    via: Option<usize>,
    /// unit vector from the microphone towards the source (or the reflection's hit point)
    arrive: [f64; 3],
}

/// The closed lid's loss (starting values, tuned by ear): sound leaving through the lid is this much
/// quieter and lowpassed at this frequency when the lid is shut; both fade in as it closes.
pub const LID_LEAK_DB: f64 = -9.0;
pub const LID_LEAK_HZ: f64 = 1200.0;
/// the lowpass's reference frequency at an open lid (Hz)
const LID_OPEN_HZ: f64 = 20000.0;

/// (amplitude gain, lowpass Hz) of the sound leaving through the lid, or None when it is open.
fn lid_leak(lid: f64) -> Option<(f64, f64)> {
    let closure = (1.0 - lid).clamp(0.0, 1.0);
    if !closure.is_finite() || closure <= 0.0 {
        return None;
    }
    let gain = 10f64.powf(LID_LEAK_DB * closure / 20.0);
    let hz = (LID_OPEN_HZ.ln() * (1.0 - closure) + LID_LEAK_HZ.ln() * closure).exp();
    Some((gain, hz))
}

/// The share of the room's excitation that comes through the lid (starting value, tuned by ear).
pub const LID_ROOM_SHARE: f64 = 0.5;

/// The closed lid's loss on the diffuse room: the top face's share of the room's excitation leaks
/// like the direct sound (quieter, lowpassed), the rest is unaffected. Does nothing at an open lid.
pub fn lid_room(ir: &mut [f32], lid: f64, sr: f64) {
    let Some((g, hz)) = lid_leak(lid) else { return };
    let s = LID_ROOM_SHARE;
    let a = (-2.0 * std::f64::consts::PI * hz / sr).exp();
    let mut l = 0.0f64;
    for v in ir.iter_mut() {
        let x = *v as f64;
        l = (1.0 - a) * x + a * l;
        *v = ((1.0 - s) * x + s * g * l) as f32;
    }
}

/// Where a bounded reflector stops reflecting (Hz): below it the reflection falls 6 dB per octave
/// (Rindel 1986, the panel's characteristic frequency c·d*/(2·(a·cos θ)²), d* = 2·d1·d2/(d1 + d2)
/// from the legs d1, d2, a its smaller side, θ the incidence). The open lid as a mirror at every
/// frequency put the bass at Player 4 dB to its side (design log step 187).
fn edge_hz(r: &Reflector, d1: f64, d2: f64, cos_in: f64) -> Option<f64> {
    let Extent::Rect { u_range, w_range, .. } = r.extent else { return None };
    let a = (u_range.1 - u_range.0).min(w_range.1 - w_range.0) * cos_in.abs().max(0.05);
    Some(SPEED_OF_SOUND * (2.0 * d1 * d2 / (d1 + d2).max(1e-9)) / (2.0 * a * a))
}

/// diffraction around the case for a one-sided source's blocked path
const DIFFRACTION_GAIN: f64 = 0.316_227_766_016_837_94; // −10 dB
const DIFFRACTION_LOWPASS_HZ: f64 = 500.0;

pub fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Whether the segment a → b passes through a bounded reflector (the lid): an unbounded
/// one (floor, wall) is never in the way of a path between points in front of it.
fn crosses(r: &Reflector, a: [f64; 3], b: [f64; 3]) -> bool {
    let Extent::Rect { u, u_range, w, w_range } = r.extent else { return false };
    let side = |p: [f64; 3]| dot(sub(p, r.origin), r.normal);
    let (sa, sb) = (side(a), side(b));
    if sa * sb >= 0.0 {
        return false;
    }
    let t = sa / (sa - sb);
    let rel = sub([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])], r.origin);
    (u_range.0..=u_range.1).contains(&dot(rel, u)) && (w_range.0..=w_range.1).contains(&dot(rel, w))
}

/// The paths from a source to a microphone: direct, then one per reflector whose
/// reflection point lies on it (mirror sources), in the reflectors' order; a
/// reflection whose legs pass through another, bounded reflector (the upright's wall
/// behind its lid) is dropped.
fn paths(src: &Source, mic: [f64; 3], refl: &[Reflector], mic_over_case: bool) -> Vec<Path> {
    let cos = |d: f64, len: f64| (d.abs() / len.max(1e-9)).min(1.0);
    // a one-sided source with the microphone behind its face, or radiating towards its
    // back: around the case instead (also its mirror paths: the upright's wall image of
    // the back face, seen from in front of the board)
    let behind = src.one_sided && dot(sub(mic, src.pos), src.normal) < 0.0;
    let make = |length: f64, gain: f64, dep: [f64; 3], via: Option<usize>, arrive: [f64; 3], highpass: Option<f64>| {
        let blocked = src.one_sided && (behind || dot(dep, src.normal) < 0.0);
        Path {
            length,
            gain: if blocked { gain * DIFFRACTION_GAIN } else { gain },
            cos_theta: cos(dot(dep, src.normal), length),
            lowpass: [src.lowpass_hz, if blocked { Some(DIFFRACTION_LOWPASS_HZ) } else { None }, None],
            highpass,
            // the grand's floor path is the underside radiation (no leak); the upright's open-top source's
            // small diffracted floor path (-10 dB diffraction x 0.8) is deliberately left alone
            lid_path: src.through_lid && !via.is_some_and(|i| refl[i].shielded_by_case),
            via,
            arrive,
        }
    };
    let direct = sub(mic, src.pos);
    let mut out = vec![make(dist(src.pos, mic), src.gain, direct, None, unit(sub(src.pos, mic)), None)];
    for (i, r) in refl.iter().enumerate() {
        if r.shielded_by_case && mic_over_case {
            continue;
        }
        let side = |p: [f64; 3]| dot(sub(p, r.origin), r.normal);
        let (ds, dm) = (side(src.pos), side(mic));
        if ds * dm <= 0.0 {
            continue; // not both in front of it
        }
        let n = r.normal;
        let img = [src.pos[0] - 2.0 * ds * n[0], src.pos[1] - 2.0 * ds * n[1], src.pos[2] - 2.0 * ds * n[2]];
        let di = side(img);
        let t = di / (di - dm);
        let hit = [img[0] + t * (mic[0] - img[0]), img[1] + t * (mic[1] - img[1]), img[2] + t * (mic[2] - img[2])];
        if let Extent::Rect { u, u_range, w, w_range } = r.extent {
            let rel = sub(hit, r.origin);
            if !(u_range.0..=u_range.1).contains(&dot(rel, u)) || !(w_range.0..=w_range.1).contains(&dot(rel, w)) {
                continue;
            }
        }
        if refl.iter().enumerate().any(|(j, o)| j != i && (crosses(o, src.pos, hit) || crosses(o, hit, mic))) {
            continue;
        }
        // the departure from the source: the image's ray to the microphone, mirrored back
        let v = sub(mic, img);
        let vn = dot(v, n);
        let dep = [v[0] - 2.0 * vn * n[0], v[1] - 2.0 * vn * n[1], v[2] - 2.0 * vn * n[2]];
        let edge = edge_hz(r, dist(src.pos, hit), dist(hit, mic), dot(unit(sub(hit, src.pos)), n));
        out.push(make(dist(img, mic), src.gain * r.gain, dep, Some(i), unit(sub(hit, mic)), edge));
    }
    out
}

/// The board's directivity (step 3). A plate's bending waves radiate efficiently only
/// above the coincidence frequency, at angles up to sin θ_c = c_air / c_bending(f) from
/// the normal, so the treble leaves the board in a cone around the normal that narrows
/// with frequency. A uniform plate would fill only the cone's rim (one bending
/// wavenumber); a soundboard's ribs, uneven thickness, edges and the near field around
/// the drive point add all smaller wavenumbers, which fills it (as measured on grands:
/// the treble goes up broadly, and the open lid sends it to the audience). Soft edge
/// CONE_EDGE (in sin θ); below coincidence the board moves nearly as a whole and
/// radiates about evenly (blended smoothly). Normalized to the mean over the
/// hemisphere, so the fitted spectrum is kept on average. Off with
/// PIANOENGINE_NO_DIRECTIVITY (checks).
const CONE_EDGE: f64 = 0.15;

struct Directivity {
    /// per frequency bin: sin θ_c, hemisphere mean, blend
    bins: Vec<(f64, f64, f64)>,
    on: bool,
}

fn cone(sin: f64, sin_c: f64) -> f64 {
    1.0 / (1.0 + ((sin - sin_c) / CONE_EDGE).exp())
}

impl Directivity {
    fn new(n: usize, sr: f64) -> Self {
        let on = std::env::var_os("PIANOENGINE_NO_DIRECTIVITY").is_none();
        let f_c = 1000.0 * (SPEED_OF_SOUND / BENDING_SPEED_1K).powi(2);
        let bins = (0..n / 2 + 1)
            .map(|i| {
                let f = (i as f64 * sr / n as f64).max(1.0);
                let sin_c = (SPEED_OF_SOUND / (BENDING_SPEED_1K * (f / 1000.0).sqrt())).min(1.0);
                // hemisphere mean: ∫ cone(sin θ)·sin θ dθ over 0…π/2 (solid-angle weight)
                let steps = 64;
                let mean = (0..steps)
                    .map(|j| {
                        let th = (j as f64 + 0.5) / steps as f64 * std::f64::consts::FRAC_PI_2;
                        cone(th.sin(), sin_c) * th.sin()
                    })
                    .sum::<f64>()
                    * std::f64::consts::FRAC_PI_2
                    / steps as f64;
                let blend = (f / f_c).powi(2) / (1.0 + (f / f_c).powi(2));
                (sin_c, mean, blend)
            })
            .collect();
        Directivity { bins, on }
    }

    /// amplitude gain at bin i for a departure at cos θ
    fn gain(&self, i: usize, cos_theta: f64) -> f64 {
        if !self.on {
            return 1.0;
        }
        let (sin_c, mean, blend) = self.bins[i];
        let sin = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
        ((1.0 - blend) + blend * cone(sin, sin_c) / mean).sqrt()
    }
}

/// Air absorption (dB/m) at 20 °C, ~50 % humidity (fit to ISO 9613-1).
fn air_db_per_m(f: f64) -> f64 {
    0.0055 * (f / 1000.0).powf(1.6)
}

/// Each channel's response sent along its microphone's paths (delays re `first` m).
/// The samples `along_paths` returns for these responses and paths: the longest response, the
/// spread of the arrivals, a margin
fn path_len(irs: &[Vec<f32>], all: &[Vec<Path>], first: f64, sr: f64) -> usize {
    let longest = all.iter().flatten().map(|p| p.length).fold(0.0, f64::max);
    irs.iter().map(Vec::len).max().unwrap_or(0) + ((longest - first) / SPEED_OF_SOUND * sr).ceil() as usize + 64
}

/// What `along_paths` needs per FFT size, made once for every zone, polarization and channel of a
/// setup (made per call, they cost more than the paths: the directivity is 64 cones per bin)
struct PathFft {
    n: usize,
    fwd: std::sync::Arc<dyn realfft::RealToComplex<f64>>,
    inv: std::sync::Arc<dyn realfft::ComplexToReal<f64>>,
    dir: Directivity,
    /// per bin: the air's absorption, dB per meter
    air_db: Vec<f64>,
}

impl PathFft {
    fn new(n: usize, sr: f64) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let (fwd, inv) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
        let air_db = (0..n / 2 + 1).map(|i| air_db_per_m(i as f64 * sr / n as f64)).collect();
        PathFft { n, fwd, inv, dir: Directivity::new(n, sr), air_db }
    }
}

/// The responses (per channel) sent along the paths to that channel's microphone, from the
/// earliest arrival `first` on; `fft` made for `path_len(…).next_power_of_two()`.
fn along_paths(irs: &[Vec<f32>], all: &[Vec<Path>], first: f64, sr: f64, fft: &PathFft) -> Vec<Vec<f32>> {
    let len = path_len(irs, all, first, sr);
    let n = fft.n;
    debug_assert_eq!(n, len.next_power_of_two());
    let mut out = Vec::new();
    for (ch, ir) in irs.iter().enumerate() {
        let mut buf: Vec<f64> = ir.iter().map(|&v| v as f64).collect();
        buf.resize(n, 0.0);
        let mut spec = fft.fwd.make_output_vec();
        fft.fwd.process(&mut buf, &mut spec).unwrap();
        let mut acc = vec![Complex64::new(0.0, 0.0); spec.len()];
        for p in &all[ch.min(all.len() - 1)] {
            let gain = p.gain * REF_DISTANCE / p.length.hypot(ZONE_RADIUS);
            let delay = (p.length - first) / SPEED_OF_SOUND;
            let extra = p.length - REF_DISTANCE; // air beyond what the recording had
            for (i, (a, s)) in acc.iter_mut().zip(&spec).enumerate() {
                let f = i as f64 * sr / n as f64;
                let air = 10f64.powf(-fft.air_db[i] * extra.max(0.0) / 20.0);
                let ph = Complex64::from_polar(1.0, -2.0 * std::f64::consts::PI * f * delay);
                let mut v = s * ph * (gain * air * fft.dir.gain(i, p.cos_theta));
                for fc in p.lowpass.iter().flatten() {
                    v /= Complex64::new(1.0, f / fc); // first-order lowpass
                }
                if let Some(fh) = p.highpass {
                    v *= Complex64::new(0.0, f / fh) / Complex64::new(1.0, f / fh); // first-order highpass
                }
                *a += v;
            }
        }
        acc[0].im = 0.0;
        if let Some(last) = acc.last_mut() {
            last.im = 0.0;
        }
        let mut y = fft.inv.make_output_vec();
        fft.inv.process(&mut acc, &mut y).unwrap();
        y.truncate(len);
        let y: Vec<f32> = y.iter().map(|v| (v / n as f64) as f32).collect();
        out.push(y);
    }
    out
}

// ---------------------------------------------------------------- step 2: the bridge

/// Where a note drives the soundboard (x, y; m): the bass bridge from A0 (back left)
/// to C2, the treble bridge from C#2 (left, behind the center) to C8 (front right).
pub fn bridge_point(note: f64) -> (f64, f64) {
    let lerp = |a: (f64, f64), b: (f64, f64), t: f64| (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
    if note < 36.5 {
        lerp((-0.48, 0.70), (-0.25, 0.30), ((note - 21.0) / 15.5).clamp(0.0, 1.0))
    } else {
        lerp((-0.30, 0.40), (0.42, -0.62), ((note - 37.0) / 71.0).clamp(0.0, 1.0))
    }
}

/// Bridge zones (their notes); a note mixes its two neighboring zones.
pub const ZONE_NOTES: [f64; 8] = [24.0, 32.0, 40.0, 52.0, 64.0, 76.0, 90.0, 104.0];

impl Body {
    /// The keys the body's geometry lays out (the bars on their frame, a harp's or a zither's
    /// bridge, a guitar's strings); none for the pianos, whose bridges hold the keyboard's 88.
    pub fn own_range(self) -> Option<(f64, f64)> {
        match self {
            Body::Grand | Body::Upright => None,
            Body::Harp | Body::LeverHarp => Some(self.harp_dims().notes),
            bar_bodies!() => Some(self.frame().notes),
            Body::Guitar | Body::Strumstick => Some(self.guitar_dims().notes),
            Body::Psaltery | Body::DanTranh => Some(self.zither_dims().notes),
        }
    }

    /// A note on the zones' scale (`zone_weights`): a body with a range of its own spreads it over
    /// the zones (at the piano's notes the crotales' C6–C8 had six of the eight zones piled at the
    /// frame's bass end and sounded from the left, the psaltery's A♯4–F♯6 two inside it), or with
    /// `all_keys` the keyboard's 21…108 (the keys beyond the range then sound along it too); the
    /// pianos as played.
    pub fn zone_key(self, note: f64, all_keys: bool) -> f64 {
        let Some(own) = self.own_range() else { return note };
        let (lo, hi) = if all_keys { (21.0, 108.0) } else { own };
        let (z0, z1) = (ZONE_NOTES[0], ZONE_NOTES[ZONE_NOTES.len() - 1]);
        z0 + (note - lo) / (hi - lo) * (z1 - z0)
    }

    /// (zone a, weight a, zone b, weight b) of a note (`zone_weights` on `zone_key`). A bar sounds
    /// from its nearest zone alone: its few modes, mixed from two zones at two delays per
    /// microphone, cancelled in one channel and not the other (the marimba's sides 20 dB apart
    /// from key to key at the close positions).
    pub fn zone_weights(self, note: f64, all_keys: bool) -> (usize, f64, usize, f64) {
        let (a, wa, b, wb) = zone_weights(self.zone_key(note, all_keys));
        if self.is_bars() {
            let z = if wb > wa { b } else { a };
            return (z, 1.0, z, 0.0);
        }
        (a, wa, b, wb)
    }

    /// The note zone `i` sounds from (`zone_key`'s inverse).
    pub fn zone_note(self, i: usize) -> f64 {
        let Some((lo, hi)) = self.own_range() else { return ZONE_NOTES[i] };
        let (z0, z1) = (ZONE_NOTES[0], ZONE_NOTES[ZONE_NOTES.len() - 1]);
        lo + (ZONE_NOTES[i] - z0) / (z1 - z0) * (hi - lo)
    }
}

/// (zone a, weight a, zone b, weight b) of a note (amplitude weights, a + b = 1).
pub fn zone_weights(note: f64) -> (usize, f64, usize, f64) {
    let z = &ZONE_NOTES;
    if note <= z[0] {
        return (0, 1.0, 0, 0.0);
    }
    if note >= z[z.len() - 1] {
        return (z.len() - 1, 1.0, z.len() - 1, 0.0);
    }
    let i = z.partition_point(|&n| n <= note) - 1;
    let t = (note - z[i]) / (z[i + 1] - z[i]);
    (i, 1.0 - t, i + 1, t)
}

/// bending-wave speed of the soundboard at 1 kHz (m/s; ∝ √f): wavelength ~25 cm there
const BENDING_SPEED_1K: f64 = 250.0;
/// plane waves per mode in the random-wave mode shapes
const WAVES: usize = 24;

/// A board mode's shape at point p: a random superposition of plane waves with the
/// mode's bending wavenumber (statistics of a plate's modes, Berry's random waves),
/// variance 1, fixed per mode.
fn mode_shape(m: usize, f: f64, p: (f64, f64)) -> f64 {
    let k = 2.0 * std::f64::consts::PI * f / (BENDING_SPEED_1K * (f / 1000.0).sqrt());
    let mut rng = crate::noise::Rng::new(0x5eed_b0a2d ^ (m as u64).wrapping_mul(0x9e37_79b9));
    let mut sum = 0.0;
    for _ in 0..WAVES {
        let th = 2.0 * std::f64::consts::PI * rng.uniform01();
        let ps = 2.0 * std::f64::consts::PI * rng.uniform01();
        sum += (k * (p.0 * th.cos() + p.1 * th.sin()) + ps).cos();
    }
    sum * (2.0 / WAVES as f64).sqrt()
}

/// The soundboard's radiated response (as `proto/board.py` `impulse_response`) for
/// excitation weights `phi` per mode: rows (channel responses) for every row's
/// radiation factors `r_rows`, each run of `group` rows at unit mean energy (a pair's
/// channels keep their balance: one at unit energy each lifted a channel's shared bass by
/// the other's mids, step 187); modes below 0.45·sr_ref, summed at flat
/// weights, then the transfer H as one minimum-phase filter (`noise::body_filter`: with H as
/// the modes' weights their tails filled the bass 12–22 dB over H where H spans 60 dB).
fn synth_body(board: &crate::physics::Board, h: &(Vec<f64>, Vec<f64>), rows: &[(Vec<f64>, &[f64])], group: usize, sr: f64, sr_ref: f64, len: usize) -> Vec<Vec<f64>> {
    // Σ over the modes of a gain per row × e^{−σt}·cos(ωt). The threads take spans of the time axis
    // (every mode computed once: split by rows, each thread made all of them again, a serial chain of
    // complex products, 0.4 s a build); within a span the tiles keep the rows' samples in the cache.
    // Each mode starts a tile at its exact value e^{t·ln z} and runs the recursion within it.
    const TILE: usize = 512;
    use realfft::num_complex::Complex64;
    let keep: Vec<usize> = (0..board.f.len()).filter(|&m| board.f[m] < 0.45 * sr_ref).collect();
    let ln_z: Vec<Complex64> = keep.iter().map(|&m| Complex64::new(-board.sigma[m] / sr, 2.0 * std::f64::consts::PI * board.f[m] / sr)).collect();
    let z: Vec<Complex64> = ln_z.iter().map(|l| l.exp()).collect();
    let gains: Vec<Vec<f64>> = rows.iter().map(|(phi, r)| keep.iter().map(|&m| phi[m] * board.sigma[m].sqrt() * r[m]).collect()).collect();
    let mut out = vec![vec![0.0; len]; rows.len()];
    let threads = crate::threads::build_threads();
    let span = len.div_ceil(threads).div_ceil(TILE).max(1) * TILE;
    // per thread: its span of every row
    let mut spans: Vec<Vec<&mut [f64]>> = (0..len.div_ceil(span)).map(|_| Vec::with_capacity(rows.len())).collect();
    for row in out.iter_mut() {
        for (k, part) in row.chunks_mut(span).enumerate() {
            spans[k].push(part);
        }
    }
    std::thread::scope(|scope| {
        for (k, mut parts) in spans.into_iter().enumerate() {
            let (ln_z, z, gains) = (&ln_z, &z, &gains);
            scope.spawn(move || {
                let n_span = parts.first().map_or(0, |p| p.len());
                let mut osc = [0.0; TILE];
                for t in (0..n_span).step_by(TILE) {
                    let n = TILE.min(n_span - t);
                    let t0 = (k * span + t) as f64;
                    for i in 0..z.len() {
                        let mut c = (ln_z[i] * t0).exp();
                        for o in osc[..n].iter_mut() {
                            *o = c.re;
                            c *= z[i];
                        }
                        for (part, g) in parts.iter_mut().zip(gains) {
                            let g = g[i];
                            if g != 0.0 {
                                part[t..t + n].iter_mut().zip(&osc[..n]).for_each(|(y, &o)| *y += g * o);
                            }
                        }
                    }
                }
            });
        }
    });
    crate::noise::body_filter(&h.0, &h.1, sr, &mut out);
    for rows in out.chunks_mut(group.max(1)) {
        let e = (rows.iter().flatten().map(|v| v * v).sum::<f64>() / rows.len() as f64).sqrt().max(1e-300);
        rows.iter_mut().flatten().for_each(|v| *v /= e);
    }
    out
}

/// Radiation per bridge zone for a microphone position (step 2), at rate `sr`.
pub struct Zones {
    /// [zone][polarization 0 v / 1 h] → channel responses (L, R)
    pub body: Vec<[Vec<Vec<f32>>; 2]>,
    /// factor on the room part (total level as at the recording's position)
    pub room_gain: f64,
}

/// Per zone: the soundboard's response for an excitation at the zone's bridge point,
/// sent along the paths from that point to the microphones. `ref_body`: the fitted
/// responses at rate sr (vertical, horizontal; their per-channel energy is kept), and
/// `room_energy` per channel (vertical) for the level match.
/// `setup`: the microphones and the wall (a preset's via `Body::preset_setup`).
#[allow(clippy::too_many_arguments)]
pub fn zones(
    body: Body,
    board: &crate::physics::Board,
    h: &(Vec<f64>, Vec<f64>),
    ref_body: [&[Vec<f32>]; 2],
    room_energy: &[f64],
    sr: f64,
    sr_ref: f64,
    setup: &MicSetup,
    keep_direct: bool,
) -> Zones {
    let mics = [setup.mics[0].pos, setup.mics[1].pos];
    let len = ref_body[0].iter().map(Vec::len).max().unwrap_or(0);
    // (the bars: every zone with the fitted excitation, the coloring the export compensates the
    // modes' levels for; a board's random shapes per zone put the keys' modes ±4 dB off at random)
    let shapes: Vec<Vec<f64>> = (0..ZONE_NOTES.len())
        .map(|i| {
            if body.is_bars() {
                return board.phi.clone();
            }
            let p = body.bridge_point(body.zone_note(i));
            (0..board.f.len()).map(|m| mode_shape(m, board.f[m], p)).collect()
        })
        .collect();
    // radiation factors of the two microphones: as fitted for the left one; the right
    // one coherent with it at low frequencies, by the spacing (two nearby microphones
    // see a board mode alike; the fitted, independent ones stand for a wide pair). Bars
    // radiate directly, each a small source with a few modes: both see the stand-in
    // board alike (independent, a key's few modes fell on random gains per channel, its
    // two sides ±10 dB apart from key to key at the close positions)
    // The fitted right one leans on the left below 400 Hz (`proto/board.py` `coherent_low`): signs
    // alternating along the modes in frequency order make it independent again at its size, every
    // band's energy kept (correlated, the mix summed to 1.4× in the bass, +3 dB on the right channel
    // at Player; signs drawn at random tipped a band of a few modes to one side; step 187).
    let d = dist(mics[0], mics[1]);
    let coherent = body.is_bars();
    let mut order: Vec<usize> = (0..board.f.len()).collect();
    order.sort_by(|&a, &b| board.f[a].total_cmp(&board.f[b]));
    let mut sign = vec![1.0; board.f.len()];
    for (i, &m) in order.iter().enumerate() {
        sign[m] = if i % 2 == 0 { 1.0 } else { -1.0 };
    }
    let pair = |r: &[Vec<f64>; 2]| -> [Vec<f64>; 2] {
        let right = (0..board.f.len())
            .map(|m| {
                let x = 2.0 * board.f[m] * d / SPEED_OF_SOUND;
                let rho = if x == 0.0 || coherent { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
                rho * r[0][m] + (1.0 - rho * rho).max(0.0).sqrt() * sign[m] * r[1][m]
            })
            .collect();
        [r[0].clone(), right]
    };
    let (rv, rh) = (pair(&board.r), pair(&board.r_h));
    let mut rows: Vec<(Vec<f64>, &[f64])> = Vec::new();
    for shape in &shapes {
        for r in [&rv, &rh] {
            for c in r {
                rows.push((shape.clone(), c.as_slice()));
            }
        }
    }
    let synth = synth_body(board, h, &rows, 2, sr, sr_ref, len);
    let e = |c: &[f32]| c.iter().map(|&v| (v as f64).powi(2)).sum::<f64>();
    // all zones' paths, the earliest arrival over everything at time 0
    let refl = body.reflectors(setup.wall, setup.lid);
    let zone_paths: Vec<Vec<Vec<Path>>> = (0..ZONE_NOTES.len())
        .map(|i| {
            let sources = body.sources(body.zone_note(i));
            mics.iter()
                .map(|&m| sources.iter().flat_map(|s| paths(s, m, &refl, body.over_case(m))).collect())
                .collect()
        })
        .collect();
    // patterns weight each path by the microphone's sensitivity to its arrival direction. The level
    // match (`k`) and time zero stay those of the omni sum, so patterns change levels like a real
    // microphone does and aiming one never shifts the timing.
    let leak = lid_leak(setup.lid);
    let patterned = setup.mics.iter().any(|m| m.pattern > 0.0);
    let rendered = patterned || (leak.is_some() && setup.mics.iter().any(|m| body.lid_shields(m.pos)));
    // patterns and the closed lid's loss scale gains only: the level match and time zero come from the plain sum
    let render_paths: Vec<Vec<Vec<Path>>> = if rendered {
        zone_paths
            .iter()
            .map(|zp| {
                zp.iter()
                    .zip(&setup.mics)
                    .map(|(ps, m)| {
                        let shielded = body.lid_shields(m.pos);
                        ps.iter()
                            .map(|p| {
                                let mut q = Path { gain: p.gain * m.gain(p.arrive), ..*p };
                                if let (true, true, Some((g, hz))) = (shielded, p.lid_path, leak) {
                                    q.gain *= g;
                                    q.lowpass[2] = Some(hz);
                                }
                                q
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };
    let first = zone_paths.iter().flatten().flatten().map(|p| p.length).fold(f64::INFINITY, f64::min);
    // the FFT sizes the zones need (the rendered paths have the omni ones' lengths), each made once
    let ir_len = len;
    let size = |zp: &[Vec<Path>]| {
        let l = zp.iter().flatten().map(|p| p.length).fold(0.0, f64::max);
        (ir_len + ((l - first) / SPEED_OF_SOUND * sr).ceil() as usize + 64).next_power_of_two()
    };
    let mut ffts: Vec<PathFft> = Vec::new();
    for zp in &zone_paths {
        let n = size(zp);
        if !ffts.iter().any(|f| f.n == n) {
            ffts.push(PathFft::new(n, sr));
        }
    }
    // the zones in parallel; their energies summed in zone order afterwards (the same bits as one after another)
    let per_zone: Vec<([Vec<Vec<f32>>; 2], f64, f64)> = std::thread::scope(|scope| {
        let handles: Vec<_> = zone_paths
            .iter()
            .enumerate()
            .map(|(z, zp)| {
                let (synth, ffts, render_paths) = (&synth, &ffts, &render_paths);
                scope.spawn(move || {
                    let fft = ffts.iter().find(|f| f.n == size(zp)).expect("made above");
                    let mut pols: [Vec<Vec<f32>>; 2] = [Vec::new(), Vec::new()];
                    let (mut e_new, mut e_body) = (0.0, 0.0);
                    for (pol, out) in pols.iter_mut().enumerate() {
                        // both channels at the fitted channels' mean level: the recording's balance
                        // between its two microphones belongs to where they stood (the right one 3 dB
                        // up lifted the whole bass of the right channel, which both share; step 187)
                        let k = (ref_body[pol].iter().map(|c| e(c)).sum::<f64>() / ref_body[pol].len().max(1) as f64).sqrt();
                        let irs: Vec<Vec<f32>> = (0..2)
                            .map(|c| {
                                synth[(z * 2 + pol) * 2 + c].iter().map(|&v| (v * k) as f32).collect()
                            })
                            .collect();
                        let omni = along_paths(&irs, zp, first, sr, fft);
                        if pol == 0 {
                            e_new = omni.iter().zip(room_energy).map(|(c, r)| e(c) + r).sum::<f64>();
                            e_body = omni.iter().map(|c| e(c)).sum::<f64>();
                        }
                        *out = if rendered { along_paths(&irs, &render_paths[z], first, sr, fft) } else { omni };
                    }
                    (pols, e_new, e_body)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a zone's thread")).collect()
    });
    let mut body = Vec::new();
    let (mut e_new, mut e_body) = (0.0, 0.0);
    for (pols, en, eb) in per_zone {
        e_new += en;
        e_body += eb;
        body.push(pols);
    }
    // level: the total (direct + room) as at the recording's position, or the direct
    // sound alone (the room then grows with the distance)
    let nz = ZONE_NOTES.len() as f64;
    let k = if keep_direct {
        let e_ref: f64 = ref_body[0].iter().map(|c| e(c)).sum();
        (e_ref / (e_body / nz)).sqrt()
    } else {
        let e_old: f64 = ref_body[0].iter().zip(room_energy).map(|(c, r)| e(c) + r).sum();
        (e_old / (e_new / nz)).sqrt()
    };
    for pols in &mut body {
        for ch in pols.iter_mut().flatten() {
            ch.iter_mut().for_each(|v| *v *= k as f32);
        }
    }
    Zones { body, room_gain: k }
}

/// The synthesis reproduces the fitted response for the original excitation (check).
pub fn synth_reference(board: &crate::physics::Board, h: &(Vec<f64>, Vec<f64>), sr: f64, len: usize) -> Vec<Vec<f64>> {
    // the measured modes radiate at unit weight (`proto/board.py` `rad`): their φ acts on the coupling alone
    let rad: Vec<f64> = board.phi.iter().enumerate().map(|(m, &p)| if m < board.measured_from { p } else { 1.0 }).collect();
    let rows: Vec<(Vec<f64>, &[f64])> = board.r.iter().map(|c| (rad.clone(), c.as_slice())).collect();
    synth_body(board, h, &rows, 1, sr, sr, len)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lid_room_is_a_no_op_when_open_and_robust() {
        let x: Vec<f32> = (0..64).map(|i| ((i * 5) % 11) as f32 - 5.0).collect();
        for lid in [1.0, 1.5, f64::NAN, f64::INFINITY] {
            let mut y = x.clone();
            lid_room(&mut y, lid, 48000.0);
            assert_eq!(y, x, "lid {lid}");
        }
        assert!(lid_leak(f64::NAN).is_none());
    }

    #[test]
    fn lid_room_shut_gains() {
        let (g, _) = lid_leak(0.0).unwrap();
        let s = LID_ROOM_SHARE;
        let mut dc = vec![1.0f32; 48000];
        lid_room(&mut dc, 0.0, 48000.0);
        assert!((dc[47999] as f64 - ((1.0 - s) + s * g)).abs() < 1e-4, "dc {}", dc[47999]);
        let mut hf: Vec<f32> = (0..48000).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        lid_room(&mut hf, 0.0, 48000.0);
        assert!((hf[47999].abs() as f64 - (1.0 - s)).abs() < 0.02, "hf {}", hf[47999]);
    }

    #[test]
    fn lid_room_is_monotone_in_the_lid() {
        // more closed: less energy on white noise and less gain at the highest frequency (the ratio to DC is not monotone: g also scales the filtered part)
        let mut r = 12345u64;
        let x: Vec<f32> = (0..48000)
            .map(|_| {
                r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (r >> 40) as f32 / (1u64 << 23) as f32 - 1.0
            })
            .collect();
        let stat = |lid: f64| {
            let mut y = x.clone();
            lid_room(&mut y, lid, 48000.0);
            let en: f64 = y.iter().map(|&v| (v as f64).powi(2)).sum();
            let mut dc = vec![1.0f32; 4800];
            lid_room(&mut dc, lid, 48000.0);
            let mut ny: Vec<f32> = (0..4800).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
            lid_room(&mut ny, lid, 48000.0);
            (en, ny[4799].abs() as f64)
        };
        let mut last = stat(1.0);
        for lid in [0.75, 0.5, 0.25, 0.0] {
            let c = stat(lid);
            assert!(c.0 < last.0 && c.1 < last.1, "lid {lid}: {c:?} vs {last:?}");
            last = c;
        }
    }

    #[test]
    fn lid_leak_curve() {
        assert_eq!(lid_leak(1.0), None);
        assert_eq!(lid_leak(2.0), None, "more than open is open");
        let (g, hz) = lid_leak(0.0).unwrap();
        assert!((20.0 * g.log10() - LID_LEAK_DB).abs() < 1e-9 && (hz - LID_LEAK_HZ).abs() < 1e-6);
        // monotone: more closed = quieter and darker, starting from the open (no) filter
        let mut last = (1.0, 20000.0);
        for lid in [0.9, 0.75, 0.5, 0.25, 0.0] {
            let (g, hz) = lid_leak(lid).unwrap();
            assert!(g < last.0 && hz < last.1, "lid {lid}");
            last = (g, hz);
        }
    }

    #[test]
    fn sources_that_leak_through_the_lid() {
        let g = Body::Grand.sources(60.0);
        assert!(g.iter().all(|s| s.through_lid), "the grand's top face");
        let u = Body::Upright.sources(60.0);
        assert_eq!(u.iter().map(|s| s.through_lid).collect::<Vec<_>>(), vec![false, true, false], "only the open-top source");
        // the grand: outside the case the lid is in the way; under it (a close pair) it is not; the upright: always
        assert!(Body::Grand.lid_shields([2.0, -1.0, 0.5]) && !Body::Grand.lid_shields([0.0, 0.0, 0.2]));
        assert!(Body::Upright.lid_shields([0.0, -0.15, 0.9]) && Body::Upright.lid_shields([0.0, -1.0, 0.5]));
    }

    #[test]
    fn floor_paths_do_not_leak() {
        // a path via the floor comes from the underside; the direct path leaks
        let src = Body::Grand.sources(60.0)[0];
        let mic = [1.5, -1.5, 0.6];
        let refl = Body::Grand.reflectors(U_WALL, 1.0);
        let p = paths(&src, mic, &refl, false);
        assert!(p[0].lid_path, "direct");
        let floor = p.iter().find(|q| q.via == Some(GRAND_FLOOR)).unwrap();
        assert!(!floor.lid_path, "floor");
    }

    /// body energy of a grand pair at lid `lid`
    fn lid_energy(body: Body, mics: [Mic; 2], lid: f64) -> f64 {
        let board = test_board();
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
        let z = zones(body, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &MicSetup { mics, wall: U_WALL, lid }, false);
        assert!(z.body.iter().flatten().flatten().flatten().all(|v| v.is_finite()));
        z.body.iter().flatten().flatten().flatten().map(|&v| (v as f64).powi(2)).sum()
    }

    #[test]
    fn closing_the_lid_makes_outside_microphones_quieter() {
        // an Audience-like pair outside the case, a Hammers-like pair under the lid
        let outside = [Mic::toward_board([2.5, -0.3, 0.45]), Mic::toward_board([2.5, 0.2, 0.45])];
        let under = [Mic::toward_board([-0.2, -0.55, 0.08]), Mic::toward_board([0.2, -0.55, 0.08])]; // 8 cm above the board: under the lid even when it is shut
        let e = |m, lid| lid_energy(Body::Grand, m, lid);
        let (open, half, shut) = (e(outside, 1.0), e(outside, 0.5), e(outside, 0.0));
        assert!(shut < half && half < open, "{shut} {half} {open}");
        assert!(shut < open * 0.5, "closed is clearly quieter: {shut} vs {open}");
        // the pair under the lid hears no leak: only the reflection geometry changes it, far less than the outside pair
        let (uo, us) = (e(under, 1.0), e(under, 0.0));
        assert!((us / uo - 1.0).abs() < (shut / open - 1.0).abs(), "under {uo} {us}");
    }

    #[test]
    fn the_uprights_open_top_pair_hears_the_closed_lid() {
        let top = [Mic::toward_board([-0.2, -0.15, 0.9]), Mic::toward_board([0.2, -0.15, 0.9])];
        let (open, shut) = (lid_energy(Body::Upright, top, 1.0), lid_energy(Body::Upright, top, 0.0));
        assert!(shut < open * 0.7, "{shut} {open}");
    }


    fn all_paths(body: Body, pos: MicPosition, mic: [f64; 3]) -> Vec<Path> {
        let refl = body.reflectors(body.wall_of(pos), 1.0);
        body.sources(60.0).iter().flat_map(|s| paths(s, mic, &refl, body.over_case(mic))).collect()
    }

    #[test]
    fn lid_one_is_the_old_geometry() {
        // the lid angle at lid = 1 is the constants themselves: heights and reflectors as before
        for x in [-0.7, 0.0, 0.5] {
            assert_eq!(lid_height(x, 1.0), LID_HINGE.1 + (x - LID_HINGE.0) * LID_ANGLE_DEG.to_radians().tan());
        }
        let r = Body::Grand.reflectors(U_WALL, 1.0);
        let a = LID_ANGLE_DEG.to_radians();
        assert_eq!(r[GRAND_LID].normal, [-a.sin(), 0.0, a.cos()]);
    }

    #[test]
    fn closed_lid_geometry() {
        // grand: flat at the hinge height; the lid height falls monotonically as it closes
        for x in [-0.7, 0.0, 0.5] {
            assert!((lid_height(x, 0.0) - LID_HINGE.1).abs() < 1e-12);
            assert!(lid_height(x, 0.5) <= lid_height(x, 1.0) + 1e-12 && lid_height(x, 0.5) >= lid_height(x, 0.0) - 1e-12);
        }
        let g = Body::Grand.reflectors(U_WALL, 0.0);
        assert_eq!(g[GRAND_LID].normal, [0.0, 0.0, 1.0]);
        // upright: the top lid lies flat on the case top, towards the front
        let u = Body::Upright.reflectors(U_WALL, 0.0);
        assert!((u[U_LID].normal[2] - 1.0).abs() < 1e-12 && u[U_LID].normal[1].abs() < 1e-12);
        let Extent::Rect { u: dir, .. } = u[U_LID].extent else { panic!("a rectangle") };
        assert!((dir[1] + 1.0).abs() < 1e-12 && dir[2].abs() < 1e-12);
        // the drawing follows the angle
        let d = Body::Grand.drawing(U_WALL, 0.0);
        assert!((d.lid_front[1][1] - LID_HINGE.1).abs() < 1e-12);
    }

    #[test]
    fn presets_and_clamp_hold_at_every_lid() {
        for lid in [0.0, 0.25, 0.5, 0.53, 0.55, 0.75, 1.0] {
            for (body, pos, spacing, distance) in golden_cases() {
                let s = body.preset_setup(pos, spacing, distance, lid).unwrap();
                assert_eq!(s.lid, lid);
                for m in s.mics {
                    assert_eq!(body.clamp_mic(s.wall, lid, m.pos), m.pos, "{body:?} {pos:?} lid {lid}");
                    assert!(body != Body::Grand || !body.over_case(m.pos) || m.pos[2] >= CLOSE_MIN_HEIGHT);
                }
                assert_eq!(body.clamp_setup(s), s, "{body:?} {pos:?} lid {lid}");
            }
        }
        // closed: the close pairs sit at the minimum height above the board
        for pos in [MicPosition::Hammers, MicPosition::Tail] {
            for m in Body::Grand.preset_setup(pos, 0.5, 0.0, 0.0).unwrap().mics {
                assert!((m.pos[2] - CLOSE_MIN_HEIGHT).abs() < 1e-12, "{pos:?} {:?}", m.pos);
            }
        }
    }

    #[test]
    fn upright_clamp_keeps_mics_above_the_closing_lid() {
        let u = Body::Upright;
        // a deterministic scatter over the case and its surroundings
        let mut seed = 12345u64;
        let mut rnd = |lo: f64, hi: f64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            lo + (hi - lo) * ((seed >> 11) as f64 / (1u64 << 53) as f64)
        };
        for lid in [0.0, 0.25, 0.5, 0.53, 0.55, 0.75, 0.94, 1.0] {
            for _ in 0..2000 {
                let p = [rnd(-1.0, 1.0), rnd(-0.8, 0.4), rnd(-0.5, 1.6)];
                let q = u.clamp_mic(U_WALL, lid, p);
                assert_eq!(u.clamp_mic(U_WALL, lid, q), q, "idempotent {p:?} lid {lid}");
                if let Some(z) = u_lid_surface(q[1], lid).filter(|_| (U_CASE_X.0..=U_CASE_X.1).contains(&q[0])) {
                    assert!(q[2] >= z + U_TOP_CLEARANCE - 1e-12, "{p:?} -> {q:?} under the lid ({z}) at {lid}");
                }
            }
        }
    }

    #[test]
    fn upright_hammers_pair_stays_above_the_lid_in_a_sweep() {
        let u = Body::Upright;
        for i in 0..=100 {
            let lid = i as f64 / 100.0;
            let p = u.placement(MicPosition::Hammers, 0.5, 0.0, lid).unwrap();
            for m in p.mics {
                if let Some(z) = u_lid_surface(m[1], lid) {
                    assert!(m[2] >= z + U_TOP_CLEARANCE - 1e-12, "lid {lid}: {m:?} under the lid surface {z}");
                }
                // the pair sits on the open-lid side of the lid plane whenever the plane is under it
                let r = &u.reflectors(U_WALL, lid)[U_LID];
                if u_lid_surface(m[1], lid).is_some() {
                    assert!(dot(sub(m, r.origin), r.normal) > 0.0, "lid {lid}: {m:?} below the lid plane");
                }
            }
            assert!((p.mics[0][2] - U_CASE_TOP - p.distance).abs() < 1e-12);
        }
        // the full opening: the requested height as before
        assert_eq!(u.placement(MicPosition::Hammers, 0.5, 0.0, 1.0).unwrap().distance, 0.3);
        // the 0.53 marker/pair above the lid line at its y
        let p = u.placement(MicPosition::Hammers, 0.5, 0.0, 0.53).unwrap();
        assert!(p.mics[0][2] > u_lid_surface(U_HAMMER_Y, 0.53).unwrap());
        let d = u.drawing(U_WALL, 0.53);
        assert!(d.markers[3][1] == U_HAMMER_Y);
    }

    #[test]
    fn clamp_setup_sanitizes_the_lid() {
        let mut s = MicSetup::default();
        // not finite: open; below closed: closed; above open: taken off
        for (bad, want) in [(f64::NAN, 1.0), (f64::INFINITY, 1.0), (-3.0, 0.0), (7.0, LID_OFF)] {
            s.lid = bad;
            let c = Body::Grand.clamp_setup(s);
            assert_eq!(c.lid, want, "{bad}");
            assert_eq!(Body::Grand.clamp_setup(c), c);
        }
        s.lid = f64::NAN;
        assert_eq!(Body::Upright.clamp_setup(s).lid, 1.0);
    }

    #[test]
    fn lid_reflects_towards_the_audience_not_the_player() {
        // a source at the board's center, as the old test had it
        let src = Source { pos: [0.0; 3], normal: [0.0, 0.0, 1.0], gain: 1.0, lowpass_hz: None, one_sided: false, through_lid: false };
        let refl = Body::Grand.reflectors(Body::Grand.wall_of(MicPosition::Audience), 1.0);
        let lid = |m: [f64; 3]| paths(&src, m, &refl, Body::Grand.over_case(m)).iter().any(|p| p.via == Some(GRAND_LID));
        assert!(lid(Body::Grand.mics(MicPosition::Audience, 0.5, 0.0, 1.0).unwrap()[0]));
        assert!(!lid(Body::Grand.mics(MicPosition::Player, 0.16, 0.0, 1.0).unwrap()[0]));
    }

    #[test]
    fn close_pairs_stay_under_the_lid_without_the_floor() {
        for pos in [MicPosition::Hammers, MicPosition::Tail] {
            for (spacing, height) in [(0.5, 0.0), (10.0, 5.0), (0.0, 0.01)] {
                for m in Body::Grand.mics(pos, spacing, height, 1.0).unwrap() {
                    assert!(Body::Grand.over_case(m) && m[2] >= CLOSE_MIN_HEIGHT && m[2] < lid_height(m[0], 1.0), "{pos:?} {m:?}");
                    let p = all_paths(Body::Grand, pos, m);
                    assert!(p.iter().all(|p| p.via != Some(GRAND_FLOOR)), "floor reflection at {pos:?} {m:?}");
                    if spacing == 0.5 {
                        assert!(p.iter().any(|p| p.via == Some(GRAND_LID)), "no lid reflection at {pos:?} {m:?}");
                    }
                }
            }
        }
    }

    fn source_paths(body: Body, pos: MicPosition, src: &Source, mic: [f64; 3]) -> Vec<Path> {
        paths(src, mic, &body.reflectors(body.wall_of(pos), 1.0), body.over_case(mic))
    }

    #[test]
    fn upright_back_pair_hears_the_back_face_and_the_wall() {
        let pos = MicPosition::Tail;
        let back = Body::Upright.sources(60.0)[0];
        for m in Body::Upright.mics(pos, 0.5, 0.0, 1.0).unwrap() {
            let p = source_paths(Body::Upright, pos, &back, m);
            assert!(p[0].via.is_none() && p[0].lowpass[1].is_none() && p[0].gain == 1.0, "direct {m:?}");
            // ribs and back posts on the board's back face
            assert_eq!(p[0].lowpass[0], Some(U_BACK_LOWPASS_HZ));
            assert!(p.iter().any(|p| p.via == Some(U_WALL_REFL) && p.lowpass[1].is_none()), "wall {m:?}");
        }
    }

    #[test]
    fn harp_player_hears_the_sound_holes_directly_and_the_board_diffracted() {
        let pos = MicPosition::Player;
        let s = Body::Harp.sources(60.0);
        let (board, holes) = (s[0], s[1]);
        for m in Body::Harp.mics(pos, 0.18, 0.0, 1.0).unwrap() {
            let h = source_paths(Body::Harp, pos, &holes, m);
            assert!(h[0].via.is_none() && h[0].lowpass[1].is_none(), "holes direct {m:?}");
            assert_eq!(h[0].lowpass[0], Some(H_HOLE_LOWPASS_HZ));
            let b = source_paths(Body::Harp, pos, &board, m);
            assert_eq!(b[0].lowpass[1], Some(DIFFRACTION_LOWPASS_HZ), "board direct not diffracted {m:?}");
        }
        // the audience and the soundboard pair: the board directly, the holes around the box
        for pos in [MicPosition::Audience, MicPosition::Hammers] {
            for m in Body::Harp.mics(pos, 0.5, 0.0, 1.0).unwrap() {
                assert!(source_paths(Body::Harp, pos, &board, m)[0].lowpass == [None; 3], "{pos:?} {m:?}");
                assert_eq!(source_paths(Body::Harp, pos, &holes, m)[0].lowpass[1], Some(DIFFRACTION_LOWPASS_HZ), "{pos:?}");
            }
        }
    }

    #[test]
    fn harp_has_no_lid_and_only_the_floor() {
        assert_eq!(Body::Harp.reflectors(U_WALL, 0.0).len(), 1);
        assert!(!Body::Harp.lid_shields([0.0, 3.0, 0.4]));
        for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
            assert_eq!(Body::Harp.preset_setup(pos, 0.5, 0.0, 0.0), Body::Harp.preset_setup(pos, 0.5, 0.0, 1.0), "{pos:?}");
            assert_eq!(zones_summary(Body::Harp, pos, 0.5, 0.0), zones_summary(Body::Harp, pos, 0.5, 0.0));
        }
        let free = Body::Harp.clamp_setup(MicSetup { lid: 0.2, ..MicSetup::default() });
        assert_eq!(free.lid, 1.0);
    }

    #[test]
    fn bars_run_bass_left_naturals_in_front() {
        for b in [Body::Vibraphone, Body::Marimba, Body::Xylophone, Body::Bells, Body::Crotales, Body::Chimes, Body::HandChimes, Body::Kalimba, Body::Balafon] {
            let fr = b.frame();
            let (lo, hi) = (fr.notes.0, fr.notes.1);
            let (first, last) = (b.bridge_point(lo), b.bridge_point(hi));
            assert_eq!((first.0, last.0), fr.bar_x);
            // the first accidental behind, the natural after it in front (F♯ and G, C♯ and D)
            let (acc, nat) = (lo + 1.0, lo + 2.0);
            assert!(b.bridge_point(acc).1 > 0.0 && b.bridge_point(nat).1 < 0.0, "{b:?}");
            assert!(b.bridge_point(acc).0 < b.bridge_point(nat).0);
            // keys beyond the range: just past the frame's ends, in order
            assert!(b.bridge_point(21.0).0 < first.0 && b.bridge_point(21.0).0 > -fr.half_width - 0.1);
            assert_eq!(b.bridge_point(lo - 3.0), b.bridge_point(lo - 10.0));
            // (the xylophone's bars reach C8)
            assert!(hi >= 108.0 || b.bridge_point(108.0).0 > last.0 && b.bridge_point(108.0).0 < fr.half_width + 0.1);
            assert!(!b.has_lid() && b.is_bars());
            assert_eq!(b.reflectors(U_WALL, 0.0).len(), 1);
            assert!(!b.lid_shields([0.0, 3.0, 0.4]) && !b.over_case([0.0, 0.0, 0.3]));
        }
        assert!(Body::Grand.has_lid() && !Body::Harp.has_lid() && !Body::Harp.is_bars());
        assert!(MARIMBA.bar_x.1 - MARIMBA.bar_x.0 > 1.5 * (VIBRAPHONE.bar_x.1 - VIBRAPHONE.bar_x.0));
    }

    #[test]
    fn bar_zones_spread_over_the_frame_one_per_key() {
        for b in [Body::Vibraphone, Body::Marimba, Body::Xylophone, Body::Bells, Body::Crotales, Body::Chimes, Body::HandChimes, Body::Kalimba, Body::Balafon] {
            let (lo, hi) = b.frame().notes;
            assert_eq!(b.own_range(), Some((lo, hi)), "{b:?}");
            let n = ZONE_NOTES.len();
            assert!((b.zone_note(0) - lo).abs() < 1e-9 && (b.zone_note(n - 1) - hi).abs() < 1e-9, "{b:?}");
            for i in 0..n {
                assert!((b.zone_key(b.zone_note(i), false) - ZONE_NOTES[i]).abs() < 1e-9);
            }
            // every key from a single zone, in order along the frame
            for all in [false, true] {
                let mut last = 0;
                for key in 21..=108 {
                    let (z, w, _, wb) = b.zone_weights(key as f64, all);
                    assert!(w == 1.0 && wb == 0.0 && z >= last, "{b:?} {key}");
                    last = z;
                }
            }
            assert_eq!(b.zone_weights(lo, false).0, 0);
            assert_eq!(b.zone_weights(hi, false).0, n - 1);
            // all keys: the keyboard's ends at the frame's, its middle in the middle
            assert_eq!((b.zone_weights(21.0, true).0, b.zone_weights(108.0, true).0), (0, n - 1));
            let mid = b.zone_weights(64.0, true).0;
            assert!(mid == 3 || mid == 4, "{b:?} {mid}");
        }
        // the pianos as played, mixed between neighboring zones, whatever the switch
        assert_eq!(Body::Grand.zone_key(60.0, true), 60.0);
        assert_eq!(Body::Grand.zone_weights(58.0, true), zone_weights(58.0));
    }

    /// The harps, the guitars and the zithers: their zones along their own range (its bridge, its
    /// strings), as the bars' (the psaltery's A♯4–F♯6 had two of the eight zones at piano notes
    /// inside it, the rest piled at its ends); with all keys the keyboard spread over it.
    #[test]
    fn string_zones_lie_along_the_instruments_own_range() {
        let n = ZONE_NOTES.len();
        for b in [Body::Harp, Body::LeverHarp, Body::Guitar, Body::Strumstick, Body::Psaltery, Body::DanTranh] {
            let (lo, hi) = b.own_range().unwrap_or_else(|| panic!("{b:?}: no range"));
            assert!(lo < hi && lo >= 21.0 && hi <= 108.0, "{b:?}");
            assert!((b.zone_note(0) - lo).abs() < 1e-9 && (b.zone_note(n - 1) - hi).abs() < 1e-9, "{b:?}");
            for i in 0..n {
                assert!((b.zone_key(b.zone_note(i), false) - ZONE_NOTES[i]).abs() < 1e-9, "{b:?}");
            }
            // mixed between neighboring zones (a board, not bars); the range's ends at the zones' ends
            let (a, wa, z, wb) = b.zone_weights(0.5 * (lo + hi) + 0.3, false);
            assert!(z == a + 1 && wa > 0.0 && wb > 0.0, "{b:?}");
            assert_eq!((b.zone_weights(lo, false).0, b.zone_weights(hi, false).2), (0, n - 1));
            // the keys beyond the range from its ends; with all keys the keyboard's ends at its ends
            assert_eq!((b.zone_weights(21.0, false), b.zone_weights(108.0, false)), ((0, 1.0, 0, 0.0), (n - 1, 1.0, n - 1, 0.0)));
            assert_eq!((b.zone_weights(21.0, true).0, b.zone_weights(108.0, true).0), (0, n - 1));
            assert!(b.zone_key(64.5, true) > ZONE_NOTES[0] && b.zone_key(64.5, true) < ZONE_NOTES[n - 1], "{b:?}");
            // every zone a distinct point where the geometry has one per note (the guitars: strings)
            if !matches!(b, Body::Guitar | Body::Strumstick) {
                let p: Vec<(f64, f64)> = (0..n).map(|i| b.bridge_point(b.zone_note(i))).collect();
                assert!(p.windows(2).all(|w| w[0] != w[1]), "{b:?} {p:?}");
            }
        }
        assert_eq!((Body::Grand.own_range(), Body::Upright.own_range()), (None, None));
    }

    #[test]
    fn bar_pairs_stay_outside_the_frame() {
        for b in [Body::Vibraphone, Body::Marimba, Body::Xylophone, Body::Bells, Body::Crotales, Body::Chimes, Body::HandChimes, Body::Kalimba, Body::Balafon] {
            bar_pairs_outside(b);
        }
    }

    fn bar_pairs_outside(b: Body) {
        let fr = b.frame();
        let inside = |m: [f64; 3]| m[0].abs() < fr.half_width && m[1].abs() < fr.half_depth && m[2] < 0.0 && m[2] > -fr.tube(m[0]);
        for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
            for spacing in [0.0, 0.5, 3.0] {
                for distance in [0.0, 0.05, 1.0, 30.0] {
                    let p = b.placement(pos, spacing, distance, 0.3).unwrap();
                    for m in p.mics {
                        assert!(m.iter().all(|v| v.is_finite()) && !inside(m) && m[2] > -fr.height, "{pos:?} {spacing} {distance} {m:?}");
                    }
                    assert!(p.distance >= p.range.0 && p.distance <= p.range.1);
                }
            }
            // no lid: one setup whatever the lid parameter, and finite zones
            assert_eq!(b.preset_setup(pos, 0.5, 0.0, 0.0), b.preset_setup(pos, 0.5, 0.0, 1.0), "{pos:?}");
            let z = zones_summary(b, pos, 0.5, 0.0);
            assert!(z.iter().all(|v| v.is_finite()) && z[2] > 0.0, "{pos:?} {z:?}");
        }
        // the player behind the bars at standing ears (the chimes' plane at the tubes' middles, 1.3 m:
        // the ears above it), the audience in front, close 1 above
        assert!(b.mics(MicPosition::Player, 0.18, 0.0, 1.0).unwrap().iter().all(|m| m[1] < -fr.half_depth && m[2] > fr.ear.1 - fr.height - 0.3));
        assert!(b.mics(MicPosition::Audience, 0.5, 0.0, 1.0).unwrap().iter().all(|m| m[1] > 3.0));
        assert!(b.mics(MicPosition::Hammers, 0.5, 0.0, 1.0).unwrap().iter().all(|m| m[2] > 0.2 && m[1].abs() < fr.half_depth));
        // a free microphone among the bars and tubes goes out through the nearest face
        for p in [[0.0, 0.0, -0.01], [0.6, 0.2, -0.1], [-0.7, -0.1, -0.5]] {
            let q = b.clamp_mic(U_WALL, 1.0, p);
            assert!(!inside(q) && q.iter().all(|v| v.is_finite()), "{p:?} → {q:?}");
        }
        let far = [0.3, 2.0, 0.5];
        assert_eq!(b.clamp_mic(U_WALL, 1.0, far), far);
        assert_eq!(b.clamp_setup(MicSetup { lid: 0.2, ..MicSetup::default() }).lid, 1.0);
        let d = b.drawing(U_WALL, 1.0);
        assert!(d.front_axis == 0 && !d.outline_front.is_empty() && d.outline_front.len() <= 32 && d.case_top.len() <= 24);
        assert!(d.markers[1..].iter().all(|m| m.iter().all(|v| v.is_finite())));
    }

    #[test]
    fn harp_bridge_runs_up_the_board_from_the_bass() {
        let (u, n) = h_axes();
        assert!((dot(u, n)).abs() < 1e-12 && u[2] > 0.8 && u[1] < 0.0 && n[1] > 0.8);
        let (lo, hi) = (Body::Harp.sources(24.0)[0].pos, Body::Harp.sources(103.0)[0].pos);
        assert!(lo[2] < -0.4 && hi[2] > 0.4 && lo[1] > hi[1], "{lo:?} {hi:?}");
        assert!(lo[2] > -HarpDims::CONCERT.board_height() + 0.15, "the board's foot above the base");
        assert_eq!(Body::Harp.bridge_point(24.0), (HarpDims::CONCERT.bridge_range().0, 0.0));
    }

    #[test]
    fn harp_positions_and_clamp_keep_out_of_the_soundbox() {
        let (u, n) = h_axes();
        let inside = |m: [f64; 3]| {
            let (s, t) = (dot(m, u), dot(m, n));
            let (depth, half) = HarpDims::CONCERT.box_at(s);
            s.abs() <= HarpDims::CONCERT.board_len() / 2.0 && m[0].abs() < half && t < 0.0 && t > -depth
        };
        for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
            for spacing in [0.0, 0.18, 0.5, 2.0, 10.0] {
                for distance in [0.0, 0.01, 0.3, 1.0, 5.0, 20.0] {
                    for m in Body::Harp.mics(pos, spacing, distance, 1.0).unwrap() {
                        assert!(m.iter().all(|v| v.is_finite()) && !inside(m) && m[2] > -HarpDims::CONCERT.board_height(), "{pos:?} s={spacing} d={distance} {m:?}");
                    }
                }
            }
        }
        // close 1 in front of the board, close 2 behind the box, the player behind the board
        for m in Body::Harp.mics(MicPosition::Hammers, 0.5, 0.0, 1.0).unwrap() {
            assert!(dot(m, n) > 0.3, "{m:?}");
        }
        for m in Body::Harp.mics(MicPosition::Tail, 0.5, 0.0, 1.0).unwrap() {
            assert!(dot(m, n) < -HarpDims::CONCERT.box_at(dot(m, u)).0, "{m:?}");
        }
        assert!(Body::Harp.mics(MicPosition::Player, 0.18, 0.0, 1.0).unwrap().iter().all(|m| m[1] < -0.4));
        // a free microphone in the box is pushed out, through the nearer face
        for p in [[0.0, 0.0, 0.0], [0.0, -0.05, -0.1], [0.05, 0.1, -0.4]] {
            let q = Body::Harp.clamp_mic(U_WALL, 1.0, p);
            assert!(!inside(q), "{p:?} → {q:?}");
        }
        let far = [1.0, 2.0, 0.3];
        assert_eq!(Body::Harp.clamp_mic(U_WALL, 1.0, far), far);
    }

    #[test]
    fn harp_zones_are_finite_everywhere() {
        let board = test_board();
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
        for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
            let z = zones(Body::Harp, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &Body::Harp.preset_setup(pos, 0.5, 0.0, 1.0).unwrap(), false);
            assert!(z.room_gain.is_finite() && z.room_gain > 0.0 && z.room_gain < 100.0, "{pos:?}: {}", z.room_gain);
            assert!(z.body.iter().flatten().flatten().flatten().all(|v| v.is_finite()), "{pos:?}");
        }
        let d = Body::Harp.drawing(U_WALL, 1.0);
        assert!(d.front_axis == 1 && !d.outline_front.is_empty() && d.outline_front.len() <= 32 && d.case_top.len() <= 24);
    }

    #[test]
    fn lever_harp_is_the_concert_harp_at_three_quarters() {
        let (c, l) = (HarpDims::CONCERT, HarpDims::LEVER);
        assert!((l.board_len() - 0.75 * c.board_len()).abs() < 1e-12 && (l.board_height() - 0.75 * c.board_height()).abs() < 1e-12);
        assert_eq!(Body::LeverHarp.harp_dims(), l);
        assert!(Body::LeverHarp.is_harp() && Body::Harp.is_harp() && !Body::Grand.is_harp());
        // its range on its board: C2 at the foot, A6 at the top
        assert_eq!(Body::LeverHarp.bridge_point(36.0), (l.bridge_range().0, 0.0));
        assert_eq!(Body::LeverHarp.bridge_point(93.0), (l.bridge_range().1, 0.0));
        // the ears stay seated at 1.25 m above the floor
        let ear = Body::LeverHarp.mics(MicPosition::Player, 0.18, 0.0, 1.0).unwrap()[0];
        assert!((ear[2] + l.board_height() - 1.25).abs() < 1e-9);
        // outside its soundbox at every position, finite responses, no lid
        let (u, n) = h_axes();
        let inside = |m: [f64; 3]| {
            let (sa, t) = (dot(m, u), dot(m, n));
            let (depth, half) = l.box_at(sa);
            sa.abs() <= l.board_len() / 2.0 && m[0].abs() < half && t < 0.0 && t > -depth
        };
        let board = test_board();
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
        for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
            for d in [0.0, 0.3, 1.0, 5.0] {
                for m in Body::LeverHarp.mics(pos, 0.5, d, 1.0).unwrap() {
                    assert!(!inside(m) && m[2] > -l.board_height(), "{pos:?} {d} {m:?}");
                }
            }
            assert_eq!(Body::LeverHarp.preset_setup(pos, 0.5, 0.0, 0.0), Body::LeverHarp.preset_setup(pos, 0.5, 0.0, 1.0));
            let z = zones(Body::LeverHarp, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &Body::LeverHarp.preset_setup(pos, 0.5, 0.0, 1.0).unwrap(), false);
            assert!(z.room_gain.is_finite() && z.body.iter().flatten().flatten().flatten().all(|v| v.is_finite()), "{pos:?}");
        }
        assert!(!Body::LeverHarp.clamp_mic(U_WALL, 1.0, [0.0, -0.05, -0.1]).iter().any(|v| !v.is_finite()));
        assert!(!inside(Body::LeverHarp.clamp_mic(U_WALL, 1.0, [0.0, -0.05, -0.1])));
        let d = Body::LeverHarp.drawing(U_WALL, 1.0);
        assert!(d.front_axis == 1 && d.floor_z == -l.board_height());
    }

    #[test]
    fn guitar_strings_microphones_and_drawing() {
        // first position: E2 on the low E, A2 on the A string, B3 on the B string, E4 up the high E
        let g = GuitarDims::CLASSICAL;
        assert_eq!([40.0, 44.0, 45.0, 58.0, 59.0, 64.0, 83.0, 30.0].map(|n| 6 - g.string(n)), [6, 6, 5, 3, 2, 1, 1, 6]);
        // the strumstick's: the bass string to G3, the middle to D♯4, the melody string above
        assert_eq!([50.0, 56.0, 57.0, 63.0, 64.0, 81.0].map(|n| GuitarDims::STRUMSTICK.string(n)), [0, 0, 1, 1, 2, 2]);
        for body in [Body::Guitar, Body::Strumstick] {
            let gd = body.guitar_dims();
            let (lo, hi) = (gd.strings[0].max(40.0), *gd.strings.last().unwrap());
            assert!(body.bridge_point(lo).1 > 0.0 && body.bridge_point(hi).1 < 0.0, "{body:?}");
            let inside = |m: [f64; 3]| {
                let half = gd.half_width(m[0]);
                half > 0.0 && m[2].abs() < half && m[1] < 0.0 && m[1] > -gd.depth
            };
            let board = test_board();
            let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
            let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
            for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
                for d in [0.0, 0.1, 1.0, 5.0] {
                    for m in body.mics(pos, 0.5, d, 1.0).unwrap() {
                        assert!(!inside(m) && m[2] > -G_HEIGHT, "{body:?} {pos:?} {d} {m:?}");
                    }
                }
                let z = zones(body, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &body.preset_setup(pos, 0.5, 0.0, 1.0).unwrap(), false);
                assert!(z.room_gain.is_finite() && z.body.iter().flatten().flatten().flatten().all(|v| v.is_finite()), "{body:?} {pos:?}");
            }
            // a free microphone inside the body comes out of it
            for p in [[0.0, -0.03, 0.0], [-0.15, -0.01, 0.05], [0.08, -0.05, -0.04]] {
                let q = body.clamp_mic(U_WALL, 1.0, p);
                assert!(!inside(q) && q.iter().all(|v| v.is_finite()), "{body:?} {p:?} → {q:?}");
            }
            let far = [1.0, 2.0, 0.3];
            assert_eq!(body.clamp_mic(U_WALL, 1.0, far), far);
            assert!(!body.has_lid() && !body.is_harp() && !body.is_bars() && !body.is_zither());
            let d = body.drawing(U_WALL, 1.0);
            assert!(d.front_axis == 0 && d.floor_z == -G_HEIGHT && d.outline_front.len() <= 32 && d.case_top.len() <= 24);
            assert!(d.markers[1..].iter().all(|m| m.iter().all(|v| v.is_finite())));
        }
        // the strumstick: a narrower, shorter body than the guitar's
        let (s, c) = (GuitarDims::STRUMSTICK, GuitarDims::CLASSICAL);
        assert!(s.half_width(0.0) < 0.5 * c.half_width(0.0) && s.outline()[0].0 < c.outline()[0].0);
    }

    #[test]
    fn zithers_lie_on_a_table() {
        let board = test_board();
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
        for body in [Body::Psaltery, Body::DanTranh] {
            let zd = body.zither_dims();
            let fr = zd.frame;
            assert!(body.is_zither() && !body.is_bars() && !body.has_lid() && Body::from_name(Some(if body == Body::Psaltery { "psaltery" } else { "dantranh" })) == body);
            // the bass's bridge left of the treble's; each note's on the top
            let (lo, hi) = zd.notes;
            assert!(body.bridge_point(lo).0 < body.bridge_point(hi).0);
            for n in [lo, (lo + hi) / 2.0, hi] {
                let (x, y) = body.bridge_point(n);
                assert!(x.abs() <= fr.half_width && y.abs() <= fr.half_depth, "{body:?} {n}: ({x}, {y})");
            }
            let inside = |m: [f64; 3]| m[0].abs() < fr.half_width && m[1].abs() < fr.half_depth && m[2] < 0.0 && m[2] > -fr.tubes.0;
            for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
                for d in [0.0, 0.1, 1.0, 5.0] {
                    for m in body.mics(pos, 0.5, d, 1.0).unwrap() {
                        assert!(!inside(m) && m[2] > -fr.height, "{body:?} {pos:?} {d} {m:?}");
                    }
                }
                let z = zones(body, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &body.preset_setup(pos, 0.5, 0.0, 1.0).unwrap(), false);
                assert!(z.room_gain.is_finite() && z.body.iter().flatten().flatten().flatten().all(|v| v.is_finite()), "{body:?} {pos:?}");
            }
            // the player behind it, above the top
            let p = body.mics(MicPosition::Player, 0.2, 0.0, 1.0).unwrap();
            assert!(p.iter().all(|m| m[1] < -fr.half_depth && m[2] > 0.3), "{body:?} {p:?}");
            // a free microphone in the box comes out of it
            let q = body.clamp_mic(U_WALL, 1.0, [0.0, 0.0, -0.02]);
            assert!(!inside(q), "{body:?} {q:?}");
            let d = body.drawing(U_WALL, 1.0);
            assert!(d.floor_z == -fr.height && d.case_top.len() >= 3 && d.case_top.len() <= 24 && d.outline_front.len() <= 32);
            // the table's legs to the floor
            assert!(d.outline_front.iter().any(|p| p[1] == -fr.height));
        }
        // the đàn tranh over twice the psaltery's width; the psaltery's rose
        assert!(ZitherDims::DANTRANH.frame.half_width > 2.0 * ZitherDims::PSALTERY.frame.half_width);
        assert!(Body::Psaltery.sources(80.0).len() == 2 && Body::DanTranh.sources(60.0).len() == 1);
    }

    #[test]
    fn upright_player_hears_the_front_directly_and_the_back_diffracted() {
        let pos = MicPosition::Player;
        let s = Body::Upright.sources(60.0);
        let (back, panel) = (s[0], s[2]);
        for m in Body::Upright.mics(pos, 0.16, 0.0, 1.0).unwrap() {
            let f = source_paths(Body::Upright, pos, &panel, m);
            assert!(f[0].via.is_none() && f[0].lowpass[1].is_none(), "front panel direct {m:?}");
            assert_eq!(f[0].lowpass[0], Some(U_FRONT_LOWPASS_HZ));
            let b = source_paths(Body::Upright, pos, &back, m);
            assert_eq!(b[0].lowpass[1], Some(DIFFRACTION_LOWPASS_HZ), "back direct not diffracted {m:?}");
        }
    }

    #[test]
    fn upright_open_top_pair_hears_the_opening_and_the_lid() {
        let pos = MicPosition::Hammers;
        let top = Body::Upright.sources(60.0)[1];
        for m in Body::Upright.mics(pos, 0.5, 0.0, 1.0).unwrap() {
            let p = source_paths(Body::Upright, pos, &top, m);
            // the open top is unobstructed: no lowpass on its direct sound
            assert!(p[0].lowpass == [None; 3], "opening direct {m:?}: {:?}", p[0].lowpass);
            assert!(p.iter().any(|p| p.via == Some(U_LID)), "lid {m:?}");
            assert!(p.iter().all(|p| p.via != Some(U_FLOOR)), "floor above the case {m:?}");
        }
    }

    #[test]
    fn upright_microphones_stay_outside_the_case_and_off_the_wall() {
        for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
            for spacing in [0.0, 0.16, 0.5, 2.0, 10.0] {
                for distance in [0.0, 0.01, 0.3, 1.0, 5.0, 20.0] {
                    for m in Body::Upright.mics(pos, spacing, distance, 1.0).unwrap() {
                        assert!(m.iter().all(|v| v.is_finite()), "{pos:?} {m:?}");
                        let inside = Body::Upright.over_case(m) && m[2] < U_CASE_TOP && m[2] > -U_BOARD_HEIGHT;
                        assert!(!inside, "inside the case: {pos:?} s={spacing} d={distance} {m:?}");
                        assert!(m[1] < u_wall(pos) - 0.05, "at the wall: {pos:?} s={spacing} d={distance} {m:?}");
                        assert!(m[2] > -U_BOARD_HEIGHT, "below the floor: {pos:?} {m:?}");
                    }
                }
            }
        }
        assert!(Body::Upright.mics(MicPosition::Recording, 0.5, 0.0, 1.0).is_none());
    }

    /// a small synthetic board: modes every 26 Hz from 35 Hz, deterministic amplitudes
    fn test_board() -> crate::physics::Board {
        let n = 400;
        let val = |i: usize, k: usize| (((i * 7919 + k * 104_729) % 1000) as f64 / 500.0) - 1.0;
        crate::physics::Board {
            f: (0..n).map(|i| 35.0 + 26.0 * i as f64).collect(),
            sigma: vec![std::f64::consts::PI * 26.0; n],
            phi: (0..n).map(|i| val(i, 1)).collect(),
            density: 1.0 / 26.0,
            r: [(0..n).map(|i| val(i, 2)).collect(), (0..n).map(|i| val(i, 3)).collect()],
            r_h: [(0..n).map(|i| val(i, 4)).collect(), (0..n).map(|i| val(i, 5)).collect()],
            measured_from: n,
            h_share: 0.0,
            mobility: None,
        }
    }

    /// `along_paths` as it was (its FFT plans, directivity and air per call): the shared ones must give the same bits
    fn along_paths_per_call(irs: &[Vec<f32>], all: &[Vec<Path>], first: f64, sr: f64) -> Vec<Vec<f32>> {
        let longest = all.iter().flatten().map(|p| p.length).fold(0.0, f64::max);
        let body = Radiation { channels: irs.len(), ir: irs.to_vec() };
        let len = body.ir.iter().map(Vec::len).max().unwrap_or(0) + ((longest - first) / SPEED_OF_SOUND * sr).ceil() as usize + 64;
        let n = len.next_power_of_two();
        let mut planner = RealFftPlanner::<f64>::new();
        let (fwd, inv) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
        let dir = Directivity::new(n, sr);
        let mut out = Vec::new();
        for (ch, ir) in body.ir.iter().enumerate() {
            let mut buf: Vec<f64> = ir.iter().map(|&v| v as f64).collect();
            buf.resize(n, 0.0);
            let mut spec = fwd.make_output_vec();
            fwd.process(&mut buf, &mut spec).unwrap();
            let mut acc = vec![Complex64::new(0.0, 0.0); spec.len()];
            for p in &all[ch.min(all.len() - 1)] {
                let gain = p.gain * REF_DISTANCE / p.length.hypot(ZONE_RADIUS);
                let delay = (p.length - first) / SPEED_OF_SOUND;
                let extra = p.length - REF_DISTANCE;
                for (i, (a, s)) in acc.iter_mut().zip(&spec).enumerate() {
                    let f = i as f64 * sr / n as f64;
                    let air = 10f64.powf(-air_db_per_m(f) * extra.max(0.0) / 20.0);
                    let ph = Complex64::from_polar(1.0, -2.0 * std::f64::consts::PI * f * delay);
                    let mut v = s * ph * (gain * air * dir.gain(i, p.cos_theta));
                    for fc in p.lowpass.iter().flatten() {
                        v /= Complex64::new(1.0, f / fc);
                    }
                    *a += v;
                }
            }
            acc[0].im = 0.0;
            if let Some(last) = acc.last_mut() {
                last.im = 0.0;
            }
            let mut y = inv.make_output_vec();
            inv.process(&mut acc, &mut y).unwrap();
            y.truncate(len);
            out.push(y.iter().map(|v| (v / n as f64) as f32).collect());
        }
        out
    }

    #[test]
    fn along_paths_with_shared_plans_gives_the_same_bits() {
        let sr = 48000.0;
        let body = Body::Grand;
        let setup = body.preset_setup(MicPosition::Player, 0.5, 0.0, 0.5).unwrap(); // half-open: lid paths too
        let refl = body.reflectors(setup.wall, setup.lid);
        let sources = body.sources(body.zone_note(3));
        let mut all: Vec<Vec<Path>> = setup.mics.iter().map(|m| sources.iter().flat_map(|s| paths(s, m.pos, &refl, body.over_case(m.pos))).collect()).collect();
        all[1][0].lowpass[2] = Some(3000.0); // a lowpass, as the closed lid's
        let first = all.iter().flatten().map(|p| p.length).fold(f64::INFINITY, f64::min);
        let irs: Vec<Vec<f32>> = (0..2).map(|c| (0..9000).map(|i| (((i * 31 + c * 7) % 97) as f32 / 97.0 - 0.5) * (-(i as f32) / 3000.0).exp()).collect()).collect();
        let fft = PathFft::new(path_len(&irs, &all, first, sr).next_power_of_two(), sr);
        assert_eq!(along_paths(&irs, &all, first, sr, &fft), along_paths_per_call(&irs, &all, first, sr));
    }

    /// `synth_body` as it was, mode by mode over the whole length by recursion: the split one must give the same (to rounding)
    fn synth_body_mode_by_mode(board: &crate::physics::Board, h: &(Vec<f64>, Vec<f64>), rows: &[(Vec<f64>, &[f64])], sr: f64, sr_ref: f64, len: usize) -> Vec<Vec<f64>> {
        let keep: Vec<usize> = (0..board.f.len()).filter(|&m| board.f[m] < 0.45 * sr_ref).collect();
        let a: Vec<f64> = keep.iter().map(|&m| board.sigma[m].sqrt()).collect();
        let mut out = vec![vec![0.0; len]; rows.len()];
        let mut osc = vec![0.0; len];
        for (i, &m) in keep.iter().enumerate() {
            let z = realfft::num_complex::Complex64::new(-board.sigma[m] / sr, 2.0 * std::f64::consts::PI * board.f[m] / sr).exp();
            let mut c = realfft::num_complex::Complex64::new(1.0, 0.0);
            for o in osc.iter_mut() {
                *o = c.re;
                c *= z;
            }
            for (row, (phi, r)) in out.iter_mut().zip(rows) {
                let g = phi[m] * a[i] * r[m];
                if g != 0.0 {
                    row.iter_mut().zip(&osc).for_each(|(y, &o)| *y += g * o);
                }
            }
        }
        crate::noise::body_filter(&h.0, &h.1, sr, &mut out);
        for row in &mut out {
            let e = row.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-300);
            row.iter_mut().for_each(|v| *v /= e);
        }
        out
    }

    #[test]
    fn synth_body_gives_the_mode_by_mode_sum() {
        let board = test_board();
        let h = (vec![20f64.log2(), 1000f64.log2(), 20000f64.log2()], vec![0.0, -3.0, -6.0]);
        let mut silent = board.r[0].clone();
        silent.iter_mut().step_by(3).for_each(|v| *v = 0.0); // some gains 0: skipped alike
        let shapes: Vec<Vec<f64>> = (0..3).map(|k| board.phi.iter().map(|p| p * (1.0 + k as f64)).collect()).collect();
        let mut rows: Vec<(Vec<f64>, &[f64])> = Vec::new();
        for sh in &shapes {
            for r in [&board.r[0], &board.r[1], &board.r_h[0], &silent] {
                rows.push((sh.clone(), r.as_slice()));
            }
        }
        for len in [1usize, 511, 4095, 20_011] { // across and inside the tiles and the threads' spans
            let (a, b) = (synth_body(&board, &h, &rows, 1, 48000.0, 44100.0, len), synth_body_mode_by_mode(&board, &h, &rows, 48000.0, 44100.0, len));
            for (ra, rb) in a.iter().zip(&b) {
                let err = ra.iter().zip(rb).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
                assert!(err < 1e-9, "len {len}: {err:e}"); // rows normalized to energy 1: rounding only
            }
        }
    }

    #[test]
    fn upright_zones_are_finite_at_close_positions_with_direct_level() {
        // the level stays finite for 1/R near 0
        let board = test_board();
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
        for pos in [MicPosition::Hammers, MicPosition::Tail, MicPosition::Player] {
            for direct in [false, true] {
                let z = zones(Body::Upright, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &Body::Upright.preset_setup(pos, 0.5, 0.0, 1.0).unwrap(), direct);
                assert!(z.room_gain.is_finite() && z.room_gain > 0.0 && z.room_gain < 100.0, "{pos:?} {direct}: {}", z.room_gain);
                assert!(z.body.iter().flatten().flatten().flatten().all(|v| v.is_finite()), "{pos:?} {direct}");
            }
        }
    }

    #[test]
    fn upright_back_face_reaches_the_front_only_around_the_case() {
        // mirror paths too: the wall image of the back face, seen from in front of the
        // board, arrives around the case (spec), not through it
        let back = Body::Upright.sources(64.0)[0];
        for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers] {
            for distance in [0.0, 20.0] {
                for m in Body::Upright.mics(pos, 0.5, distance, 1.0).unwrap() {
                    for p in source_paths(Body::Upright, pos, &back, m) {
                        assert_eq!(p.lowpass[1], Some(DIFFRACTION_LOWPASS_HZ), "{pos:?} d={distance} via {:?} {m:?}", p.via);
                    }
                }
            }
        }
    }

    #[test]
    fn upright_paths_do_not_pass_through_the_lid() {
        // the open top's wall reflection lies behind the lid, seen from the open-top pair
        let top = Body::Upright.sources(64.0)[1];
        for m in Body::Upright.mics(MicPosition::Hammers, 0.5, 0.0, 1.0).unwrap() {
            let p = source_paths(Body::Upright, MicPosition::Hammers, &top, m);
            assert!(p.iter().all(|p| p.via != Some(U_WALL_REFL)), "wall path through the lid {m:?}");
            assert!(p.iter().any(|p| p.via == Some(U_LID)), "lid reflection kept {m:?}");
        }
    }

    #[test]
    fn upright_audience_is_clearly_above_the_open_top() {
        // on the open top's plane the blocking rule is a knife edge (dot = 0): any change
        // to either height would flip its direct sound to diffracted
        let top = Body::Upright.sources(64.0)[1];
        for distance in [0.0, 1.0, 20.0] {
            for m in Body::Upright.mics(MicPosition::Audience, 0.5, distance, 1.0).unwrap() {
                assert!(dot(sub(m, top.pos), top.normal) > 0.02, "{m:?}");
            }
        }
    }

    #[test]
    fn grand_sources_are_the_bridge_points() {
        for n in [21.0, 40.0, 64.0, 108.0] {
            let s = Body::Grand.sources(n);
            let (x, y) = bridge_point(n);
            assert_eq!(s.len(), 1);
            assert_eq!(s[0].pos, [x, y, 0.0]);
            assert!(!s[0].one_sided && s[0].lowpass_hz.is_none());
        }
    }

    const BODIES: [Body; 2] = [Body::Grand, Body::Upright];
    const POSITIONS: [MicPosition; 4] = [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail];

    fn center(p: &Placement) -> [f64; 3] {
        let [a, b] = p.mics;
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
    }

    /// the pair where the engine put it before `placement` existed (the formulas of the
    /// former `grand_mics`/`upright_mics`, evaluated by hand)
    #[test]
    fn placement_keeps_the_former_positions() {
        let cases: [(Body, MicPosition, f64, f64, [[f64; 3]; 2]); 6] = [
            // the player centered in front of the keyboard (was x = −0.10 until 2026-10-04)
            (Body::Grand, MicPosition::Player, 0.5, 0.0, [[-0.25, -1.25, 0.45], [0.25, -1.25, 0.45]]),
            (Body::Upright, MicPosition::Player, 0.5, 0.0, [[-0.25, -1.0, 0.45], [0.25, -1.0, 0.45]]),
            (Body::Grand, MicPosition::Audience, 0.5, 2.0, [[2.0, -0.55, 0.45], [2.0, -0.05, 0.45]]),
            (Body::Grand, MicPosition::Hammers, 0.5, 0.0, [[-0.25, -0.55, 0.3], [0.25, -0.55, 0.3]]),
            (Body::Upright, MicPosition::Hammers, 0.5, 0.0, [[-0.25, U_HAMMER_Y, 0.85], [0.25, U_HAMMER_Y, 0.85]]),
            (Body::Upright, MicPosition::Tail, 0.5, 0.0, [[-0.25, 0.4, 0.0], [0.25, 0.4, 0.0]]),
        ];
        for (body, pos, spacing, distance, want) in cases {
            let got = body.mics(pos, spacing, distance, 1.0).unwrap();
            for i in 0..2 {
                for k in 0..3 {
                    assert!((got[i][k] - want[i][k]).abs() < 1e-12, "{body:?} {pos:?}: {got:?}");
                }
            }
        }
    }

    #[test]
    fn close_height_is_the_higher_microphones() {
        // the lid stops the bass-side microphone first; the distance is the height the
        // other one still has (the parameter's value), so a drag starts where it is
        let p = Body::Grand.placement(MicPosition::Tail, 0.5, 0.0, 1.0).unwrap();
        assert!(p.mics[0][2] < 0.45 - 1e-3, "{:?}", p.mics);
        assert!((p.distance - 0.45).abs() < 1e-12, "{}", p.distance);
        let q = Body::Grand.placement(MicPosition::Hammers, 0.5, 0.6, 1.0).unwrap();
        assert!((q.distance - 0.6).abs() < 1e-12, "{}", q.distance);
        assert!((q.range.1 - (lid_height(0.25, 1.0) - CLOSE_LID_CLEARANCE)).abs() < 1e-12);
    }

    /// the microphone farthest along the axis (the pair's distance; open positions: both alike)
    fn farthest(p: &Placement) -> [f64; 3] {
        let [a, b] = p.mics;
        if dot(sub(a, p.origin), p.axis) >= dot(sub(b, p.origin), p.axis) { a } else { b }
    }

    #[test]
    fn distance_reads_back_along_the_axis() {
        for body in BODIES {
            for pos in POSITIONS {
                let p = body.placement(pos, 0.3, 0.0, 1.0).unwrap();
                // the default distance lies within the limits and reads back from the pair
                assert!(p.range.0 <= p.distance && p.distance <= p.range.1, "{body:?} {pos:?}");
                let c = sub(farthest(&p), p.origin);
                assert!((dot(c, p.axis) - p.distance).abs() < 1e-12, "{body:?} {pos:?}");
                // a distance inside the limits is taken as given
                let d = (p.range.0 + p.range.1.min(p.range.0 + 0.5)) / 2.0;
                let q = body.placement(pos, 0.3, d, 1.0).unwrap();
                assert!((q.distance - d).abs() < 1e-12, "{body:?} {pos:?}: {} vs {d}", q.distance);
                assert!((dot(sub(farthest(&q), q.origin), q.axis) - d).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn limits_hold_for_every_body_and_position() {
        for body in BODIES {
            for pos in POSITIONS {
                let far = body.placement(pos, MAX_SPACING, MAX_DISTANCE, 1.0).unwrap();
                assert!(far.distance <= far.range.1 + 1e-12, "{body:?} {pos:?}");
                assert!(far.spacing <= far.spacing_max + 1e-12);
                let near = body.placement(pos, 0.0, 1e-3, 1.0).unwrap();
                assert!(near.distance >= near.range.0 - 1e-12, "{body:?} {pos:?}");
                // the spacing reads back along the spread axis
                let [a, b] = far.mics;
                assert!((dot(sub(b, a), far.spread) - far.spacing).abs() < 1e-12, "{body:?} {pos:?}");
            }
        }
    }

    #[test]
    fn the_kalimba_is_held_not_stood() {
        // no stand down to the floor: its player sits behind it, the hands hold the board
        let d = Body::Kalimba.drawing(0.0, 1.0);
        let pts: Vec<[f64; 2]> = d.outline_front.iter().copied().filter(|p| p[0].is_finite()).collect();
        assert!(d.outline_front.len() <= 32 && d.floor_z == -KALIMBA.height);
        assert!(pts.iter().all(|p| p[1] > -0.6), "{pts:?}");
        // the board and the hands beside it within its half width and a hand's
        assert!(pts.iter().any(|p| p[0] < -KALIMBA.half_width) && pts.iter().any(|p| p[0] > KALIMBA.half_width));
        // the bars' stand reaches the floor
        assert!(Body::Marimba.drawing(0.0, 1.0).outline_front.iter().any(|p| p[1] == -Body::Marimba.frame().height));
    }

    #[test]
    fn drawings_hold_the_markers_and_the_wall() {
        for body in BODIES {
            let d = body.drawing(body.wall_of(MicPosition::Player), 1.0);
            assert!(d.case_top.len() >= 4 && d.case_top.len() <= 24);
            assert!(d.markers[0][0].is_nan());
            for (i, pos) in POSITIONS.iter().enumerate() {
                let c = center(&body.placement(*pos, 0.5, 0.0, 1.0).unwrap());
                assert_eq!(d.markers[i + 1], [c[0], c[1]]);
            }
            assert_eq!(d.wall_y.is_some(), body == Body::Upright);
        }
        assert_eq!(Body::Upright.drawing(Body::Upright.wall_of(MicPosition::Tail), 1.0).wall_y, Some(U_WALL_BACK));
        assert_eq!(Body::Upright.drawing(Body::Upright.wall_of(MicPosition::Player), 1.0).wall_y, Some(U_WALL));
    }

    /// every preset's radiation, summarized (energy, a position-weighted sum, the room gain)
    fn zones_summary(body: Body, pos: MicPosition, spacing: f64, distance: f64) -> [f64; 3] {
        let board = test_board();
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
        let z = zones(body, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &body.preset_setup(pos, spacing, distance, 1.0).unwrap(), false);
        let (mut energy, mut weighted) = (0.0, 0.0);
        for (i, v) in z.body.iter().flatten().flatten().flatten().enumerate() {
            let v = *v as f64;
            energy += v * v;
            weighted += v * ((i % 97) as f64 + 1.0);
        }
        [energy, weighted, z.room_gain]
    }

    #[test]
    fn the_open_lid_reflects_from_its_size_up() {
        let mut lids = 0;
        for note in (21..=108).step_by(3) {
            let src = Body::Grand.sources(note as f64)[0];
            for m in Body::Grand.mics(MicPosition::Player, 0.5, 0.0, 1.0).unwrap() {
                let p = source_paths(Body::Grand, MicPosition::Player, &src, m);
                for lid in p.iter().filter(|p| p.via == Some(GRAND_LID)) {
                    let hz = lid.highpass.expect("bounded: a cutoff");
                    // grazing over the bass keys (1.2 kHz), overhead in the treble (~120 Hz)
                    assert!((80.0..2000.0).contains(&hz), "{note} {m:?}: {hz} Hz");
                    lids += 1;
                }
                assert!(p.iter().filter(|p| p.via != Some(GRAND_LID)).all(|p| p.highpass.is_none()), "{note} {m:?}: direct and floor full band");
            }
        }
        assert!(lids > 0, "the lid reaches Player");
    }

    #[test]
    fn a_lid_taken_off_reflects_nothing() {
        for (body, lid_index) in [(Body::Grand, GRAND_LID), (Body::Upright, U_LID)] {
            let r = body.reflectors(U_WALL, LID_OFF);
            assert_eq!(r.len(), lid_index, "{body:?}: the lid was the last reflector");
            assert_eq!(format!("{r:?}"), format!("{:?}", &body.reflectors(U_WALL, 1.0)[..lid_index]), "{body:?}: the floor and wall stay");
            for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
                // the positions are the open lid's
                assert_eq!(body.mics(pos, 0.5, 0.0, LID_OFF), body.mics(pos, 0.5, 0.0, 1.0), "{body:?} {pos:?}");
                for m in body.mics(pos, 0.5, 0.0, LID_OFF).unwrap() {
                    for note in (21..=108).step_by(8) {
                        for src in body.sources(note as f64) {
                            let p = paths(&src, m, &r, body.over_case(m));
                            assert!(p.iter().all(|p| p.via != Some(lid_index)), "{body:?} {pos:?} {note}");
                        }
                    }
                }
            }
            assert!(body.drawing(U_WALL, LID_OFF).lid_front.iter().flatten().all(|v| v.is_nan()), "{body:?}: no lid drawn");
        }
        assert_eq!(lid_leak(LID_OFF), None);
        // nothing over a grand's case: a free microphone may hang above the open lid's height
        let over = [0.0, 0.2, 1.5];
        assert!(lid_height(over[0], 1.0) < 1.5);
        assert_eq!(Body::Grand.clamp_mic(U_WALL, LID_OFF, over), over);
        assert!(Body::Grand.clamp_mic(U_WALL, 1.0, over)[2] < 1.5);
    }

    /// Below 160 Hz the pair at Player hears the board alike: the fitted right channel 6 dB up and
    /// leaning on the left (as `coherent_low` makes it) once put the right channel's bass 6–9 dB up.
    #[test]
    fn a_players_pair_hears_the_bass_alike() {
        let mut board = test_board();
        for m in 0..board.f.len() {
            if board.f[m] < 400.0 {
                board.r[1][m] = board.r[0][m];
                board.r_h[1][m] = board.r_h[0][m];
            }
        }
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| (1 + c) as f32 * (((i * 7 + c) % 13) as f32 / 13.0 - 0.5)).collect()).collect();
        let setup = Body::Grand.preset_setup(MicPosition::Player, 0.5, 0.0, 0.0).unwrap();
        let z = zones(Body::Grand, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &setup, false);
        let low = |v: &[f32]| {
            let n = v.len().next_power_of_two();
            let fft = RealFftPlanner::<f64>::new().plan_fft_forward(n);
            let mut buf: Vec<f64> = v.iter().map(|&x| x as f64).collect();
            buf.resize(n, 0.0);
            let mut spec = fft.make_output_vec();
            fft.process(&mut buf, &mut spec).unwrap();
            spec.iter().take((160.0 * n as f64 / 48000.0) as usize).map(|c| c.norm_sqr()).sum::<f64>()
        };
        let (mut l, mut r) = (0.0, 0.0);
        for pols in &z.body {
            for chans in pols {
                l += low(&chans[0]);
                r += low(&chans[1]);
            }
        }
        let ild = 10.0 * (r / l).log10();
        assert!(ild.abs() < 1.5, "right − left below 160 Hz: {ild:+.2} dB");
    }

    #[test]
    fn preset_microphones_are_fixed_points_of_the_clamp() {
        for (body, pos, spacing, distance) in golden_cases().into_iter().chain([
            (Body::Grand, MicPosition::Player, 10.0, 20.0),
            (Body::Grand, MicPosition::Hammers, 10.0, 5.0),
            (Body::Grand, MicPosition::Tail, 0.0, 0.01),
            (Body::Upright, MicPosition::Tail, 10.0, 20.0),
            (Body::Upright, MicPosition::Hammers, 0.0, 0.0),
        ]) {
            let s = body.preset_setup(pos, spacing, distance, 1.0).unwrap();
            for m in s.mics {
                assert_eq!(body.clamp_mic(s.wall, 1.0, m.pos), m.pos, "{body:?} {pos:?} {spacing} {distance}");
            }
            assert_eq!(body.clamp_setup(s), s, "{body:?} {pos:?}");
        }
    }

    #[test]
    fn clamp_replaces_non_finite_values_by_the_defaults() {
        let bad = |v: f64| {
            let m = Mic { pos: [v, v, v], pattern: v, yaw: v, pitch: v };
            MicSetup { mics: [m, m], wall: v, lid: 1.0 }
        };
        for body in [Body::Grand, Body::Upright] {
            for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let c = body.clamp_setup(bad(v));
                for m in c.mics {
                    assert!(m.pos.iter().all(|p| p.is_finite()) && m.pattern.is_finite() && m.yaw.is_finite() && m.pitch.is_finite(), "{body:?} {v}");
                }
                assert!(c.wall.is_finite(), "{body:?} {v}");
                assert_eq!(c, c, "equal to itself");
                assert_eq!(body.clamp_setup(c), c, "idempotent {body:?} {v}");
            }
        }
        // a single bad value is replaced by the default, the rest is kept
        let mut s = MicSetup::default();
        s.mics[1].yaw = f64::NAN;
        s.mics[0].pos[2] = 0.3;
        let c = Body::Grand.clamp_setup(s);
        assert_eq!(c.mics[1].yaw, MicSetup::default().mics[1].yaw);
        assert_eq!(c.mics[0].pos[2], 0.3);
    }

    #[test]
    fn the_grand_ignores_the_wall_and_the_upright_keeps_it() {
        let a = MicSetup { wall: 0.4, ..MicSetup::default() };
        let b = MicSetup { wall: 1.7, ..MicSetup::default() };
        assert_eq!(Body::Grand.clamp_setup(a), Body::Grand.clamp_setup(b));
        assert_eq!(Body::Grand.clamp_setup(a).wall, U_WALL);
        assert_eq!(Body::Grand.wall_of(MicPosition::Tail), U_WALL);
        assert_eq!(Body::Upright.clamp_setup(a).wall, 0.4);
        assert_eq!(Body::Upright.clamp_setup(b).wall, 1.7);
        assert_eq!(Body::Upright.wall_of(MicPosition::Tail), U_WALL_BACK);
    }

    #[test]
    fn clamp_moves_impossible_points_to_allowed_ones() {
        let g = Body::Grand;
        // under the floor, far away
        let q = g.clamp_mic(0.0, 1.0, [100.0, -100.0, -5.0]);
        assert_eq!(q[0], MAX_DISTANCE);
        assert_eq!(q[1], -MAX_DISTANCE);
        assert!(q[2] > -BOARD_HEIGHT);
        // inside the case: above the board and under the lid, never above the lid
        for x in [-0.5, 0.0, 0.5] {
            let low = g.clamp_mic(0.0, 1.0, [x, 0.0, -0.3]);
            let high = g.clamp_mic(0.0, 1.0, [x, 0.0, 2.0]);
            assert_eq!(low[2], CLOSE_MIN_HEIGHT);
            assert!(high[2] <= lid_height(x, 1.0) - CLOSE_LID_CLEARANCE + 1e-12, "{x}: {high:?}");
        }
        // the upright: not inside the case, not behind the wall
        let u = Body::Upright;
        let inside = u.clamp_mic(U_WALL, 1.0, [0.0, -0.3, 0.0]);
        assert!(!(u.over_case(inside) && inside[2] < U_CASE_TOP), "{inside:?}");
        let behind = u.clamp_mic(U_WALL, 1.0, [0.0, 5.0, 0.3]);
        assert!(behind[1] <= U_WALL - WALL_CLEARANCE + 1e-12, "{behind:?}");
        let above = u.clamp_mic(U_WALL, 1.0, [0.0, -0.3, U_CASE_TOP + 0.3]);
        assert_eq!(above, [0.0, -0.3, U_CASE_TOP + 0.3], "above the open top is fine");
    }

    #[test]
    fn clamp_is_idempotent() {
        for body in [Body::Grand, Body::Upright] {
            for wall in [U_WALL, 1.2] {
                for i in 0..400 {
                    let f = |k: usize| ((i * 7919 + k * 104_729) % 2001) as f64 / 1000.0 - 1.0;
                    let p = [f(1) * 25.0, f(2) * 25.0, f(3) * 4.0];
                    let q = body.clamp_mic(wall, 1.0, p);
                    assert_eq!(body.clamp_mic(wall, 1.0, q), q, "{body:?} {wall} {p:?}");
                }
            }
        }
    }

    #[test]
    fn coincident_microphones_give_finite_radiation() {
        let board = test_board();
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
        for body in [Body::Grand, Body::Upright] {
            let m = Mic::toward_board(if body == Body::Grand { [0.0, -1.5, 0.45] } else { [0.0, -1.5, 0.6] });
            let z = zones(body, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &MicSetup { mics: [m, m], wall: U_WALL, lid: 1.0 }, false);
            assert!(z.room_gain.is_finite() && z.room_gain > 0.0);
            assert!(z.body.iter().flatten().flatten().flatten().all(|v| v.is_finite()), "{body:?}");
        }
    }

    fn golden_cases() -> Vec<(Body, MicPosition, f64, f64)> {
        let mut cases = Vec::new();
        for body in [Body::Grand, Body::Upright] {
            for pos in [MicPosition::Player, MicPosition::Audience, MicPosition::Hammers, MicPosition::Tail] {
                for (spacing, distance) in [(0.5, 0.0), (0.8, 1.3)] {
                    cases.push((body, pos, spacing, distance));
                }
            }
        }
        cases
    }

    /// pinned before the free-placement refactor,
    /// re-pinned when a pair's channels came to one level and the lid's reflection to its size (step 187)
    const GOLDEN: &[[f64; 3]] = &[
    [1.2683499614322067e4, 2.7454567050390656e3, 1.305581496512314e0], // Grand Player 0.5 0
    [1.3421871243372521e4, 4.8828173199682415e3, 1.4449361007898482e0], // Grand Player 0.8 1.3
    [1.2833848998826821e4, -3.6601043922537074e3, 2.2804496865416963e0], // Grand Audience 0.5 0
    [1.2912797135916433e4, -1.686974416198829e3, 1.0831819775958167e0], // Grand Audience 0.8 1.3
    [1.2663171687997201e4, -5.989653037382303e3, 5.545846214478761e-1], // Grand Hammers 0.5 0
    [1.2479957541922615e4, -1.955480474662312e2, 7.430871278410738e-1], // Grand Hammers 0.8 1.3
    [1.3038380912048171e4, 1.5436313353612036e3, 3.623611249909795e-1], // Grand Tail 0.5 0
    [1.3107009019690167e4, -4.046813317222386e3, 5.4756160247404e-1], // Grand Tail 0.8 1.3
    [1.294432709345621e4, -2.9430154313686985e3, 1.9692259679774242e0], // Upright Player 0.5 0
    [1.2917812373407602e4, -1.374737007260587e4, 2.629850276383382e0], // Upright Player 0.8 1.3
    [1.2107377180408206e4, -1.6879520241688533e3, 6.096020436325113e0], // Upright Audience 0.5 0
    [1.3111288957962608e4, -3.5078863052976108e3, 2.5233948570487823e0], // Upright Audience 0.8 1.3
    [1.2961689079694048e4, -6.047277203197568e3, 6.720480308616092e-1], // Upright Hammers 0.5 0
    [1.2790863776729528e4, 5.298237332076422e3, 1.2521028274076624e0], // Upright Hammers 0.8 1.3
    [1.305776686960492e4, -4.771716307999735e3, 6.985642678333562e-1], // Upright Tail 0.5 0
    [1.3001667147425254e4, -4.0755658083573117e3, 1.059881191002203e0], // Upright Tail 0.8 1.3
];

    #[test]
    fn presets_keep_their_radiation() {
        let cases = golden_cases();
        if GOLDEN.is_empty() {
            for &(b, p, s, d) in &cases {
                let g = zones_summary(b, p, s, d);
                println!("    [{:e}, {:e}, {:e}], // {b:?} {p:?} {s} {d}", g[0], g[1], g[2]);
            }
            panic!("GOLDEN is empty: paste the printed rows into it");
        }
        assert_eq!(GOLDEN.len(), cases.len());
        for (&(b, p, s, d), g) in cases.iter().zip(GOLDEN) {
            let got = zones_summary(b, p, s, d);
            for k in 0..3 {
                assert!((got[k] - g[k]).abs() <= 1e-9 * g[k].abs().max(1e-12), "{b:?} {p:?} {s} {d} [{k}]: {} vs {}", got[k], g[k]);
            }
        }
    }

    fn mic_at(pattern: f64, yaw: f64, pitch: f64) -> Mic {
        Mic { pos: [0.0; 3], pattern, yaw, pitch }
    }

    #[test]
    fn aim_convention() {
        let near = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-12);
        assert!(near(mic_at(0.0, 0.0, 0.0).aim(), [0.0, 1.0, 0.0]), "yaw 0 along +y");
        assert!(near(mic_at(0.0, 90.0, 0.0).aim(), [1.0, 0.0, 0.0]), "positive yaw towards +x");
        assert!(near(mic_at(0.0, 0.0, 90.0).aim(), [0.0, 0.0, 1.0]), "pitch up");
        // toward_board: a mic in front of the keyboard looks into the piano
        let m = Mic::toward_board([0.0, -2.0, 0.0]);
        assert!(near(m.aim(), [0.0, 1.0, 0.0]) && m.pattern == 0.0);
        assert!(Mic::toward_board([0.0; 3]).aim().iter().all(|v| v.is_finite()));
    }

    #[test]
    fn pattern_gain() {
        let front = [0.0, 1.0, 0.0];
        let side = [1.0, 0.0, 0.0];
        let back = [0.0, -1.0, 0.0];
        let eight = mic_at(1.0, 0.0, 0.0);
        assert_eq!((eight.gain(front), eight.gain(back)), (1.0, -1.0));
        assert!(eight.gain(side).abs() < 1e-12);
        let card = mic_at(0.5, 0.0, 0.0);
        assert_eq!(card.gain(front), 1.0);
        assert!(card.gain(back).abs() < 1e-12);
        assert!((card.gain(side) - 0.5).abs() < 1e-12);
        let omni = mic_at(0.0, 123.0, 45.0);
        for d in [front, side, back] {
            assert_eq!(omni.gain(d), 1.0);
        }
        let hyper = mic_at(0.75, 0.0, 0.0);
        assert!((hyper.gain(back) + 0.5).abs() < 1e-12, "a small rear lobe in antiphase");
    }

    #[test]
    fn blumlein_pair_tracks_the_source_side() {
        // two figure-8s at ±45°: a source 30° to the left is heard by the left one far more
        let (l, r) = (mic_at(1.0, -45.0, 0.0), mic_at(1.0, 45.0, 0.0));
        let az = (-30f64).to_radians(); // from +y towards −x
        let arrive = [az.sin(), az.cos(), 0.0];
        assert!(l.gain(arrive) > 0.9 && r.gain(arrive).abs() < 0.3, "{} {}", l.gain(arrive), r.gain(arrive));
    }

    #[test]
    fn diffuse_field_sensitivity_and_coupling() {
        let setup = |a: Mic, b: Mic| MicSetup { mics: [a, b], wall: U_WALL, lid: 1.0 };
        let (s, c) = setup(mic_at(0.0, 0.0, 0.0), mic_at(0.0, 0.0, 0.0)).diffuse();
        assert_eq!((s, c), ([1.0, 1.0], 1.0), "omni pair");
        let (s, c) = setup(mic_at(1.0, -45.0, 0.0), mic_at(1.0, 45.0, 0.0)).diffuse();
        assert!(c.abs() < 1e-12, "Blumlein: no diffuse coherence");
        assert!((s[0] - (1.0f64 / 3.0).sqrt()).abs() < 1e-12);
        let (_, c) = setup(mic_at(0.5, -45.0, 0.0), mic_at(0.5, 45.0, 0.0)).diffuse();
        assert!((c - 0.75).abs() < 1e-12, "coincident cardioids at 90°: {c}");
        let (_, c) = setup(mic_at(1.0, 0.0, 0.0), mic_at(1.0, 180.0, 0.0)).diffuse();
        assert!((c + 1.0).abs() < 1e-12, "back to back figure-8s are in antiphase");
    }

    #[test]
    fn arrival_directions_point_at_the_source() {
        let src = Source { pos: [0.3, 0.2, 0.0], normal: [0.0, 0.0, 1.0], gain: 1.0, lowpass_hz: None, one_sided: false, through_lid: false };
        let mic = [-0.25, -1.25, 0.45];
        let refl = Body::Grand.reflectors(0.0, 1.0);
        let p = paths(&src, mic, &refl, false);
        let want = unit(sub(src.pos, mic));
        assert!(p[0].arrive.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-12));
        for q in &p {
            let n = dot(q.arrive, q.arrive).sqrt();
            assert!((n - 1.0).abs() < 1e-9, "unit");
        }
        // the floor reflection comes from below the horizon
        let floor = p.iter().find(|q| q.via == Some(GRAND_FLOOR)).unwrap();
        assert!(floor.arrive[2] < 0.0);
        // a microphone on top of the source: finite, not NaN
        assert!(unit([0.0; 3]).iter().all(|v| v.is_finite()));
    }

    fn pattern_energy(body: Body, mics: [Mic; 2]) -> f64 {
        let board = test_board();
        let h = (vec![20f64.log2(), 20000f64.log2()], vec![0.0, 0.0]);
        let ir: Vec<Vec<f32>> = (0..2).map(|c| (0..4800).map(|i| ((i * 7 + c) % 13) as f32 / 13.0 - 0.5).collect()).collect();
        let z = zones(body, &board, &h, [&ir, &ir], &[1.0, 1.0], 48000.0, 48000.0, &MicSetup { mics, wall: U_WALL, lid: 1.0 }, false);
        assert!(z.body.iter().flatten().flatten().flatten().all(|v| v.is_finite()));
        z.body.iter().flatten().flatten().flatten().map(|&v| (v as f64).powi(2)).sum()
    }

    #[test]
    fn a_cardioid_aimed_away_is_much_quieter_and_the_level_match_ignores_patterns() {
        let at = |pattern: f64, extra_yaw: f64| {
            let aim = |x: f64| {
                let m = Mic::toward_board([x, -1.25, 0.45]);
                Mic { pattern, yaw: m.yaw + extra_yaw, ..m }
            };
            [aim(-0.25), aim(0.25)]
        };
        let omni = pattern_energy(Body::Grand, at(0.0, 0.0));
        let toward = pattern_energy(Body::Grand, at(0.5, 0.0));
        let away = pattern_energy(Body::Grand, at(0.5, 180.0));
        assert!(away < toward / 10.0, "away {away} toward {toward}");
        // pointed at the piano a cardioid is about as loud as an omni (direct sound has gain 1 on axis)
        assert!(toward > omni * 0.3 && toward < omni * 1.5, "toward {toward} omni {omni}");
        // both microphones aimed away: nearly silent, but finite
        assert!(away.is_finite() && away >= 0.0);
        // omni with any aim is the omni radiation
        let spun = pattern_energy(Body::Grand, [Mic { yaw: 77.0, pitch: 20.0, ..Mic::toward_board([-0.25, -1.25, 0.45]) }, Mic::toward_board([0.25, -1.25, 0.45])]);
        assert_eq!(spun, omni);
    }
}

#[cfg(test)]
mod lift_tests {
    use super::*;

    #[test]
    fn the_uprights_hammers_pair_rises_and_settles_without_a_step() {
        let mut last = u_lid_lift(U_HAMMER_Y, 0.0);
        for i in 1..=1000 {
            let lid = i as f64 / 1000.0;
            let z = u_lid_lift(U_HAMMER_Y, lid);
            assert!((z - last).abs() < 0.02, "step of {} m at lid {lid}", z - last);
            last = z;
        }
    }
}
