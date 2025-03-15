"""
Holds the CLI extensions to conda
"""

import sys
from pathlib import Path

import click
from conda.base.constants import ROOT_ENV_NAME

from .conda_rlock import lock_prefix


def find_environment(ctx, param, value) -> Path:
    """
    Tries to find an environment when given a prefix or a name
    """
    from conda.base.context import context
    from conda.core.envs_manager import list_all_known_prefixes

    name_mapping = {
        ROOT_ENV_NAME: Path(context.root_prefix),
    }

    for env_dir in context.envs_dirs:
        for path in list_all_known_prefixes():
            path = Path(path)
            if path.parent == Path(env_dir):
                name_mapping[path.name] = path

    # Check fist if it's a named environment
    if value in name_mapping:
        if name_mapping[value].exists():
            return name_mapping[value]

    # Assume it's a path to an environment
    value = Path(value)
    if value.exists():
        return value

    raise click.BadParameter(f"Environment {value} not found")


@click.command()
@click.argument("environment", callback=find_environment)
@click.option(
    "--file", "-f", help="Output file to write the lock to", default="conda.lock"
)
def rlock(environment, file) -> None:
    """
    Locks an environment using the rattler lock format
    """
    try:
        lock_prefix(str(environment), file)
    except Exception as exc:
        click.echo(exc, err=True)
        click.echo("Failed to lock the environment", err=True)
        sys.exit(1)

    click.echo("(r)Locked the environment")
