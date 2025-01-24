from conda.plugins import hookimpl, CondaSubcommand


@hookimpl
def conda_subcommands():
    def main(argv):
        print("conda rlock subcommand")

    yield CondaSubcommand(
        "rlock",
        "subcommand for interacting with rlock",
        main
    )
