from fathomdb import Engine


def test_cursor_advances_on_write() -> None:
    engine = Engine.open("rewrite.sqlite")

    receipt = engine.write([{"kind": "doc"}])

    assert receipt.cursor == 1
