import struct
import subprocess
from ._base import EmbedderIdentity, QueryEmbedder


class SubprocessEmbedder(QueryEmbedder):
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

    def embed(self, text: str) -> list[float]:
        proc = self._ensure_proc()
        assert proc.stdin is not None
        assert proc.stdout is not None
        proc.stdin.write((text + "\n").encode("utf-8"))
        proc.stdin.flush()
        data = proc.stdout.read(self._dimensions * 4)
        return list(struct.unpack(f"{self._dimensions}f", data))
