"""The phantom partials' level per instrument ([export] phantom): written at the document's top level,
added by the engine to the Wear control's (design 2026-10-20-bass-high-band)."""
import export


def test_no_phantom_doc_without_the_key():
    assert export.phantom_doc({}) is None
    assert export.phantom_doc({"phantom": 0.0}) is None


def test_the_phantom_doc_is_the_level():
    assert export.phantom_doc({"phantom": 0.2}) == 0.2
