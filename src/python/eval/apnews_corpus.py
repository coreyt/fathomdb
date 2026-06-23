"""0.8.4 AP-News BenchmarkQED corpus + AutoQ loader (Slice 5, $0 eval-infra).

RED stub — implementation lands in the GREEN commit.
"""

from __future__ import annotations

from dataclasses import dataclass


#: The canonical AutoQ buckets (activity/data × global/local, plus data_linked).
AUTOQ_BUCKETS = (
    "activity_global",
    "activity_local",
    "data_global",
    "data_local",
    "data_linked",
)


class CorpusValidityError(RuntimeError):
    """Raised when the on-disk corpus fails its count / sha256 validity guard."""


@dataclass(frozen=True)
class Article:
    doc_id: str
    title: str
    body: str


def load_articles(root=None, *, verify=True):  # noqa: ANN001, ANN201
    raise NotImplementedError


def load_autoq(root=None):  # noqa: ANN001, ANN201
    raise NotImplementedError


def autoq_coverage(questions):  # noqa: ANN001, ANN201
    raise NotImplementedError
