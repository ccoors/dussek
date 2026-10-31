import datasets as D
import register


def test_a_set_scaled_per_key_keeps_its_steps_not_its_levels():
    # Grand A: each key scaled by one gain (its v16 peaks at −1.5 dBFS, its layers share one noise
    # floor, the keys' floors 15 dB apart): the steps between its layers are recorded, its level
    # along the keys is not
    assert D.levels("grand_a") == "per_key"
    assert D.level_steps_recorded("grand_a") and not D.levels_recorded("grand_a")
    assert not register.can_fit("grand_a")
    # a set as recorded keeps both; a normalized one neither
    assert D.level_steps_recorded("grand_c") and D.levels_recorded("grand_c")
    assert not D.level_steps_recorded("grand_e") and not D.levels_recorded("grand_e")


def test_the_felt_fit_follows_a_per_key_sets_steps():
    # the level step between dynamics is within a key: Grand A's felt fit takes it, and the treble
    # keys fitted to the step alone (without them its C8 pp and mf rendered too quiet)
    import fit_profile_felt as FP
    assert set(FP.TREBLE_SUPPORT) <= set(FP.wanted_support("grand_a"))
    assert FP.level_steps_from("grand_a") == "grand_a"


def test_a_normalized_set_takes_its_references_steps():
    # Grand E's files are normalized each to its peak, its levels follow Grand B ([uses] levels):
    # the felt fit takes Grand B's recorded steps too, and fits the top treble to them (without them
    # its treble pp rendered 8–24 dB further under its mf than Grand B's recordings)
    import fit_profile_felt as FP
    assert FP.level_steps_from("grand_e") == "grand_b"
    assert set(FP.TREBLE_SUPPORT) <= set(FP.wanted_support("grand_e"))
    steps = FP.level_targets(FP.level_steps_from("grand_e"), [96], [("mf", "pp")])
    assert 5 < steps[96][("mf", "pp")] < 30, steps
