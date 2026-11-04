import unittest
from unittest.mock import patch

import admin_server


def listing(*entries):
    blocks = []
    for path, size, attributes in entries:
        blocks.extend(
            [
                f"Path = {path}",
                f"Size = {size}",
                f"Attributes = {attributes}",
                "",
            ]
        )
    return "Archive listing\n----------\n" + "\n".join(blocks)


class ArchiveValidationTests(unittest.TestCase):
    def test_lists_only_managed_samp_files_and_counts_archive_size(self):
        content = listing(("SAMP", 0, "D"), ("SAMP/custom.img", 5, "A"), ("models/gta3.img", 7, "A"))

        managed, unpacked_size = admin_server.parse_archive_listing(content)

        self.assertEqual(managed, [("SAMP/custom.img", 5)])
        self.assertEqual(unpacked_size, 12)

    def test_rejects_path_traversal_before_extraction(self):
        content = listing(("SAMP", 0, "D"), ("../outside.txt", 10, "A"))

        with self.assertRaisesRegex(admin_server.PanelError, "небезопасный путь"):
            admin_server.parse_archive_listing(content)

    def test_rejects_windows_device_names(self):
        content = listing(("SAMP", 0, "D"), ("SAMP/NUL.ide", 1, "A"))

        with self.assertRaisesRegex(admin_server.PanelError, "небезопасный путь"):
            admin_server.parse_archive_listing(content)


class ReleaseMetadataTests(unittest.TestCase):
    @patch.object(admin_server, "current_manifest", return_value={"version": "1.2.0"})
    def test_requires_a_strictly_newer_numeric_version(self, _current_manifest):
        release = admin_server.validate_metadata(
            {"version": "1.3.0", "changelog": "Исправлена карта."}
        )

        self.assertEqual(release["version"], "1.3.0")

    @patch.object(admin_server, "current_manifest", return_value={"version": "1.2.0"})
    def test_rejects_republishing_an_existing_version(self, _current_manifest):
        with self.assertRaisesRegex(admin_server.PanelError, "должна быть новее"):
            admin_server.validate_metadata(
                {"version": "1.2.0", "changelog": "Исправлена карта."}
            )

    def test_rejects_empty_release_notes(self):
        with patch.object(admin_server, "current_manifest", return_value={"version": "1.2.0"}):
            with self.assertRaisesRegex(admin_server.PanelError, "Список изменений"):
                admin_server.validate_metadata({"version": "1.3.0", "changelog": "  "})


if __name__ == "__main__":
    unittest.main()
