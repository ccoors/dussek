//! Render a Standard MIDI file to a stereo WAV with the piano engine: `midi2wav --help`. The code is
//! the library's (`pianoengine::midi2wav`), which the standalone runs as `pianoengine render`.

fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(pianoengine::midi2wav::main(std::env::args_os()) as u8)
}
