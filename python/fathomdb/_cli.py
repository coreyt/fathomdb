import dataclasses
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


@admin.command("check-integrity")
@click.option("--db", required=True, help="Path to database file")
def check_integrity(db):
    """Run physical and logical integrity checks."""
    engine = _open_engine(db)
    report = engine.admin.check_integrity()
    click.echo(json.dumps(dataclasses.asdict(report)))


@admin.command("check-semantics")
@click.option("--db", required=True, help="Path to database file")
def check_semantics(db):
    """Run semantic consistency checks."""
    engine = _open_engine(db)
    report = engine.admin.check_semantics()
    click.echo(json.dumps(dataclasses.asdict(report)))


@admin.command("rebuild")
@click.option("--db", required=True, help="Path to database file")
@click.option(
    "--target",
    default="all",
    type=click.Choice(["all", "fts", "vec"]),
    help="Which projections to rebuild",
)
def rebuild(db, target):
    """Rebuild projection indexes (FTS, vector, or all)."""
    engine = _open_engine(db)
    report = engine.admin.rebuild(target)
    click.echo(
        json.dumps(
            {
                "targets": [t.value for t in report.targets],
                "rebuilt_rows": report.rebuilt_rows,
                "notes": report.notes,
            }
        )
    )


@admin.command("register-fts-property-schema")
@click.option("--db", required=True, help="Path to database file")
@click.option("--kind", required=True, help="Node kind")
@click.option(
    "--paths", multiple=True, required=True, help="JSON path strings to index"
)
def register_fts_property_schema(db, kind, paths):
    """Register (or update) the FTS property schema for a node kind."""
    engine = _open_engine(db)
    record = engine.admin.register_fts_property_schema(kind, list(paths))
    click.echo(
        json.dumps(
            {
                "kind": record.kind,
                "property_paths": list(record.property_paths),
                "separator": record.separator,
                "format_version": record.format_version,
            }
        )
    )


@admin.command("describe-fts-property-schema")
@click.option("--db", required=True, help="Path to database file")
@click.option("--kind", required=True, help="Node kind")
def describe_fts_property_schema(db, kind):
    """Show the registered FTS property schema for a node kind."""
    engine = _open_engine(db)
    record = engine.admin.describe_fts_property_schema(kind)
    if record is None:
        click.echo(f"No FTS property schema for kind '{kind}'")
    else:
        click.echo(
            json.dumps(
                {
                    "kind": record.kind,
                    "property_paths": list(record.property_paths),
                    "separator": record.separator,
                    "format_version": record.format_version,
                }
            )
        )


@admin.command("list-fts-property-schemas")
@click.option("--db", required=True, help="Path to database file")
def list_fts_property_schemas(db):
    """List all registered FTS property schemas."""
    engine = _open_engine(db)
    records = engine.admin.list_fts_property_schemas()
    click.echo(
        json.dumps(
            [
                {
                    "kind": record.kind,
                    "property_paths": list(record.property_paths),
                    "separator": record.separator,
                    "format_version": record.format_version,
                }
                for record in records
            ]
        )
    )


@admin.command("remove-fts-property-schema")
@click.option("--db", required=True, help="Path to database file")
@click.option("--kind", required=True, help="Node kind")
def remove_fts_property_schema(db, kind):
    """Remove the FTS property schema for a node kind."""
    engine = _open_engine(db)
    engine.admin.remove_fts_property_schema(kind)
    click.echo(f"Removed FTS property schema for kind '{kind}'")


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


@admin.command("restore-vector-profiles")
@click.option("--db", required=True)
def restore_vector_profiles(db):
    """Restore vector profile metadata from the database schema."""
    engine = _open_engine(db)
    report = engine.admin.restore_vector_profiles()
    click.echo(
        json.dumps(
            {
                "targets": [t.value for t in report.targets],
                "rebuilt_rows": report.rebuilt_rows,
                "notes": report.notes,
            }
        )
    )


@admin.command("regen-vectors")
@click.option("--db", required=True)
@click.option("--embedder", required=True, help="Embedder preset or model identity")
@click.option("--kind", required=True, help="Node kind")
@click.option("--profile", default="main")
@click.option("--chunking-policy", default="full")
@click.option("--preprocessing-policy", default="none")
def regen_vectors(db, embedder, kind, profile, chunking_policy, preprocessing_policy):
    """Regenerate vector embeddings for a node kind."""
    from fathomdb import Engine
    from fathomdb._types import VectorRegenerationConfig

    engine = Engine.open(db, embedder=embedder)
    config = VectorRegenerationConfig(
        kind=kind,
        profile=profile,
        chunking_policy=chunking_policy,
        preprocessing_policy=preprocessing_policy,
    )
    report = engine.admin.regenerate_vector_embeddings(config)
    click.echo(json.dumps(dataclasses.asdict(report)))


@admin.command("trace-source")
@click.option("--db", required=True)
@click.option("--source-ref", required=True, help="Source reference URI")
def trace_source(db, source_ref):
    """Trace all nodes, edges, and actions originating from a source reference."""
    engine = _open_engine(db)
    report = engine.admin.trace_source(source_ref)
    click.echo(json.dumps(dataclasses.asdict(report)))


@admin.command("excise-source")
@click.option("--db", required=True)
@click.option("--source-ref", required=True, help="Source reference URI to excise")
def excise_source(db, source_ref):
    """Remove all data originating from a source reference."""
    engine = _open_engine(db)
    report = engine.admin.excise_source(source_ref)
    click.echo(json.dumps(dataclasses.asdict(report)))


@admin.command("restore-logical-id")
@click.option("--db", required=True)
@click.option("--logical-id", required=True)
def restore_logical_id(db, logical_id):
    """Restore a previously retired node by its logical ID."""
    engine = _open_engine(db)
    report = engine.admin.restore_logical_id(logical_id)
    click.echo(json.dumps(dataclasses.asdict(report)))


@admin.command("purge-logical-id")
@click.option("--db", required=True)
@click.option("--logical-id", required=True)
def purge_logical_id(db, logical_id):
    """Permanently delete all rows associated with a logical ID."""
    engine = _open_engine(db)
    report = engine.admin.purge_logical_id(logical_id)
    click.echo(json.dumps(dataclasses.asdict(report)))


@admin.command("safe-export")
@click.option("--db", required=True)
@click.option(
    "--destination", required=True, help="Destination path for the exported database"
)
@click.option(
    "--no-checkpoint", is_flag=True, default=False, help="Skip WAL checkpoint"
)
def safe_export(db, destination, no_checkpoint):
    """Export a consistent snapshot of the database."""
    engine = _open_engine(db)
    report = engine.admin.safe_export(destination, force_checkpoint=not no_checkpoint)
    click.echo(json.dumps(dataclasses.asdict(report)))


def main():
    cli()
