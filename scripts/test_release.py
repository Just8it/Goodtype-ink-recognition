"""Small offline checks for packaging trust boundaries."""
import io
from pathlib import Path
import tempfile
import unittest
import zipfile
from acquire_inputs import digest, fetch
from test_package import safe_members


class ReleaseTests(unittest.TestCase):
    def test_cache_is_always_hash_checked(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'asset'
            path.write_bytes(b'abc')
            expected = 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
            self.assertEqual(digest(path), expected)
            fetch('https://unused.invalid', path, expected, 3)
            path.write_bytes(b'abd')
            with self.assertRaisesRegex(ValueError, 'checksum'):
                fetch('https://unused.invalid', path, expected, 3)

    def test_archive_paths_and_duplicates(self):
        for names in [('../escape',),('/absolute',),('C:/drive',),('a\\b',),('A','a')]:
            with self.subTest(names=names), zipfile.ZipFile(io.BytesIO(), 'w') as archive:
                for name in names:
                    info = zipfile.ZipInfo('placeholder')
                    info.filename = name
                    archive.writestr(info, 'x')
                with self.assertRaises(ValueError):
                    safe_members(archive)
        with zipfile.ZipFile(io.BytesIO(), 'w') as archive:
            archive.writestr('package/models/encoder.onnx', 'x')
            self.assertEqual(len(safe_members(archive)),1)


if __name__ == '__main__':
    unittest.main()
