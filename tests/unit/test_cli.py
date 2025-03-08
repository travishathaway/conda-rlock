from conda_rlock_plugin.cli import find_environment


def test_find_environments_find_by_prefix(tmp_path):
    """
    Makes sure that we can find environments by name or prefix
    """
    # Create a fake environment
    env_path = tmp_path / "test-env"
    env_path.mkdir()

    # Test finding by prefix
    assert find_environment(None, None, str(env_path)) == env_path
