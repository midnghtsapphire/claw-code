from __future__ import annotations

import unittest

from src.models import (
    PermissionDenial,
    PortingBacklog,
    PortingModule,
    Subsystem,
    UsageSummary,
)


class TestPortingModule(unittest.TestCase):
    def test_creation_with_all_fields(self) -> None:
        m = PortingModule(
            name='TestTool',
            responsibility='A test tool module',
            source_hint='tools/TestTool/TestTool.tsx',
            status='mirrored',
        )
        self.assertEqual(m.name, 'TestTool')
        self.assertEqual(m.responsibility, 'A test tool module')
        self.assertEqual(m.source_hint, 'tools/TestTool/TestTool.tsx')
        self.assertEqual(m.status, 'mirrored')

    def test_equality_same_fields(self) -> None:
        a = PortingModule(name='Foo', responsibility='r', source_hint='s', status='mirrored')
        b = PortingModule(name='Foo', responsibility='r', source_hint='s', status='mirrored')
        self.assertEqual(a, b)

    def test_inequality_different_name(self) -> None:
        a = PortingModule(name='Foo', responsibility='r', source_hint='s', status='mirrored')
        b = PortingModule(name='Bar', responsibility='r', source_hint='s', status='mirrored')
        self.assertNotEqual(a, b)

    def test_repr_contains_name(self) -> None:
        m = PortingModule(name='ReprTool', responsibility='r', source_hint='s', status='stub')
        self.assertIn('ReprTool', repr(m))

    def test_status_field_is_preserved(self) -> None:
        for status in ('mirrored', 'stub', 'partial', 'pending'):
            m = PortingModule(name='X', responsibility='r', source_hint='s', status=status)
            self.assertEqual(m.status, status)


class TestSubsystem(unittest.TestCase):
    def test_creation(self) -> None:
        s = Subsystem(name='assistant', path='src/assistant', file_count=5, notes='core assistant module')
        self.assertEqual(s.name, 'assistant')
        self.assertEqual(s.path, 'src/assistant')
        self.assertEqual(s.file_count, 5)
        self.assertEqual(s.notes, 'core assistant module')

    def test_file_count_is_integer(self) -> None:
        s = Subsystem(name='utils', path='src/utils', file_count=42, notes='')
        self.assertIsInstance(s.file_count, int)
        self.assertGreaterEqual(s.file_count, 0)

    def test_equality(self) -> None:
        a = Subsystem(name='X', path='p', file_count=1, notes='n')
        b = Subsystem(name='X', path='p', file_count=1, notes='n')
        self.assertEqual(a, b)


class TestUsageSummary(unittest.TestCase):
    def test_token_counts(self) -> None:
        u = UsageSummary(input_tokens=100, output_tokens=50)
        self.assertEqual(u.input_tokens, 100)
        self.assertEqual(u.output_tokens, 50)

    def test_zero_tokens(self) -> None:
        u = UsageSummary(input_tokens=0, output_tokens=0)
        self.assertEqual(u.input_tokens, 0)
        self.assertEqual(u.output_tokens, 0)

    def test_large_token_counts(self) -> None:
        u = UsageSummary(input_tokens=1_000_000, output_tokens=500_000)
        self.assertGreater(u.input_tokens, u.output_tokens)


class TestPermissionDenial(unittest.TestCase):
    def test_creation(self) -> None:
        d = PermissionDenial(tool_name='write_file', reason='read-only mode')
        self.assertEqual(d.tool_name, 'write_file')
        self.assertEqual(d.reason, 'read-only mode')

    def test_reason_is_non_empty(self) -> None:
        d = PermissionDenial(tool_name='bash', reason='destructive command blocked')
        self.assertTrue(d.reason)

    def test_equality(self) -> None:
        a = PermissionDenial(tool_name='edit_file', reason='workspace boundary')
        b = PermissionDenial(tool_name='edit_file', reason='workspace boundary')
        self.assertEqual(a, b)


class TestPortingBacklog(unittest.TestCase):
    def test_empty_modules(self) -> None:
        b = PortingBacklog(title='Empty Sprint', modules=[])
        self.assertEqual(b.title, 'Empty Sprint')
        self.assertEqual(b.modules, [])

    def test_with_modules(self) -> None:
        m1 = PortingModule(name='A', responsibility='r', source_hint='s', status='stub')
        m2 = PortingModule(name='B', responsibility='r', source_hint='s', status='mirrored')
        b = PortingBacklog(title='Sprint 1', modules=[m1, m2])
        self.assertEqual(len(b.modules), 2)
        self.assertEqual(b.modules[0].name, 'A')

    def test_title_is_string(self) -> None:
        b = PortingBacklog(title='Test', modules=[])
        self.assertIsInstance(b.title, str)


if __name__ == '__main__':
    unittest.main()
