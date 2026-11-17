"""Regression tests for reproducible corpus accounting, no external dependencies."""
import unittest
from pathlib import Path
from check_book_corpus import source_listings, runtime_status


class CorpusTests(unittest.TestCase):
    def test_replaying_archive_copy_does_not_duplicate_quiz_fragments(self):
        self.assertEqual(source_listings([Path("x.8"), Path("quiz1.txt.part-01.8"),
                                         Path("q.TXT.PART-02.8"), Path("helper.inc")]), [Path("x.8")])

    def test_halt_and_instruction_limit_are_not_semantic_success(self):
        for stderr, expected in [("[halted at 0700:0100]", "halted"),
                                 ("[instruction limit reached]", "instruction_limit"),
                                 ("[input exhausted]", "needs_input")]:
            self.assertEqual(runtime_status(dict(stderr=stderr, timeout=False, code=0)), expected)


if __name__ == "__main__":
    unittest.main()
