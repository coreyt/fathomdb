from setuptools import setup

# AC-051b stand-in for `fathomdb`: pins the lower api version.
setup(
    name="mock-fathomdb",
    version="0.6.0",
    install_requires=["mock-skew-api==0.6.0"],
)
