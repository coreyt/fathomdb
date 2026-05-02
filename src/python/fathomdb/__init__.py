from dataclasses import dataclass, field

__all__ = ["Engine", "SearchResult", "WriteReceipt", "__version__"]
__version__ = "0.6.0"


@dataclass
class WriteReceipt:
    cursor: int


@dataclass
class SearchResult:
    projection_cursor: int
    results: list[str] = field(default_factory=list)


@dataclass
class Engine:
    path: str
    _cursor: int = 0
    _closed: bool = False

    @classmethod
    def open(cls, path: str) -> "Engine":
        return cls(path=path)

    def write(self, batch: list[object] | None = None) -> WriteReceipt:
        self._ensure_open()
        self._cursor += max(len(batch or []), 1)
        return WriteReceipt(cursor=self._cursor)

    def search(self, query: str) -> SearchResult:
        self._ensure_open()
        if not query.strip():
            raise ValueError("query must not be empty")
        return SearchResult(
            projection_cursor=self._cursor,
            results=[f"rewrite scaffold query: {query.strip()}"],
        )

    def close(self) -> None:
        self._closed = True

    def _ensure_open(self) -> None:
        if self._closed:
            raise RuntimeError("engine is closed")
