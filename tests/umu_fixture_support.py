"""Bounded subprocess execution for isolated UMU fixture runs."""
import json
import subprocess


def run_fixture(command, env, timeout):
    """Return the exit status, or None after terminating a timed-out fixture."""
    try:
        return subprocess.run(command, env=env, check=False, timeout=timeout).returncode
    except subprocess.TimeoutExpired:
        return None


def identified_runner_copy(inputs, owned_runtime):
    """Accept only a successful identified fixture or an existing owned probe."""
    if inputs.parent == owned_runtime and (inputs / 'probe-report.json').is_file():
        return True
    if not (inputs.is_relative_to('/tmp') or inputs.parent == owned_runtime):
        return False
    try:
        report = json.loads((inputs / 'fixture-report.json').read_text())
    except (OSError, UnicodeError, json.JSONDecodeError):
        return False
    return (
        isinstance(report, dict)
        and report.get('root') == str(inputs)
        and type(report.get('test_exit')) is int
        and report['test_exit'] == 0
        and report.get('timed_out', False) is False
        and report.get('source_trees_unchanged') is True
    )
