from setuptools import setup

# AC-051b stand-in for `fathomdb-embedder`: pins the upper api version
# (skew vs probe-a → resolver must reject).
setup(
    name="mock-fathomdb-embedder",
    version="0.6.0",
    install_requires=["mock-skew-api==99.99.99"],
)
