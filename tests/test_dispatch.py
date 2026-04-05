"""E4-4: Python unit tests for tool dispatch, command dispatch, permission
context filtering, and ExecutionRegistry wiring.

These tests validate the individual classes and functions that the previous
test_tool_coverage.py tests only touch at the surface level (list structure).
We go deeper here: dispatch logic, case-insensitive lookup, search / filter
semantics, permission gating, and registry wiring.
"""
from __future__ import annotations

import unittest

from src.commands import (
    PORTED_COMMANDS,
    CommandExecution,
    build_command_backlog,
    command_names,
    execute_command,
    find_commands,
    get_command,
    get_commands,
)
from src.execution_registry import (
    ExecutionRegistry,
    MirroredCommand,
    MirroredTool,
    build_execution_registry,
)
from src.models import PortingBacklog, PortingModule
from src.permissions import ToolPermissionContext
from src.tools import (
    PORTED_TOOLS,
    ToolExecution,
    build_tool_backlog,
    execute_tool,
    filter_tools_by_permission_context,
    find_tools,
    get_tool,
    get_tools,
    tool_names,
)


# ──────────────────────────────────────────────────────────────────────────────
# ToolPermissionContext
# ──────────────────────────────────────────────────────────────────────────────


class TestToolPermissionContext(unittest.TestCase):
    def test_empty_context_blocks_nothing(self) -> None:
        ctx = ToolPermissionContext()
        self.assertFalse(ctx.blocks("BashTool"))
        self.assertFalse(ctx.blocks("FileReadTool"))
        self.assertFalse(ctx.blocks("anything"))

    def test_deny_by_exact_name(self) -> None:
        ctx = ToolPermissionContext.from_iterables(deny_names=["BashTool"])
        self.assertTrue(ctx.blocks("BashTool"))
        self.assertFalse(ctx.blocks("FileReadTool"))

    def test_deny_by_exact_name_case_insensitive(self) -> None:
        ctx = ToolPermissionContext.from_iterables(deny_names=["bashtool"])
        self.assertTrue(ctx.blocks("BashTool"))
        self.assertTrue(ctx.blocks("BASHTOOL"))
        self.assertFalse(ctx.blocks("FileReadTool"))

    def test_deny_by_prefix(self) -> None:
        ctx = ToolPermissionContext.from_iterables(deny_prefixes=["mcp_"])
        self.assertTrue(ctx.blocks("mcp_github"))
        self.assertTrue(ctx.blocks("mcp_search_results"))
        self.assertFalse(ctx.blocks("BashTool"))

    def test_deny_by_prefix_case_insensitive(self) -> None:
        ctx = ToolPermissionContext.from_iterables(deny_prefixes=["MCP_"])
        self.assertTrue(ctx.blocks("mcp_github"))
        self.assertTrue(ctx.blocks("MCP_GITHUB"))

    def test_combined_name_and_prefix_deny(self) -> None:
        ctx = ToolPermissionContext.from_iterables(
            deny_names=["BashTool"],
            deny_prefixes=["mcp_"],
        )
        self.assertTrue(ctx.blocks("BashTool"))
        self.assertTrue(ctx.blocks("mcp_github"))
        self.assertFalse(ctx.blocks("FileReadTool"))

    def test_none_deny_lists_treated_as_empty(self) -> None:
        ctx = ToolPermissionContext.from_iterables(deny_names=None, deny_prefixes=None)
        self.assertFalse(ctx.blocks("BashTool"))

    def test_from_iterables_stores_lowercase_names(self) -> None:
        ctx = ToolPermissionContext.from_iterables(deny_names=["BashTool", "READTOOL"])
        self.assertIn("bashtool", ctx.deny_names)
        self.assertIn("readtool", ctx.deny_names)


# ──────────────────────────────────────────────────────────────────────────────
# Tool dispatch
# ──────────────────────────────────────────────────────────────────────────────


class TestGetTool(unittest.TestCase):
    def test_known_tool_returns_porting_module(self) -> None:
        first_tool = PORTED_TOOLS[0]
        result = get_tool(first_tool.name)
        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result.name, first_tool.name)

    def test_lookup_is_case_insensitive(self) -> None:
        first_tool = PORTED_TOOLS[0]
        upper = get_tool(first_tool.name.upper())
        lower = get_tool(first_tool.name.lower())
        self.assertIsNotNone(upper)
        self.assertIsNotNone(lower)

    def test_unknown_tool_returns_none(self) -> None:
        self.assertIsNone(get_tool("totally-nonexistent-xyzzy"))

    def test_empty_string_returns_none(self) -> None:
        # No tool should have an empty name
        self.assertIsNone(get_tool(""))


class TestExecuteTool(unittest.TestCase):
    def test_known_tool_returns_handled_execution(self) -> None:
        first_tool = PORTED_TOOLS[0]
        result = execute_tool(first_tool.name, "test-payload")
        self.assertIsInstance(result, ToolExecution)
        self.assertTrue(result.handled)
        self.assertEqual(result.name, first_tool.name)
        self.assertIn("test-payload", result.message)

    def test_unknown_tool_returns_unhandled_execution(self) -> None:
        result = execute_tool("ghost-tool-xyzzy")
        self.assertIsInstance(result, ToolExecution)
        self.assertFalse(result.handled)
        self.assertIn("ghost-tool-xyzzy", result.message)

    def test_handled_message_contains_source_hint(self) -> None:
        first_tool = PORTED_TOOLS[0]
        result = execute_tool(first_tool.name)
        self.assertIn(first_tool.source_hint, result.message)

    def test_default_payload_is_empty_string(self) -> None:
        first_tool = PORTED_TOOLS[0]
        result = execute_tool(first_tool.name)
        self.assertEqual(result.payload, "")


class TestFindTools(unittest.TestCase):
    def test_search_returns_list(self) -> None:
        results = find_tools("bash")
        self.assertIsInstance(results, list)

    def test_search_is_case_insensitive(self) -> None:
        lower = find_tools("bash")
        upper = find_tools("BASH")
        self.assertEqual(len(lower), len(upper))

    def test_search_with_no_match_returns_empty(self) -> None:
        results = find_tools("zzz_no_match_at_all_xyzzy")
        self.assertEqual(results, [])

    def test_limit_is_respected(self) -> None:
        results = find_tools("tool", limit=3)
        self.assertLessEqual(len(results), 3)

    def test_results_are_porting_modules(self) -> None:
        results = find_tools("bash")
        for r in results:
            self.assertIsInstance(r, PortingModule)


class TestGetTools(unittest.TestCase):
    def test_default_returns_all_tools(self) -> None:
        all_tools = get_tools()
        self.assertEqual(len(all_tools), len(PORTED_TOOLS))

    def test_simple_mode_returns_subset(self) -> None:
        simple = get_tools(simple_mode=True)
        self.assertLessEqual(len(simple), 3)
        names = {t.name for t in simple}
        self.assertTrue(names.issubset({"BashTool", "FileReadTool", "FileEditTool"}))

    def test_exclude_mcp_filters_mcp_tools(self) -> None:
        without_mcp = get_tools(include_mcp=False)
        for tool in without_mcp:
            self.assertNotIn("mcp", tool.name.lower() + tool.source_hint.lower())

    def test_permission_context_filters_denied_tools(self) -> None:
        first_tool = PORTED_TOOLS[0]
        ctx = ToolPermissionContext.from_iterables(deny_names=[first_tool.name])
        filtered = get_tools(permission_context=ctx)
        self.assertNotIn(first_tool, filtered)

    def test_none_permission_context_returns_all(self) -> None:
        result = get_tools(permission_context=None)
        self.assertEqual(len(result), len(PORTED_TOOLS))


class TestFilterToolsByPermissionContext(unittest.TestCase):
    def test_none_context_returns_original_tuple(self) -> None:
        result = filter_tools_by_permission_context(PORTED_TOOLS, None)
        self.assertEqual(result, PORTED_TOOLS)

    def test_empty_context_returns_all(self) -> None:
        ctx = ToolPermissionContext()
        result = filter_tools_by_permission_context(PORTED_TOOLS, ctx)
        self.assertEqual(len(result), len(PORTED_TOOLS))

    def test_deny_first_tool_removes_it(self) -> None:
        first = PORTED_TOOLS[0]
        ctx = ToolPermissionContext.from_iterables(deny_names=[first.name])
        result = filter_tools_by_permission_context(PORTED_TOOLS, ctx)
        self.assertNotIn(first, result)
        self.assertEqual(len(result), len(PORTED_TOOLS) - 1)

    def test_returns_tuple_not_list(self) -> None:
        result = filter_tools_by_permission_context(PORTED_TOOLS)
        self.assertIsInstance(result, tuple)


class TestBuildToolBacklog(unittest.TestCase):
    def test_returns_porting_backlog(self) -> None:
        bl = build_tool_backlog()
        self.assertIsInstance(bl, PortingBacklog)

    def test_backlog_title(self) -> None:
        bl = build_tool_backlog()
        self.assertEqual(bl.title, "Tool surface")

    def test_backlog_modules_match_ported_tools(self) -> None:
        bl = build_tool_backlog()
        self.assertEqual(len(bl.modules), len(PORTED_TOOLS))


class TestToolNames(unittest.TestCase):
    def test_returns_list_of_strings(self) -> None:
        names = tool_names()
        self.assertIsInstance(names, list)
        for name in names:
            self.assertIsInstance(name, str)

    def test_length_matches_ported_tools(self) -> None:
        self.assertEqual(len(tool_names()), len(PORTED_TOOLS))

    def test_no_empty_names(self) -> None:
        for name in tool_names():
            self.assertTrue(name, "tool_names() should not contain empty strings")


# ──────────────────────────────────────────────────────────────────────────────
# Command dispatch
# ──────────────────────────────────────────────────────────────────────────────


class TestGetCommand(unittest.TestCase):
    def test_known_command_returns_porting_module(self) -> None:
        first_cmd = PORTED_COMMANDS[0]
        result = get_command(first_cmd.name)
        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result.name, first_cmd.name)

    def test_lookup_is_case_insensitive(self) -> None:
        first_cmd = PORTED_COMMANDS[0]
        upper = get_command(first_cmd.name.upper())
        lower = get_command(first_cmd.name.lower())
        self.assertIsNotNone(upper)
        self.assertIsNotNone(lower)

    def test_unknown_command_returns_none(self) -> None:
        self.assertIsNone(get_command("totally-nonexistent-xyzzy"))


class TestExecuteCommand(unittest.TestCase):
    def test_known_command_returns_handled_execution(self) -> None:
        first_cmd = PORTED_COMMANDS[0]
        result = execute_command(first_cmd.name, "hello prompt")
        self.assertIsInstance(result, CommandExecution)
        self.assertTrue(result.handled)
        self.assertEqual(result.name, first_cmd.name)
        self.assertIn("hello prompt", result.message)

    def test_unknown_command_returns_unhandled_execution(self) -> None:
        result = execute_command("ghost-command-xyzzy")
        self.assertIsInstance(result, CommandExecution)
        self.assertFalse(result.handled)
        self.assertIn("ghost-command-xyzzy", result.message)

    def test_handled_message_contains_source_hint(self) -> None:
        first_cmd = PORTED_COMMANDS[0]
        result = execute_command(first_cmd.name)
        self.assertIn(first_cmd.source_hint, result.message)

    def test_default_prompt_is_empty_string(self) -> None:
        first_cmd = PORTED_COMMANDS[0]
        result = execute_command(first_cmd.name)
        self.assertEqual(result.prompt, "")


class TestFindCommands(unittest.TestCase):
    def test_search_returns_list(self) -> None:
        results = find_commands("review")
        self.assertIsInstance(results, list)

    def test_search_is_case_insensitive(self) -> None:
        lower = find_commands("review")
        upper = find_commands("REVIEW")
        self.assertEqual(len(lower), len(upper))

    def test_search_with_no_match_returns_empty(self) -> None:
        results = find_commands("zzz_no_match_at_all_xyzzy")
        self.assertEqual(results, [])

    def test_limit_is_respected(self) -> None:
        results = find_commands("command", limit=2)
        self.assertLessEqual(len(results), 2)

    def test_results_are_porting_modules(self) -> None:
        results = find_commands("review")
        for r in results:
            self.assertIsInstance(r, PortingModule)


class TestGetCommands(unittest.TestCase):
    def test_default_returns_all_commands(self) -> None:
        all_cmds = get_commands()
        self.assertEqual(len(all_cmds), len(PORTED_COMMANDS))

    def test_exclude_plugin_commands_filters_correctly(self) -> None:
        without_plugins = get_commands(include_plugin_commands=False)
        for cmd in without_plugins:
            self.assertNotIn("plugin", cmd.source_hint.lower())

    def test_exclude_skill_commands_filters_correctly(self) -> None:
        without_skills = get_commands(include_skill_commands=False)
        for cmd in without_skills:
            self.assertNotIn("skills", cmd.source_hint.lower())

    def test_returns_tuple_of_porting_modules(self) -> None:
        cmds = get_commands()
        self.assertIsInstance(cmds, tuple)
        for cmd in cmds:
            self.assertIsInstance(cmd, PortingModule)


class TestBuildCommandBacklog(unittest.TestCase):
    def test_returns_porting_backlog(self) -> None:
        bl = build_command_backlog()
        self.assertIsInstance(bl, PortingBacklog)

    def test_backlog_title(self) -> None:
        bl = build_command_backlog()
        self.assertEqual(bl.title, "Command surface")

    def test_backlog_modules_match_ported_commands(self) -> None:
        bl = build_command_backlog()
        self.assertEqual(len(bl.modules), len(PORTED_COMMANDS))


class TestCommandNames(unittest.TestCase):
    def test_returns_list_of_strings(self) -> None:
        names = command_names()
        self.assertIsInstance(names, list)
        for name in names:
            self.assertIsInstance(name, str)

    def test_length_matches_ported_commands(self) -> None:
        self.assertEqual(len(command_names()), len(PORTED_COMMANDS))

    def test_no_empty_names(self) -> None:
        for name in command_names():
            self.assertTrue(name, "command_names() should not contain empty strings")


# ──────────────────────────────────────────────────────────────────────────────
# ExecutionRegistry wiring
# ──────────────────────────────────────────────────────────────────────────────


class TestMirroredTool(unittest.TestCase):
    def test_execute_known_tool_returns_message(self) -> None:
        first_tool = PORTED_TOOLS[0]
        mirrored = MirroredTool(name=first_tool.name, source_hint=first_tool.source_hint)
        msg = mirrored.execute("my-payload")
        self.assertIsInstance(msg, str)
        self.assertIn("my-payload", msg)

    def test_execute_unknown_tool_returns_error_message(self) -> None:
        mirrored = MirroredTool(name="ghost-xyzzy", source_hint="")
        msg = mirrored.execute("p")
        self.assertIn("ghost-xyzzy", msg)

    def test_is_frozen_dataclass(self) -> None:
        mirrored = MirroredTool(name="BashTool", source_hint="tools/")
        with self.assertRaises(Exception):
            mirrored.name = "other"  # type: ignore[misc]


class TestMirroredCommand(unittest.TestCase):
    def test_execute_known_command_returns_message(self) -> None:
        first_cmd = PORTED_COMMANDS[0]
        mirrored = MirroredCommand(name=first_cmd.name, source_hint=first_cmd.source_hint)
        msg = mirrored.execute("some prompt")
        self.assertIsInstance(msg, str)
        self.assertIn("some prompt", msg)

    def test_execute_unknown_command_returns_error_message(self) -> None:
        mirrored = MirroredCommand(name="ghost-cmd-xyzzy", source_hint="")
        msg = mirrored.execute("p")
        self.assertIn("ghost-cmd-xyzzy", msg)

    def test_is_frozen_dataclass(self) -> None:
        mirrored = MirroredCommand(name="review", source_hint="commands/")
        with self.assertRaises(Exception):
            mirrored.name = "other"  # type: ignore[misc]


class TestBuildExecutionRegistry(unittest.TestCase):
    def test_returns_execution_registry(self) -> None:
        reg = build_execution_registry()
        self.assertIsInstance(reg, ExecutionRegistry)

    def test_registry_commands_count_matches_ported_commands(self) -> None:
        reg = build_execution_registry()
        self.assertEqual(len(reg.commands), len(PORTED_COMMANDS))

    def test_registry_tools_count_matches_ported_tools(self) -> None:
        reg = build_execution_registry()
        self.assertEqual(len(reg.tools), len(PORTED_TOOLS))

    def test_registry_command_lookup_known(self) -> None:
        reg = build_execution_registry()
        first_cmd = PORTED_COMMANDS[0]
        cmd = reg.command(first_cmd.name)
        self.assertIsNotNone(cmd)
        assert cmd is not None
        self.assertEqual(cmd.name, first_cmd.name)

    def test_registry_command_lookup_unknown_returns_none(self) -> None:
        reg = build_execution_registry()
        self.assertIsNone(reg.command("ghost-command-xyzzy"))

    def test_registry_tool_lookup_known(self) -> None:
        reg = build_execution_registry()
        first_tool = PORTED_TOOLS[0]
        tool = reg.tool(first_tool.name)
        self.assertIsNotNone(tool)
        assert tool is not None
        self.assertEqual(tool.name, first_tool.name)

    def test_registry_tool_lookup_unknown_returns_none(self) -> None:
        reg = build_execution_registry()
        self.assertIsNone(reg.tool("ghost-tool-xyzzy"))

    def test_registry_lookup_is_case_insensitive(self) -> None:
        reg = build_execution_registry()
        first_tool = PORTED_TOOLS[0]
        upper = reg.tool(first_tool.name.upper())
        lower = reg.tool(first_tool.name.lower())
        self.assertIsNotNone(upper)
        self.assertIsNotNone(lower)

    def test_all_mirrored_tools_are_correct_type(self) -> None:
        reg = build_execution_registry()
        for t in reg.tools:
            self.assertIsInstance(t, MirroredTool)

    def test_all_mirrored_commands_are_correct_type(self) -> None:
        reg = build_execution_registry()
        for c in reg.commands:
            self.assertIsInstance(c, MirroredCommand)


if __name__ == "__main__":
    unittest.main()
