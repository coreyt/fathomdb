from fathomdb import Engine

# 0.8.20 (R-20-E3): `source_id` is mandatory on every canonical write.
_SOURCE_ID = "py-test:scaffold"



def test_cursor_advances_on_write(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        receipt = engine.write([{"kind": "doc", "body": "{}", "source_id": _SOURCE_ID}])
        assert receipt.cursor == 1
    finally:
        engine.close()
