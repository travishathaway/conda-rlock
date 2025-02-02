from conda.plugins import hookimpl, CondaSubcommand

from .conda_rlock import lock_prefix


@hookimpl
def conda_subcommands():
    def main(argv):
        if len(argv) > 0:
            lock_prefix(argv[0], "./rlock.yaml")

    yield CondaSubcommand("rlock", "subcommand for interacting with rlock", main)
