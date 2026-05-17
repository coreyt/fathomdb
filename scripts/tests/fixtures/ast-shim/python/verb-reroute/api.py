"""Negative fixture for the V05_VERBS re-route rule. `rebuild` was an
AdminClient verb in 0.5.x and has no 0.6.0 equivalent — defining a
public function with that name in 0.6.0 is banned."""


def rebuild():
    pass
