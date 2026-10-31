//! Render a Standard MIDI file to a stereo WAV with Dussek's engine: `midi2wav --help`. The code is
//! the library's (`dussek::midi2wav`), which the standalone runs as `dussek render`.

fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(dussek::midi2wav::main(std::env::args_os()) as u8)
}
