import struct
import subprocess
from ._base import EmbedderIdentity, QueryEmbedder


class SubprocessEmbedder(QueryEmbedder):
    r"""Query-time embedder that delegates to a persistent subprocess.

    **Wire protocol**: write a UTF-8 line (text + ``\n``) to the
    subprocess stdin; read exactly ``dimensions * 4`` bytes from stdout
    as little-endian ``float32`` values (``struct.unpack(f"{dimensions}f", ...)``).

    The subprocess is started lazily on the first :meth:`embed` call and
    kept alive across calls.  If the subprocess exits unexpectedly,
    :meth:`_ensure_proc` restarts it on the next call.

    **Known limitations**:

    - :meth:`_read_exact` blocks indefinitely if the subprocess stalls
      mid-write.  Callers that require bounded latency should wrap
      :meth:`embed` with their own timeout (e.g. ``concurrent.futures``).
    - Concurrent calls from multiple threads are not safe: two threads
      calling :meth:`embed` simultaneously may interleave stdin writes
      or share stdout reads.

    Parameters
    ----------
    command : list[str]
        The subprocess command and arguments.
    dimensions : int
        Expected output vector dimensionality.
    """

    def __init__(self, command: list[str], dimensions: int) -> None:
        self._command = command
        self._dimensions = dimensions
        self._proc: subprocess.Popen | None = None

    def identity(self) -> EmbedderIdentity:
        return EmbedderIdentity(
            model_identity=" ".join(self._command),
            model_version=None,
            dimensions=self._dimensions,
            normalization_policy="none",
        )

    def _ensure_proc(self) -> subprocess.Popen:
        if self._proc is None or self._proc.poll() is not None:
            self._proc = subprocess.Popen(
                self._command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
            )
        return self._proc

    def _read_exact(self, n: int) -> bytes:
        buf = bytearray()
        proc = self._proc
        while len(buf) < n:
            chunk = proc.stdout.read(n - len(buf))
            if not chunk:
                raise RuntimeError(
                    f"SubprocessEmbedder: subprocess closed stdout after "
                    f"{len(buf)} bytes (expected {n})"
                )
            buf.extend(chunk)
        return bytes(buf)

    def embed(self, text: str) -> list[float]:
        proc = self._ensure_proc()
        assert proc.stdin is not None
        assert proc.stdout is not None
        proc.stdin.write((text + "\n").encode("utf-8"))
        proc.stdin.flush()
        data = self._read_exact(self._dimensions * 4)
        return list(struct.unpack(f"{self._dimensions}f", data))
