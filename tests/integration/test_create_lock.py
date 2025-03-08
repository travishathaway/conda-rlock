from click.testing import CliRunner

from conda_rlock_plugin.cli import rlock


def test_lock_environment(tmp_path, snapshot):
    """
    Makes sure that we can produce a lock file from our `test-data` folder environment
    """
    runner = CliRunner()
    lockfile_path = tmp_path / "lockfile.txt"
    result = runner.invoke(
        rlock, ["test-data/test-install-prefix", "--file", str(lockfile_path)]
    )

    assert result.exit_code == 0
    assert lockfile_path.exists()
    assert lockfile_path.read_text() == snapshot
