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


def _asdict_json_safe(obj):
    """Recursively convert a dataclass to a JSON-serializable dict, converting enums to .value."""
    import dataclasses
    from enum import Enum

    if dataclasses.is_dataclass(obj) and not isinstance(obj, type):
        return {
            f.name: _asdict_json_safe(getattr(obj, f.name))
            for f in dataclasses.fields(obj)
        }
    if isinstance(obj, Enum):
        return obj.value
    if isinstance(obj, list):
        return [_asdict_json_safe(item) for item in obj]
    if isinstance(obj, tuple):
        return [_asdict_json_safe(item) for item in obj]
    return obj


@admin.command("describe-operational-collection")
@click.option("--db", required=True)
@click.option("--name", required=True, help="Collection name")
def describe_operational_collection(db, name):
    """Show the record for a named operational collection."""
    engine = _open_engine(db)
    record = engine.admin.describe_operational_collection(name)
    if record is None:
        click.echo(f"No collection '{name}'")
    else:
        click.echo(json.dumps(_asdict_json_safe(record)))


@admin.command("register-operational-collection")
@click.option("--db", required=True)
@click.option("--name", required=True)
@click.option(
    "--kind",
    required=True,
    type=click.Choice(["append_only_log", "latest_state"]),
)
@click.option("--schema-json", required=True, help="JSON schema definition")
@click.option("--retention-json", required=True, help="JSON retention policy")
@click.option("--format-version", default=1, type=int)
def register_operational_collection(
    db, name, kind, schema_json, retention_json, format_version
):
    """Register a new operational collection."""
    from fathomdb._types import OperationalCollectionKind, OperationalRegisterRequest

    request = OperationalRegisterRequest(
        name=name,
        kind=OperationalCollectionKind(kind),
        schema_json=schema_json,
        retention_json=retention_json,
        format_version=format_version,
    )
    engine = _open_engine(db)
    result = engine.admin.register_operational_collection(request)
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("trace-operational-collection")
@click.option("--db", required=True)
@click.option("--name", required=True, help="Collection name")
@click.option("--record-key", default=None, help="Narrow to specific record key")
def trace_operational_collection(db, name, record_key):
    """Trace mutation and current-state rows for an operational collection."""
    engine = _open_engine(db)
    result = engine.admin.trace_operational_collection(name, record_key)
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("rebuild-operational-current")
@click.option("--db", required=True)
@click.option("--name", default=None, help="Collection name (omit for all)")
def rebuild_operational_current(db, name):
    """Rebuild current-state views for operational collections."""
    engine = _open_engine(db)
    result = engine.admin.rebuild_operational_current(name)
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("validate-operational-history")
@click.option("--db", required=True)
@click.option("--name", required=True)
def validate_operational_history(db, name):
    """Validate mutation history of an operational collection."""
    engine = _open_engine(db)
    result = engine.admin.validate_operational_collection_history(name)
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("rebuild-operational-secondary-indexes")
@click.option("--db", required=True)
@click.option("--name", required=True)
def rebuild_operational_secondary_indexes(db, name):
    """Rebuild secondary indexes for an operational collection."""
    engine = _open_engine(db)
    result = engine.admin.rebuild_operational_secondary_indexes(name)
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("plan-operational-retention")
@click.option("--db", required=True)
@click.option("--now", required=True, type=int, help="Reference epoch timestamp")
@click.option("--names", multiple=True, default=None, help="Limit to these collections")
def plan_operational_retention(db, now, names):
    """Preview which mutations would be purged by the retention policy."""
    engine = _open_engine(db)
    result = engine.admin.plan_operational_retention(
        now, collection_names=list(names) if names else None
    )
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("run-operational-retention")
@click.option("--db", required=True)
@click.option("--now", required=True, type=int)
@click.option("--dry-run", is_flag=True, default=False)
@click.option("--names", multiple=True, default=None)
def run_operational_retention(db, now, dry_run, names):
    """Execute the retention policy, deleting expired mutations."""
    engine = _open_engine(db)
    result = engine.admin.run_operational_retention(
        now, collection_names=list(names) if names else None, dry_run=dry_run
    )
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("compact-operational-collection")
@click.option("--db", required=True)
@click.option("--name", required=True)
@click.option("--dry-run", is_flag=True, default=False)
def compact_operational_collection(db, name, dry_run):
    """Compact an operational collection by removing superseded mutations."""
    engine = _open_engine(db)
    result = engine.admin.compact_operational_collection(name, dry_run=dry_run)
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("purge-operational-collection")
@click.option("--db", required=True)
@click.option("--name", required=True)
@click.option(
    "--before-timestamp", required=True, type=int, help="Epoch seconds cutoff"
)
def purge_operational_collection(db, name, before_timestamp):
    """Delete all mutations older than before-timestamp from a collection."""
    engine = _open_engine(db)
    result = engine.admin.purge_operational_collection(
        name, before_timestamp=before_timestamp
    )
    click.echo(json.dumps(_asdict_json_safe(result)))


@admin.command("disable-operational-collection")
@click.option("--db", required=True)
@click.option("--name", required=True)
def disable_operational_collection(db, name):
    """Disable an operational collection, preventing new writes."""
    engine = _open_engine(db)
    result = engine.admin.disable_operational_collection(name)
    click.echo(json.dumps(_asdict_json_safe(result)))


def main():
    cli()
