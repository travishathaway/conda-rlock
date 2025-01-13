from .conda_rlock import sum_as_string

from conda.plugins import hookimpl, CondaSubcommand


@hookimpl
def conda_subcommands():
    def main(argv):
        print(sum_as_string(1, 2))
        print("conda rlock subcommand")

    yield CondaSubcommand(
        "rlock",
        "subcommand for interacting with rlock",
        main
    )