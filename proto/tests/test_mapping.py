"""analysis/mapping.py on small synthetic sets (file names only; no audio is read)."""
import os

import pytest

import mapping


def touch(root, *rels):
    for rel in rels:
        path = os.path.join(root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        open(path, "w").close()


def test_note_names():
    assert [mapping.note_name(n) for n in ("A0", "C4", "D#1", "Bb0", "Cs4", "g3")] == [21, 60, 27, 22, 61, 55]
    with pytest.raises(ValueError):
        mapping.note_name("H4")


def test_pattern_with_takes_layers_and_order(tmp_path):
    root = str(tmp_path)
    touch(root, "h/KSHarp_C4_mf2.wav", "h/KSHarp_C4_mf1.wav", "h/KSHarp_C4_f1.wav", "h/KSHarp_D4_p1.wav",
          "h/KSHarp_C#3_f3.wav", "h/notes.txt")
    src = {"id": "harp", "map": {"kind": "pattern", "glob": "h/KSHarp_*.wav",
                                 "regex": r"KSHarp_(?P<note>[A-G]#?\d)_(?P<dynamic>[a-z]+)(?P<take>\d+)\.wav",
                                 "note": {"group": "note"}, "layers": ["mf", "f"], "prefer": ["take"], "order": "note"}}
    got = mapping.items(src, root)
    assert [(i["note"], i["dynamic"], i["layer"], i["layers"], i["relpath"]) for i in got] == [
        (49, "f", 2, 2, "h/KSHarp_C#3_f3.wav"), (60, "mf", 1, 2, "h/KSHarp_C4_mf1.wav"), (60, "f", 2, 2, "h/KSHarp_C4_f1.wav")]
    assert all(i["tail_cut_s"] == 0.0 and i["path"] == os.path.join(root, i["relpath"]) for i in got)


def test_pattern_relpath_midi_fields_and_layer_order(tmp_path):
    root = str(tmp_path)
    touch(root, "notes/guitar.s2/mf/64.wav", "notes/guitar.s1/mf/64.wav", "notes/guitar.s2/pp/59.wav",
          "notes/guitar.s1/pp/100.wav")
    src = {"id": "g", "map": {"kind": "pattern", "glob": "notes/guitar.s*/*/*.wav", "match": "relpath",
                              "regex": r"notes/guitar\.s(?P<string>\d)/(?P<dynamic>pp|mf|ff)/(?P<midi>\d+)\.wav",
                              "note": {"group": "midi", "style": "midi"}, "layers": ["pp", "mf", "ff"],
                              "prefer": ["string"], "fields": {"string": "int"}, "order": "layer"}}
    got = mapping.items(src, root)
    assert [(i["layer"], i["note"], i["string"]) for i in got] == [(1, 59, 2), (1, 100, 1), (2, 64, 1)]


def test_pattern_constants_index_names_gains_and_source_keys(tmp_path):
    root = str(tmp_path)
    touch(root, "c/01-PedalOffPianissimo1Close.flac", "c/01-PedalOffForte1Close.flac", "c/28-PedalOffForte1Close.flac",
          "c/29-PedalOffForte1Close.flac")
    src = {"id": "p", "relabel": {"48": 47}, "skip": [49], "cut_s": {"47": 1.7}, "gain_db": {"pp": 4.4},
           "extra": {"a4": 415.0}, "map": {
               "kind": "pattern", "glob": "c/*.flac", "regex": r"(?P<index>\d\d)-PedalOff(?P<dynamic>[A-Za-z]+)1Close\.flac",
               "note": {"group": "index", "style": "index", "offset": 20}, "layers": ["Pianissimo", "Forte"],
               "dynamic_names": {"Pianissimo": "pp", "Forte": "f"}}}
    got = mapping.items(src, root)
    assert [(i["note"], i["dynamic"], i["tail_cut_s"], i.get("gain_db"), i["a4"]) for i in got] == [
        (21, "pp", 0.0, 4.4, 415.0), (21, "f", 0.0, None, 415.0), (47, "f", 1.7, None, 415.0)]


def test_pattern_one_layer_and_octave(tmp_path):
    root = str(tmp_path)
    touch(root, "k/sus_C3_r01_main.wav", "k/sus_D3_r01_main.wav")
    src = {"id": "k", "octave": 12, "map": {"kind": "pattern", "glob": "k/*.wav", "regex": r"sus_(?P<note>[A-G]#?\d)_r\d+_main\.wav",
                                            "note": {"group": "note", "shift": 12}, "dynamic": "mf", "order": "path"}}
    got = mapping.items(src, root)
    assert [(i["key"], i["note"], i["layer"], i["layers"], i["dynamic"]) for i in got] == [(60, 72, 1, 1, "mf"), (62, 74, 1, 1, "mf")]


SFZ = r"""
<global> volume=-3
<group> trigger=attack
<region> sample=Sustains\C4_vl1_rr1.wav pitch_keycenter=60 lovel=0 hivel=63
<region> sample=Sustains\C4_vl2_rr1.wav pitch_keycenter=60 lovel=64 hivel=127 tune=5
<region> sample=Sustains\C4_vl2_rr2.wav pitch_keycenter=60 lovel=64 hivel=127 seq_position=2
<region> sample=Other\D4_vl1_rr1.wav pitch_keycenter=62
<group> trigger=release
<region> sample=Rel\C4_rel.wav pitch_keycenter=60
"""


def test_sfz_ranked(tmp_path):
    root = str(tmp_path)
    (tmp_path / "set.sfz").write_text(SFZ)
    m = {"kind": "sfz", "files": ["set.sfz"], "folders": ["Sustains"],
         "filter": [{"opcode": "trigger", "default": "attack", "in": ["attack"]},
                    {"opcode": "seq_position", "default": "1", "in": ["1"]}]}
    got = mapping.items({"id": "s", "map": m}, root)
    assert [(i["note"], i["layer"], i["layers"], i["dynamic"], i["relpath"], i["sfz_volume_db"], i["sfz_tune_cents"],
             i["velocity"]) for i in got] == [
        (60, 1, 2, "1/2", "Sustains/C4_vl1_rr1.wav", -3.0, 0.0, (0, 63)),
        (60, 2, 2, "2/2", "Sustains/C4_vl2_rr1.wav", -3.0, 5.0, (64, 127))]
    got = mapping.items({"id": "s", "map": {**m, "dynamic_from": {"regex": r"_vl?(\d+)_", "format": "v{}"}}}, root)
    assert [i["dynamic"] for i in got] == ["v1", "v2"]


def test_sfz_one_file_per_layer(tmp_path):
    root = str(tmp_path)
    os.makedirs(tmp_path / "Programs" / "maps")
    (tmp_path / "Programs" / "maps" / "vl1.sfz").write_text(
        "<region> sample=../../s/a.wav lokey=60\n<region> sample=../../s/a.wav lokey=61\n<region> sample=../../s/b.wav pitch_keycenter=62\n")
    (tmp_path / "Programs" / "maps" / "vl2.sfz").write_text("<region> sample=../../s/c.wav pitch_keycenter=60\n")
    m = {"kind": "sfz", "files": ["Programs/maps/vl1.sfz", "Programs/maps/vl2.sfz"], "layers": "file",
         "prefix": "Programs/maps/", "normpath": True, "unique": "sample", "dynamic_format": "vl{layer}", "sfz_fields": False}
    got = mapping.items({"id": "o", "map": m}, root)
    assert [(i["note"], i["layer"], i["layers"], i["dynamic"], i["relpath"]) for i in got] == [
        (60, 1, 2, "vl1", "s/a.wav"), (62, 1, 2, "vl1", "s/b.wav"), (60, 2, 2, "vl2", "s/c.wav")]
    assert "velocity" not in got[0]


def test_sfz_key_and_the_control_offsets(tmp_path):
    # key= stands for lokey, hikey and pitch_keycenter; <control>'s octave_offset and
    # note_offset move every key that follows (a set named by its keys two octaves low)
    (tmp_path / "set.sfz").write_text("<control>\ndefault_path=s/\noctave_offset=2\nnote_offset=-1\n"
                                      "<group> lovel=1 hivel=64\n<region> sample=24 p.wav key=24\n"
                                      "<group> lovel=65 hivel=127\n<region> sample=24 f.wav key=24\n"
                                      "<region> sample=30 f.wav pitch_keycenter=30 lokey=29 hikey=31\n")
    r = mapping.parse_sfz(str(tmp_path / "set.sfz"))
    assert [(x["pitch_keycenter"], x["lokey"], x["hikey"]) for x in r] == [("47", "47", "47"), ("47", "47", "47"), ("53", "52", "54")]
    got = mapping.items({"id": "s", "map": {"kind": "sfz", "files": ["set.sfz"], "prefix": "s/"}}, str(tmp_path))
    assert [(i["note"], i["layer"], i["relpath"]) for i in got] == [(47, 1, "s/24 p.wav"), (47, 2, "s/24 f.wav"), (53, 1, "s/30 f.wav")]


def test_sfz_keys_by_note_name(tmp_path):
    # an SFZ may name its keys (c4 = 60); the offsets move them as numbers
    (tmp_path / "set.sfz").write_text("<control> note_offset=1\n<region> sample=a.wav lokey=a0 hikey=a#0 pitch_keycenter=a0\n"
                                      "<region> sample=b.wav key=c4\n")
    r = mapping.parse_sfz(str(tmp_path / "set.sfz"))
    assert [(x["pitch_keycenter"], x["lokey"], x["hikey"]) for x in r] == [("22", "22", "23"), ("61", "61", "61")]


def test_sfz_pitch_keycenter_defaults_to_60(tmp_path):
    # the SFZ default: a file that leaves it out where it is 60 (middle C's regions)
    (tmp_path / "set.sfz").write_text("<region> sample=b.wav lokey=60 hikey=60\n<region> sample=a.wav pitch_keycenter=59\n")
    got = mapping.items({"id": "s", "map": {"kind": "sfz", "files": ["set.sfz"]}}, str(tmp_path))
    assert [(i["note"], i["relpath"]) for i in got] == [(59, "a.wav"), (60, "b.wav")]


def test_unknown_kind(tmp_path):
    with pytest.raises(ValueError, match="kind"):
        mapping.items({"id": "x", "map": {"kind": "runs"}}, str(tmp_path))


def test_a_root_with_brackets(tmp_path):
    # the root is a folder's name, not a pattern: "Set [2]" holds files ("[2]" no character class)
    root = str(tmp_path / "Set [2], A's")
    touch(root, "Harp, Lever/C#4 f.wav", "Harp, Lever/C4 f.wav")
    src = {"id": "b", "map": {"kind": "pattern", "glob": "Harp, Lever/*.wav",
                              "regex": r"(?P<note>[A-G]#?\d) (?P<dynamic>f)\.wav", "note": {"group": "note"}, "layers": ["f"]}}
    assert [(i["note"], i["relpath"]) for i in mapping.items(src, root)] == [
        (60, "Harp, Lever/C4 f.wav"), (61, "Harp, Lever/C#4 f.wav")]


def test_sfz_includes_and_defines(tmp_path):
    # #include pulls in a file (relative to the top file's folder, also from an included one);
    # #define names a value $NAME stands for in what follows (Ethan Winer's sets)
    (tmp_path / "Data").mkdir()
    (tmp_path / "Data" / "ext.txt").write_text('#define $EXT flac')
    (tmp_path / "Data" / "regions.txt").write_text("<region> key=60 sample=G a.$EXT\n#include \"Data/more.txt\"\n")
    (tmp_path / "Data" / "more.txt").write_text("<region> key=62 sample=G b.$EXT // a comment\n")
    (tmp_path / "set.sfz").write_text('// #include "Data/none.txt"\n#include "Data/ext.txt"\n<control>\ndefault_path=s/\n'
                                      '<global> volume=-6\n#include "Data/regions.txt"\n')
    r = mapping.parse_sfz(str(tmp_path / "set.sfz"))
    assert [(x["sample"], x["pitch_keycenter"], x["volume"]) for x in r] == [("G a.flac", "60", "-6"), ("G b.flac", "62", "-6")]
    with pytest.raises(ValueError, match="none.txt"):
        (tmp_path / "bad.sfz").write_text('#include "Data/none.txt"\n')
        mapping.parse_sfz(str(tmp_path / "bad.sfz"))


HARP = {"kind": "pattern", "glob": "h/KSHarp_*.wav",
        "regex": r"KSHarp_(?P<note>[A-G]#?\d)_(?P<dynamic>[a-z]+)(?P<take>\d+)\.wav",
        "note": {"group": "note"}, "layers": ["mf", "f"], "prefer": ["take"], "order": "note"}
RR = ("<region> sample=C4_rr1.wav pitch_keycenter=60 lovel=1 hivel=127 seq_position=1\n"
      "<region> sample=C4_rr2.wav pitch_keycenter=60 lovel=1 hivel=127 seq_position=2\n"
      "<region> sample=C4_rr3.wav pitch_keycenter=60 lovel=1 hivel=127 seq_position=3\n"
      "<region> sample=D4_rr1.wav pitch_keycenter=62 lovel=1 hivel=127 seq_position=1\n")
FIRST = [{"opcode": "seq_position", "default": "1", "in": ["1"]}]


def test_a_left_out_take_gives_way_to_the_spare(tmp_path):
    root = str(tmp_path)
    touch(root, "h/KSHarp_C4_mf1.wav", "h/KSHarp_C4_mf2.wav")
    got = mapping.items({"id": "harp", "map": HARP, "skip_files": ["h/KSHarp_C4_mf1.wav"]}, root)
    assert [i["relpath"] for i in got] == ["h/KSHarp_C4_mf2.wav"]


def test_sfz_a_left_out_round_robin_gives_way_to_the_next(tmp_path):
    root = str(tmp_path)
    (tmp_path / "set.sfz").write_text(RR)
    m = {"kind": "sfz", "files": ["set.sfz"], "filter": FIRST}

    def c4(skip):
        return [i["relpath"] for i in mapping.items({"id": "s", "map": m, "skip_files": skip}, root) if i["note"] == 60]
    assert c4([]) == ["C4_rr1.wav"]
    assert c4(["C4_rr1.wav"]) == ["C4_rr2.wav"]
    assert c4(["C4_rr1.wav", "C4_rr2.wav"]) == ["C4_rr3.wav"]


def test_sfz_lorand_round_robins_are_takes(tmp_path):
    root = str(tmp_path)
    (tmp_path / "set.sfz").write_text("<region> sample=a.wav pitch_keycenter=60 lorand=0 hirand=0.5\n"
                                      "<region> sample=b.wav pitch_keycenter=60 lorand=0.5 hirand=1\n")
    m = {"kind": "sfz", "files": ["set.sfz"], "filter": [{"opcode": "lorand", "default": "0", "eq": 0.0}]}
    assert [i["relpath"] for i in mapping.items({"id": "s", "map": m}, root)] == ["a.wav"]
    assert [i["relpath"] for i in mapping.items({"id": "s", "map": m, "skip_files": ["a.wav"]}, root)] == ["b.wav"]


def test_a_left_out_file_without_a_spare_is_gone(tmp_path):
    root = str(tmp_path)
    touch(root, "C4.wav", "D4.wav", "E4.wav")
    m = {"kind": "pattern", "glob": "*.wav", "regex": r"(?P<note>[A-G]#?\d)\.wav", "note": {"group": "note"},
         "dynamic": "mf", "order": "note"}
    got = mapping.items({"id": "s", "map": m, "skip_files": ["C4.wav"]}, root)
    assert [(i["note"], i["relpath"]) for i in got] == [(62, "D4.wav"), (64, "E4.wav")]


def test_a_group_without_a_first_round_robin_still_maps_nothing(tmp_path):
    root = str(tmp_path)
    (tmp_path / "set.sfz").write_text("<region> sample=C4_rr2.wav pitch_keycenter=60 seq_position=2\n"
                                      "<region> sample=D4_rr1.wav pitch_keycenter=62 seq_position=1\n")
    got = mapping.items({"id": "s", "map": {"kind": "sfz", "files": ["set.sfz"], "filter": FIRST}}, root)
    assert [i["relpath"] for i in got] == ["D4_rr1.wav"]


def test_a_left_out_layer_keeps_the_ranks_above_it(tmp_path):
    # the soft layer left out whole: the loud one stays layer 2 of 2 (it moved down to 1 of 1)
    root = str(tmp_path)
    (tmp_path / "set.sfz").write_text("<region> sample=C4_v1.wav pitch_keycenter=60 lovel=1 hivel=63\n"
                                      "<region> sample=C4_v2.wav pitch_keycenter=60 lovel=64 hivel=127\n")
    got = mapping.items({"id": "s", "map": {"kind": "sfz", "files": ["set.sfz"]}, "skip_files": ["C4_v1.wav"]}, root)
    assert [(i["relpath"], i["layer"], i["layers"], i["dynamic"]) for i in got] == [("C4_v2.wav", 2, 2, "2/2")]
