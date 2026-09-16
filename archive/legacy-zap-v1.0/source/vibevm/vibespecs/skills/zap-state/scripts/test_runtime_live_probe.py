"""The reproducible live proof prepares without invoking either provider lane."""
from pathlib import Path
import tempfile
import unittest

from zaplib.runtime_live_probe import prepare_probe


class RuntimeLiveProbeTests(unittest.TestCase):
    def test_prepare_is_idempotent_and_never_starts_worker_or_semantic_transport(self):
        with tempfile.TemporaryDirectory(prefix="zap-live-probe-prepare-") as temporary:
            root = Path(temporary) / "probe"
            first = prepare_probe(root); second = prepare_probe(root)
            self.assertEqual(first["base_sha256"], second["base_sha256"])
            self.assertEqual(first["revision"], second["revision"])
            self.assertFalse((root / "result.txt").exists())
            self.assertFalse((root / "worker-transport").exists())
            self.assertFalse((root / "semantic-transport").exists())


if __name__ == "__main__":
    unittest.main()
