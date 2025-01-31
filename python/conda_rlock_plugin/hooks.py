from conda.base.context import context
from conda.plugins import hookimpl, CondaSubcommand

from .conda_rlock import lock_prefix


@hookimpl
def conda_subcommands():
    def main(argv):
        print(argv)
        if len(argv) > 0:
            lock_prefix(argv[0])

    yield CondaSubcommand(
        "rlock",
        "subcommand for interacting with rlock",
        main
    )
