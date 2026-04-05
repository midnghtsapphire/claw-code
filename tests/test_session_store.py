from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from src.session_store import StoredSession, load_session, save_session


class TestStoredSession(unittest.TestCase):
    def test_creation_and_fields(self) -> None:
        s = StoredSession(
            session_id='abc123',
            messages=('hello', 'world'),
            input_tokens=10,
            output_tokens=5,
        )
        self.assertEqual(s.session_id, 'abc123')
        self.assertEqual(s.messages, ('hello', 'world'))
        self.assertEqual(s.input_tokens, 10)
        self.assertEqual(s.output_tokens, 5)

    def test_is_frozen(self) -> None:
        s = StoredSession(session_id='x', messages=(), input_tokens=0, output_tokens=0)
        with self.assertRaises(Exception):
            s.session_id = 'y'  # type: ignore[misc]

    def test_messages_is_tuple(self) -> None:
        s = StoredSession(session_id='t', messages=('a', 'b', 'c'), input_tokens=1, output_tokens=1)
        self.assertIsInstance(s.messages, tuple)

    def test_equality(self) -> None:
        a = StoredSession(session_id='s', messages=('x',), input_tokens=1, output_tokens=1)
        b = StoredSession(session_id='s', messages=('x',), input_tokens=1, output_tokens=1)
        self.assertEqual(a, b)

    def test_inequality_different_id(self) -> None:
        a = StoredSession(session_id='s1', messages=(), input_tokens=0, output_tokens=0)
        b = StoredSession(session_id='s2', messages=(), input_tokens=0, output_tokens=0)
        self.assertNotEqual(a, b)


class TestSaveAndLoadSession(unittest.TestCase):
    def _make_session(self, session_id: str = 'test-session') -> StoredSession:
        return StoredSession(
            session_id=session_id,
            messages=('user: hello', 'assistant: hi there'),
            input_tokens=42,
            output_tokens=21,
        )

    def test_save_returns_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            session = self._make_session()
            path = save_session(session, directory=Path(tmpdir))
            self.assertIsInstance(path, Path)
            self.assertTrue(path.exists())

    def test_saved_file_has_json_extension(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            session = self._make_session('my-session')
            path = save_session(session, directory=Path(tmpdir))
            self.assertEqual(path.suffix, '.json')
            self.assertIn('my-session', path.name)

    def test_roundtrip_save_and_load(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            original = self._make_session('roundtrip-test')
            save_session(original, directory=Path(tmpdir))
            loaded = load_session('roundtrip-test', directory=Path(tmpdir))
            self.assertEqual(loaded.session_id, original.session_id)
            self.assertEqual(loaded.messages, original.messages)
            self.assertEqual(loaded.input_tokens, original.input_tokens)
            self.assertEqual(loaded.output_tokens, original.output_tokens)

    def test_roundtrip_with_empty_messages(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            original = StoredSession(
                session_id='empty-msgs',
                messages=(),
                input_tokens=0,
                output_tokens=0,
            )
            save_session(original, directory=Path(tmpdir))
            loaded = load_session('empty-msgs', directory=Path(tmpdir))
            self.assertEqual(loaded.messages, ())
            self.assertEqual(loaded.input_tokens, 0)

    def test_roundtrip_with_many_messages(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            msgs = tuple(f'message-{i}' for i in range(100))
            original = StoredSession(
                session_id='many-msgs',
                messages=msgs,
                input_tokens=5000,
                output_tokens=3000,
            )
            save_session(original, directory=Path(tmpdir))
            loaded = load_session('many-msgs', directory=Path(tmpdir))
            self.assertEqual(len(loaded.messages), 100)
            self.assertEqual(loaded.messages[0], 'message-0')
            self.assertEqual(loaded.messages[99], 'message-99')

    def test_save_creates_directory_if_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            nested = Path(tmpdir) / 'deep' / 'nested' / 'dir'
            session = self._make_session('nested-dir-test')
            path = save_session(session, directory=nested)
            self.assertTrue(path.exists())
            self.assertTrue(nested.is_dir())

    def test_load_missing_session_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            with self.assertRaises(Exception):
                load_session('nonexistent-session', directory=Path(tmpdir))

    def test_multiple_sessions_in_same_directory(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            sessions = [self._make_session(f'session-{i}') for i in range(5)]
            for s in sessions:
                save_session(s, directory=Path(tmpdir))
            for s in sessions:
                loaded = load_session(s.session_id, directory=Path(tmpdir))
                self.assertEqual(loaded.session_id, s.session_id)

    def test_token_counts_preserved_with_large_values(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            original = StoredSession(
                session_id='large-tokens',
                messages=('test',),
                input_tokens=999_999,
                output_tokens=123_456,
            )
            save_session(original, directory=Path(tmpdir))
            loaded = load_session('large-tokens', directory=Path(tmpdir))
            self.assertEqual(loaded.input_tokens, 999_999)
            self.assertEqual(loaded.output_tokens, 123_456)


if __name__ == '__main__':
    unittest.main()
