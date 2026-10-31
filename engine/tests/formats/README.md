# Frozen instrument formats

One export of each released instrument format, as released: `5-bars` (one register, a bar
instrument: Kalimba), `5-strings` (one register, a hammer piano: Grand A, whose fields players' own
pianos carry) and `6-registers` (registers: Harpsichord B). `engine/tests/formats.rs` loads and
plays each. Never refit or re-export them: they are what players' instruments of that format look
like (`docs/compatibility.md`). Each keeps its own `LICENSE.md`.
