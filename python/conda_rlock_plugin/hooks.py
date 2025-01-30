from conda.base.context import context
from conda.plugins import hookimpl, CondaSubcommand

from .conda_rlock import get_conda_packages


@hookimpl
def conda_subcommands():
    def main(argv):
        get_conda_packages(context.active_prefix)

    yield CondaSubcommand(
        "rlock",
        "subcommand for interacting with rlock",
        main
    )
