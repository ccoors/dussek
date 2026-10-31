# Lessons

What building this instrument taught about building and judging a modeled instrument, drawn from
the [design log](log/README.md). Each lesson links the steps it comes from, where the log records
it.

## Listen blind, under neutral names

A name carries an expectation: an instrument called after its maker or its recordings is heard
through what one knows of them. The listening rounds that changed the most were blind, with the
stimuli matched in loudness, their onsets aligned and their names neutral; the instruments' own
names went neutral too, and the grands are lettered by how they sound, best first.
Steps [117](log/05-playing-and-new-pianos.md#step-117), [119](log/05-playing-and-new-pianos.md#step-119),
[198](log/05-playing-and-new-pianos.md#step-198).

## Switch variants live, over loud repeated notes

Two variants rendered to files and played one after the other often sound alike: the memory of
the first fades before the second starts. The same two as instruments in the plugin's menu,
switched while an arpeggiator repeats loud notes, are told apart at once, and loud repeats reach
what a change to the strike affects. Snapshots of any git revision in the plugin's menu make this
the normal way to compare.
Steps [135](log/05-playing-and-new-pianos.md#step-135), [136](log/05-playing-and-new-pianos.md#step-136).

## Keep listening rounds small

Long listening dulls the ear: after a day of comparisons, the judgments stop being reliable. A
round of two or three variants and the most telling passages, early in a session, does more than
a long list. A sound question waits for fresh ears rather than being pushed through.

## Know what is below hearing

Changes of about 1 dB in a register's level, or 9–23 % in the pedal resonance's fit, went unheard
in the listening rounds; a coupling change of a few dB after 2 s was heard at once. Measure a change
before asking anyone to listen to it, ask about the ones of a few dB or more, and list the small
ones as probably inaudible rather than as things to check.
Steps [106](log/04-projects-and-editor.md#step-106), [140](log/05-playing-and-new-pianos.md#step-140),
[146](log/05-playing-and-new-pianos.md#step-146).

## The ear outranks the metrics

A fit can match its measure and still sound wrong, because a measure averages what the ear hears
as a pattern: beating too deep, a stripe of noise in the bass, a length judged in the note's own
band. A complaint about the sound becomes a statistic that captures it, then a check, then a fix,
verified on rendered audio. A smooth curve set by ear beat the per-key fit it replaced.
Steps [99](log/04-projects-and-editor.md#step-99), [100](log/04-projects-and-editor.md#step-100),
[128](log/05-playing-and-new-pianos.md#step-128), [134](log/05-playing-and-new-pianos.md#step-134).

## Correct is not default

A change that is more physical is not better until someone has played it. Re-striking a ringing
string where it is in its swing is the more faithful model, and from a keyboard it grew annoying;
it stayed a switch, off. Such changes go in as switches, off by default, until they have been
played.
Steps [170](log/05-playing-and-new-pianos.md#step-170), [171](log/05-playing-and-new-pianos.md#step-171).

## Measure at the velocities a player uses

The engine picks each key's hammer speed from its level table, so a claim about how the sound
changes from soft to loud has to be measured at the velocities a recording's layers stand for,
not at fixed hammer speeds: a first comparison at fixed speeds overstated one grand's change from
mp to f by 11–22 dB. Fit the felt against every layer, soft playing included: fitted from mf and ff
alone, soft playing came out 10–14 dB too bright.
Steps [124](log/05-playing-and-new-pianos.md#step-124), [140](log/05-playing-and-new-pianos.md#step-140),
[141](log/05-playing-and-new-pianos.md#step-141), [142](log/05-playing-and-new-pianos.md#step-142).

## A new sample set exposes old bugs

The well-behaved sets never reach some of the pipeline's assumptions; a sparse, odd or mislabeled
set does. Two sparse sets once needed fixes that turned up a stale cache which had left two older
instruments 2–14 dB off in their output and release levels. When a new set needs a fix, check
whether the same flaw touched the instruments already there, and compare their exports before and
after.
Step [17](log/01-engine.md#step-17). Old flaws turn up the same way when an export is made again:
steps [183](log/05-playing-and-new-pianos.md#step-183) and [200](log/05-playing-and-new-pianos.md#step-200).

## A base plus traits, not a fit per key

Fitted key by key, an instrument records the measurement's noise along with the instrument: per-key
felt cores zig-zagged, and a board refit moved the keys' gains by up to 7 dB for a 0.3 dB change in
the strings. A smooth base across the keys, with the few real breaks (wound strings, the bridges,
the dampers' end), plus the traits that make an instrument itself, plays better. Irregularity
belongs to realism, so it comes back as a statistic with a seed, not as per-key values.
Steps [134](log/05-playing-and-new-pianos.md#step-134), [136](log/05-playing-and-new-pianos.md#step-136),
[139](log/05-playing-and-new-pianos.md#step-139), [148](log/05-playing-and-new-pianos.md#step-148).
