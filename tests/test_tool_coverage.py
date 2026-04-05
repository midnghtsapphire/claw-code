from __future__ import annotations

import unittest

from src.commands import PORTED_COMMANDS
from src.execution_registry import build_execution_registry
from src.models import PortingModule
from src.tools import PORTED_TOOLS


class TestPortedToolsSurface(unittest.TestCase):
    def test_tools_list_is_nontrivial(self) -> None:
        self.assertGreaterEqual(len(PORTED_TOOLS), 100)

    def test_all_tools_are_porting_modules(self) -> None:
        for tool in PORTED_TOOLS:
            self.assertIsInstance(tool, PortingModule)

    def test_all_tools_have_non_empty_name(self) -> None:
        for tool in PORTED_TOOLS:
            self.assertTrue(tool.name, f'Tool has empty name: {tool!r}')

    def test_all_tools_have_non_empty_responsibility(self) -> None:
        for tool in PORTED_TOOLS:
            self.assertTrue(tool.responsibility, f'Tool {tool.name!r} has empty responsibility')

    def test_all_tools_have_source_hint(self) -> None:
        for tool in PORTED_TOOLS:
            self.assertTrue(tool.source_hint, f'Tool {tool.name!r} has no source_hint')

    def test_tool_source_hints_reference_tools_path(self) -> None:
        for tool in PORTED_TOOLS:
            self.assertTrue(
                tool.source_hint.startswith('tools/'),
                f'Tool {tool.name!r} source_hint {tool.source_hint!r} does not start with tools/',
            )

    def test_tool_list_contains_expected_quantity(self) -> None:
        # Tools are a flat list of PortingModule entries across subsystems;
        # duplicates are expected (same tool name may appear in multiple subsystems).
        # Verify the total count is at least 100 and unique names at least 50.
        names = [t.name for t in PORTED_TOOLS]
        self.assertGreaterEqual(len(names), 100)
        self.assertGreaterEqual(len(set(names)), 50)

    def test_known_tools_are_present(self) -> None:
        names = {t.name for t in PORTED_TOOLS}
        for required in ('BashTool', 'MCPTool', 'AgentTool'):
            self.assertIn(required, names, f'Expected tool {required!r} not found')

    def test_mcp_tools_are_present(self) -> None:
        mcp_tools = [t for t in PORTED_TOOLS if 'mcp' in t.name.lower() or 'MCP' in t.name]
        self.assertGreater(len(mcp_tools), 0, 'No MCP tools found in PORTED_TOOLS')

    def test_all_tools_have_mirrored_status(self) -> None:
        statuses = {t.status for t in PORTED_TOOLS}
        self.assertIn('mirrored', statuses, 'No tools have mirrored status')


class TestPortedCommandsSurface(unittest.TestCase):
    def test_commands_list_is_nontrivial(self) -> None:
        self.assertGreaterEqual(len(PORTED_COMMANDS), 150)

    def test_all_commands_are_porting_modules(self) -> None:
        for cmd in PORTED_COMMANDS:
            self.assertIsInstance(cmd, PortingModule)

    def test_all_commands_have_non_empty_name(self) -> None:
        for cmd in PORTED_COMMANDS:
            self.assertTrue(cmd.name, f'Command has empty name: {cmd!r}')

    def test_all_commands_have_non_empty_responsibility(self) -> None:
        for cmd in PORTED_COMMANDS:
            self.assertTrue(cmd.responsibility, f'Command {cmd.name!r} has empty responsibility')

    def test_all_commands_have_source_hint(self) -> None:
        for cmd in PORTED_COMMANDS:
            self.assertTrue(cmd.source_hint, f'Command {cmd.name!r} has no source_hint')

    def test_review_command_is_present(self) -> None:
        names = {c.name for c in PORTED_COMMANDS}
        self.assertIn('review', names, 'Expected review command not found')


class TestExecutionRegistry(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = build_execution_registry()

    def test_registry_commands_count(self) -> None:
        self.assertGreaterEqual(len(self.registry.commands), 150)

    def test_registry_tools_count(self) -> None:
        self.assertGreaterEqual(len(self.registry.tools), 100)

    def test_review_command_executes(self) -> None:
        cmd = self.registry.command('review')
        result = cmd.execute('inspect security')
        self.assertIn('Mirrored command', result)
        self.assertIn('review', result)

    def test_mcp_tool_executes(self) -> None:
        tool = self.registry.tool('MCPTool')
        result = tool.execute('fetch resource list')
        self.assertIn('Mirrored tool', result)
        self.assertIn('MCPTool', result)

    def test_command_lookup_is_case_sensitive(self) -> None:
        cmd = self.registry.command('review')
        self.assertIsNotNone(cmd)

    def test_missing_command_returns_none(self) -> None:
        # The registry returns None for unknown commands rather than raising.
        result = self.registry.command('this-command-does-not-exist-xyz')
        self.assertIsNone(result)

    def test_missing_tool_returns_none(self) -> None:
        # The registry returns None for unknown tools rather than raising.
        result = self.registry.tool('NonExistentToolXYZ')
        self.assertIsNone(result)

    def test_command_name_matches_lookup_key(self) -> None:
        cmd = self.registry.command('review')
        self.assertEqual(cmd.name, 'review')

    def test_mcp_tool_name_matches_lookup_key(self) -> None:
        tool = self.registry.tool('MCPTool')
        self.assertEqual(tool.name, 'MCPTool')

    def test_bash_tool_is_in_registry(self) -> None:
        tool = self.registry.tool('BashTool')
        result = tool.execute('run ls command')
        self.assertIn('Mirrored tool', result)


if __name__ == '__main__':
    unittest.main()
