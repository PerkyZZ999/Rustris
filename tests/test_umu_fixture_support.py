"""Exercise normal and timed-out fixture subprocesses without UMU or Wine."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

from umu_fixture_support import identified_runner_copy, run_fixture


class FixtureSubprocessTest(unittest.TestCase):
    def test_returns_child_exit_status(self):
        command = [sys.executable, '-c', 'raise SystemExit(7)']
        self.assertEqual(run_fixture(command, os.environ.copy(), 5), 7)

    def test_timeout_reaps_child(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / 'pid'
            child = ('import os, pathlib, sys, time; '
                     'pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); time.sleep(30)')
            command = [sys.executable, '-c', child, str(marker)]
            self.assertIsNone(run_fixture(command, os.environ.copy(), 1))
            pid = int(marker.read_text())
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)

    def test_failed_or_mismatched_fixture_reports_are_refused(self):
        with tempfile.TemporaryDirectory(dir='/tmp') as directory:
            root = Path(directory).resolve()
            report_path = root / 'fixture-report.json'
            report = {'root': str(root), 'test_exit': 0, 'source_trees_unchanged': True}
            report_path.write_text(json.dumps(report))
            self.assertTrue(identified_runner_copy(root, root / 'owned'))
            failures = ({'test_exit': None, 'timed_out': True}, {'test_exit': 1},
                        {'source_trees_unchanged': False}, {'root': '/elsewhere'})
            for change in failures:
                report_path.write_text(json.dumps(report | change))
                self.assertFalse(identified_runner_copy(root, root / 'owned'))
            report_path.write_text('{')
            self.assertFalse(identified_runner_copy(root, root / 'owned'))

    def test_successful_owned_fixture_must_be_a_direct_child(self):
        shared_memory = Path('/dev/shm')
        if not shared_memory.is_dir():
            self.skipTest('A non-/tmp temporary directory is unavailable')
        with tempfile.TemporaryDirectory(dir=shared_memory) as directory:
            owned = Path(directory).resolve()
            fixture = owned / 'fixture'
            fixture.mkdir()
            report = {'root': str(fixture), 'test_exit': 0, 'source_trees_unchanged': True}
            (fixture / 'fixture-report.json').write_text(json.dumps(report))
            self.assertTrue(identified_runner_copy(fixture, owned))
            self.assertFalse(identified_runner_copy(fixture, owned / 'other'))


if __name__ == '__main__':
    unittest.main()
