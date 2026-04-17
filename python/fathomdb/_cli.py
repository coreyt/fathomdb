import json
import sys

import click

from fathomdb import RebuildImpactError


def _open_engine(db: str):
    """Open a fathomdb Engine at the given path."""
    from fathomdb import Engine

    return Engine.open(db)


@click.group()
def cli():
    """FathomDB CLI."""


@cli.group()
def admin():
    """Database administration commands."""


@admin.command("configure-fts")
@click.option("--db", required=True, help="Path to database file")
@click.option("--kind", required=True, help="Node kind")
@click.option("--tokenizer", required=True, help="Tokenizer name or preset")
@click.option("--agree-to-rebuild-impact", is_flag=True, default=False)
def configure_fts(db, kind, tokenizer, agree_to_rebuild_impact):
    """Set the FTS tokenizer profile for a node kind."""
    engine = _open_engine(db)
    try:
        profile = engine.admin.configure_fts(
            kind,
            tokenizer,
            agree_to_rebuild_impact=agree_to_rebuild_impact,
        )
        click.echo(json.dumps({"kind": profile.kind, "tokenizer": profile.tokenizer}))
    except RebuildImpactError as e:
        report = e.report
        if not agree_to_rebuild_impact:
            if sys.stdin.isatty():
                click.echo(
                    f"Rebuild required: {report.rows_to_rebuild} rows "
                    f"(~{report.estimated_seconds}s). Proceed? [y/N] ",
                    nl=False,
                )
                answer = click.getchar()
                click.echo()
                if answer.lower() != "y":
                    click.echo("Aborted.")
                    raise SystemExit(1)
                # retry with agree
                profile = engine.admin.configure_fts(
                    kind,
                    tokenizer,
                    agree_to_rebuild_impact=True,
                )
                click.echo(
                    json.dumps({"kind": profile.kind, "tokenizer": profile.tokenizer})
                )
            else:
                click.echo(
                    f"Aborted: rebuild required for {report.rows_to_rebuild} rows. "
                    "Pass --agree-to-rebuild-impact to proceed.",
                    err=True,
                )
                raise SystemExit(1)


@admin.command("configure-vec")
@click.option("--db", required=True)
@click.option(
    "--embedder", required=True, help="Embedder preset name or model identity"
)
@click.option("--agree-to-rebuild-impact", is_flag=True, default=False)
def configure_vec(db, embedder, agree_to_rebuild_impact):
    """Set the vector embedding profile from an embedder preset."""
    engine = _open_engine(db)
    embedder_obj = _resolve_embedder(embedder)
    try:
        profile = engine.admin.configure_vec(
            embedder_obj,
            agree_to_rebuild_impact=agree_to_rebuild_impact,
        )
        click.echo(
            json.dumps(
                {
                    "model_identity": profile.model_identity,
                    "dimensions": profile.dimensions,
                }
            )
        )
    except RebuildImpactError as e:
        report = e.report
        if sys.stdin.isatty():
            click.echo(
                f"Rebuild required: {report.rows_to_rebuild} rows "
                f"(~{report.estimated_seconds}s). Proceed? [y/N] ",
                nl=False,
            )
            answer = click.getchar()
            click.echo()
            if answer.lower() != "y":
                click.echo("Aborted.")
                raise SystemExit(1)
            profile = engine.admin.configure_vec(
                embedder_obj,
                agree_to_rebuild_impact=True,
            )
            click.echo(
                json.dumps(
                    {
                        "model_identity": profile.model_identity,
                        "dimensions": profile.dimensions,
                    }
                )
            )
        else:
            click.echo(
                f"Aborted: rebuild required for {report.rows_to_rebuild} rows. "
                "Pass --agree-to-rebuild-impact to proceed.",
                err=True,
            )
            raise SystemExit(1)


@admin.command("preview-impact")
@click.option("--db", required=True)
@click.option("--kind", required=True)
@click.option("--target", required=True, type=click.Choice(["fts", "vec"]))
def preview_impact(db, kind, target):
    """Preview the rebuild impact for a projection change."""
    engine = _open_engine(db)
    report = engine.admin.preview_projection_impact(kind, target)
    click.echo(
        json.dumps(
            {
                "rows_to_rebuild": report.rows_to_rebuild,
                "estimated_seconds": report.estimated_seconds,
                "temp_db_size_bytes": report.temp_db_size_bytes,
                "current_tokenizer": report.current_tokenizer,
                "target_tokenizer": report.target_tokenizer,
            }
        )
    )


@admin.command("get-fts-profile")
@click.option("--db", required=True)
@click.option("--kind", required=True)
def get_fts_profile(db, kind):
    """Print the FTS profile for a node kind."""
    engine = _open_engine(db)
    profile = engine.admin.get_fts_profile(kind)
    if profile is None:
        click.echo(f"No FTS profile configured for kind '{kind}'")
    else:
        click.echo(json.dumps({"kind": profile.kind, "tokenizer": profile.tokenizer}))


@admin.command("get-vec-profile")
@click.option("--db", required=True)
@click.option("--kind", required=True, help="Node kind to look up the vec profile for")
def get_vec_profile(db, kind):
    """Print the vector embedding profile for a given node kind."""
    engine = _open_engine(db)
    profile = engine.admin.get_vec_profile(kind)
    if profile is None:
        click.echo("No vec profile configured")
    else:
        click.echo(
            json.dumps(
                {
                    "model_identity": profile.model_identity,
                    "dimensions": profile.dimensions,
                }
            )
        )


def _resolve_embedder(name: str):
    """Resolve CLI embedder name to a QueryEmbedder for configure-vec."""
    from fathomdb.embedders import BuiltinEmbedder, EmbedderIdentity, QueryEmbedder

    # Builtin Candle/BGE-small — use BuiltinEmbedder for exact identity match
    _BUILTIN_ALIASES = {"bge-small-en-v1.5", "BAAI/bge-small-en-v1.5"}
    if name in _BUILTIN_ALIASES:
        return BuiltinEmbedder()

    # Known preset dimensions (models that produce L2-normalized vectors)
    _L2_PRESETS: dict[str, int] = {
        "bge-base-en-v1.5": 768,
        "bge-large-en-v1.5": 1024,
        "text-embedding-3-small": 1536,
        "text-embedding-3-large": 3072,
        "jina-embeddings-v2-base-en": 768,
    }
    dimensions = _L2_PRESETS.get(name, 384)

    class _StubEmbedder(QueryEmbedder):
        def identity(self) -> EmbedderIdentity:
            return EmbedderIdentity(
                model_identity=name,
                model_version=None,
                dimensions=dimensions,
                normalization_policy="l2",
            )

        def embed(self, text: str) -> list[float]:
            return [0.0] * dimensions

    return _StubEmbedder()


def main():
    cli()
