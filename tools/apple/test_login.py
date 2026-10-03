"""Offline checks; these never contact Apple or request credentials."""

import json
from pathlib import Path
import tempfile
import unittest

from login import export_session, write_session


class LoginHelperTests(unittest.TestCase):
    def test_export_discards_password_and_other_tokens(self):
        class Account:
            def to_json(self):
                return {
                    "ids": {"uid": "user", "devid": "device"},
                    "account": {"password": "secret-password"},
                    "login": {"data": {"dsid": 123, "mobileme_data": {
                        "tokens": {"searchPartyToken": "report-token", "other": "secret-token"}
                    }}},
                }

        session = export_session(Account())
        self.assertEqual(session, {"dsid": "123", "searchPartyToken": "report-token", "uid": "user", "device_id": "device"})
        self.assertNotIn("secret", json.dumps(session))

    def test_export_does_not_overwrite_existing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "session.json"
            write_session(path, {"dsid": "123"})
            with self.assertRaises(FileExistsError):
                write_session(path, {"dsid": "456"})
            self.assertEqual(json.loads(path.read_text()), {"dsid": "123"})


if __name__ == "__main__":
    unittest.main()
