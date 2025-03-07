from conda.plugins import hookimpl, CondaSubcommand

from . import cli


@hookimpl
def conda_subcommands():
    yield CondaSubcommand(
        "rlock",
        "Lock files using the rattler lock format",
        lambda args: cli.rlock(args=args, prog_name="conda rlock"),
    )
