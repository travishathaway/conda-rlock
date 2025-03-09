import logging
from pathlib import Path

from conda.plugins import hookimpl, CondaSubcommand, CondaSetting, CondaPostCommand
from conda.common.configuration import PrimitiveParameter
from conda.reporters import get_spinner

from . import cli
from .conda_rlock import lock_prefix

logger = logging.getLogger("conda_rlock")


@hookimpl
def conda_settings():
    """
    Registers settings for the plugin
    """
    yield CondaSetting(
        name="lock_environments",
        description="When true, environments lock file will be created/updated after running commands which modify them",
        parameter=PrimitiveParameter(False, element_type=bool),
    )
    yield CondaSetting(
        name="lock_file_name",
        description="Name of the lock file",
        parameter=PrimitiveParameter("conda.lock", element_type=str),
    )


@hookimpl
def conda_subcommands():
    """
    Registers all subcommands for the plugin
    """
    yield CondaSubcommand(
        name="rlock",
        summary="Lock files using the rattler lock format",
        action=lambda args: cli.rlock(args=args, prog_name="conda rlock"),
    )


@hookimpl
def conda_post_commands():
    """
    Runs the environment locking after install, create, update or remove has been run
    """

    def lock_environment(command: str):
        """
        Locks the environment
        """
        from conda.base.context import context

        if context.plugins.lock_environments and Path(context.target_prefix).exists():
            with get_spinner("Locking environment"):
                try:
                    lock_prefix(
                        str(context.target_prefix), context.plugins.lock_file_name
                    )
                except Exception as exc:
                    # If locking fails, we simply log the error because it's not critical, but users
                    # should know about it to debug any issues.
                    logger.error(f"Error: {exc}")

    yield CondaPostCommand(
        "rlock_post_command",
        action=lock_environment,
        run_for={"install", "create", "update", "remove"},
    )
