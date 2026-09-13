"""Run with: uv run --with tiktoken==0.14.0 python tests/test_measure_context.py."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "measure_context", Path(__file__).resolve().parents[1] / "scripts/measure-context.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class CatalogueTest(unittest.TestCase):
    def test_namespaced_names_and_missing_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "prompt.json"
            text = (
                "<skills_instructions>\n"
                "- ecc:context-budget: Description: with punctuation. (file: r0/context-budget/SKILL.md)\n"
                "- plain: (file: r0/plain/SKILL.md)\n"
                "</skills_instructions>"
            )
            path.write_text(json.dumps([{"content": [{"text": text}]}]))
            self.assertEqual(module.catalogue(path)[1], {"ecc:context-budget", "plain"})
            path.write_text("[]")
            with self.assertRaisesRegex(ValueError, "expected one"):
                module.catalogue(path)


if __name__ == "__main__":
    unittest.main()
