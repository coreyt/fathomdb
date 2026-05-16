from fathomdb import Engine


def test_cursor_advances_on_write(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        receipt = engine.write([{"kind": "doc", "body": "{}"}])
        assert receipt.cursor == 1
    finally:
        engine.close()
