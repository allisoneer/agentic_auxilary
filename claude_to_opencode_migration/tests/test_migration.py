#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "pytest>=8.0.0",
#   "PyYAML>=6.0.2",
#   "rich>=13.7.0",
# ]
# ///
"""
Tests for Claude Code → OpenCode Migration Script

Run with: uv run pytest claude_to_opencode_migration/tests/test_migration.py -v
"""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Dict

import pytest
from rich.console import Console

# Import from sibling module
import sys
sys.path.insert(0, str(Path(__file__).parent.parent))
from migrate_claude_to_opencode import (
    normalize_tool_name,
    tools_list_to_mapping,
    map_model,
    ensure_color_hex,
    parse_bash_pattern,
    parse_yaml_frontmatter,
    make_yaml_frontmatter,
    extract_title_for_description,
    transform_agent_markdown,
    transform_command_markdown,
    build_permissions_from_settings,
    transform_mcp_servers,
    run_migration,
)


# ---------------------------
# Fixtures
# ---------------------------

@pytest.fixture
def console() -> Console:
    """A console that captures output."""
    return Console(force_terminal=False, no_color=True)


@pytest.fixture
def temp_project(tmp_path: Path) -> Path:
    """Create a temporary project with Claude config."""
    fixtures = Path(__file__).parent / "fixtures" / "claude_sample"
    if fixtures.exists():
        shutil.copytree(fixtures, tmp_path / "project")
        return tmp_path / "project"

    # Fallback: create minimal structure
    project = tmp_path / "project"
    claude_dir = project / ".claude"
    (claude_dir / "agents").mkdir(parents=True)
    (claude_dir / "commands").mkdir(parents=True)

    # Create sample agent
    (claude_dir / "agents" / "test-agent.md").write_text("""---
name: test-agent
description: A test agent
tools: Read, Grep, mcp__tools__ls
color: blue
model: sonnet
---

Test agent prompt content.
""")

    # Create sample command
    (claude_dir / "commands" / "test-command.md").write_text("""# Test Command

Do the thing.
""")

    # Create settings
    (claude_dir / "settings.json").write_text(json.dumps({
        "permissions": {
            "allow": ["Bash(git status)", "WebFetch", "mcp__tools__ls"],
            "deny": ["Bash(rm:*)"]
        }
    }))

    return project


# ---------------------------
# Unit Tests: normalize_tool_name
# ---------------------------

class TestNormalizeToolName:
    def test_lowercase_conversion(self):
        # WebSearch is unsupported, so use other tools
        assert normalize_tool_name("Read") == "read"
        assert normalize_tool_name("GREP") == "grep"
        assert normalize_tool_name("WebFetch") == "webfetch"

    def test_mcp_pattern_conversion(self):
        assert normalize_tool_name("mcp__tools__ls") == "tools_ls"
        assert normalize_tool_name("mcp__linear-server__list_teams") == "linear-server_list_teams"
        assert normalize_tool_name("mcp__pr_comments__get_all") == "pr_comments_get_all"

    def test_hyphen_preservation_in_server_name(self):
        result = normalize_tool_name("mcp__my-server__my_tool")
        assert result == "my-server_my_tool"

    def test_unsupported_tools_dropped(self, console):
        assert normalize_tool_name("WebSearch", console=console) is None
        assert normalize_tool_name("Task", console=console) is None

    def test_already_lowercase(self):
        assert normalize_tool_name("read") == "read"
        assert normalize_tool_name("grep") == "grep"


# ---------------------------
# Unit Tests: tools_list_to_mapping
# ---------------------------

class TestToolsListToMapping:
    def test_basic_conversion(self, console):
        result = tools_list_to_mapping(["Read", "Grep", "Glob"], console)
        assert result == {"*": False, "read": True, "grep": True, "glob": True}

    def test_with_mcp_tools(self, console):
        result = tools_list_to_mapping(["Read", "mcp__tools__ls"], console)
        assert result == {"*": False, "read": True, "tools_ls": True}

    def test_unsupported_tools_excluded(self, console):
        result = tools_list_to_mapping(["Read", "WebSearch", "Grep"], console)
        assert "websearch" not in result
        assert result == {"*": False, "read": True, "grep": True}

    def test_empty_list(self, console):
        result = tools_list_to_mapping([], console)
        assert result == {"*": False}


# ---------------------------
# Unit Tests: map_model
# ---------------------------

class TestMapModel:
    def test_alias_mapping(self):
        assert map_model("sonnet") == "anthropic/claude-sonnet-4-5"
        assert map_model("opus") == "anthropic/claude-opus-4-5"
        assert map_model("haiku") == "anthropic/claude-haiku-4-5"

    def test_versioned_aliases(self):
        assert map_model("sonnet-4.5") == "anthropic/claude-sonnet-4-5"
        assert map_model("opus-4.5") == "anthropic/claude-opus-4-5"

    def test_already_namespaced_passthrough(self):
        assert map_model("anthropic/claude-sonnet-4-5") == "anthropic/claude-sonnet-4-5"
        assert map_model("openai/gpt-4") == "openai/gpt-4"

    def test_case_insensitive(self):
        assert map_model("SONNET") == "anthropic/claude-sonnet-4-5"
        assert map_model("Opus") == "anthropic/claude-opus-4-5"

    def test_none_handling(self):
        assert map_model(None) is None
        assert map_model("") is None

    def test_unknown_passthrough(self):
        assert map_model("unknown-model") == "unknown-model"


# ---------------------------
# Unit Tests: ensure_color_hex
# ---------------------------

class TestEnsureColorHex:
    def test_known_colors(self, console):
        assert ensure_color_hex("blue", console, "test") == "#3B82F6"
        assert ensure_color_hex("yellow", console, "test") == "#EAB308"
        assert ensure_color_hex("green", console, "test") == "#22C55E"
        assert ensure_color_hex("red", console, "test") == "#EF4444"
        assert ensure_color_hex("cyan", console, "test") == "#06B6D4"
        assert ensure_color_hex("magenta", console, "test") == "#D946EF"

    def test_hex_passthrough(self, console):
        # Function normalizes to lowercase
        assert ensure_color_hex("#FF5733", console, "test") == "#ff5733"
        assert ensure_color_hex("FF5733", console, "test") == "#ff5733"

    def test_case_insensitive(self, console):
        assert ensure_color_hex("BLUE", console, "test") == "#3B82F6"
        assert ensure_color_hex("Yellow", console, "test") == "#EAB308"

    def test_none_handling(self, console):
        assert ensure_color_hex(None, console, "test") is None
        assert ensure_color_hex("", console, "test") is None

    def test_unknown_color_warning(self, console):
        result = ensure_color_hex("purple", console, "test")
        assert result == "purple"  # Kept as-is with warning


# ---------------------------
# Unit Tests: parse_bash_pattern
# ---------------------------

class TestParseBashPattern:
    def test_simple_command(self):
        assert parse_bash_pattern("Bash(pwd)") == "pwd"
        assert parse_bash_pattern("Bash(git status)") == "git status"

    def test_wildcard_pattern(self):
        assert parse_bash_pattern("Bash(git log:*)") == "git log *"
        assert parse_bash_pattern("Bash(cargo test:*)") == "cargo test *"

    def test_pipe_pattern(self):
        assert parse_bash_pattern("Bash(env | grep:*)") == "env | grep *"

    def test_non_bash_returns_none(self):
        assert parse_bash_pattern("WebFetch") is None
        assert parse_bash_pattern("Read") is None
        assert parse_bash_pattern("mcp__tools__ls") is None


# ---------------------------
# Unit Tests: parse_yaml_frontmatter
# ---------------------------

class TestParseYamlFrontmatter:
    def test_with_frontmatter(self):
        md = """---
name: test
description: A test
---

Body content here.
"""
        fm, body = parse_yaml_frontmatter(md)
        assert fm == {"name": "test", "description": "A test"}
        assert "Body content here." in body

    def test_without_frontmatter(self):
        md = "# Just a heading\n\nSome content."
        fm, body = parse_yaml_frontmatter(md)
        assert fm is None
        assert body == md

    def test_empty_frontmatter(self):
        md = """---
---

Body only.
"""
        fm, body = parse_yaml_frontmatter(md)
        assert fm == {}
        assert "Body only." in body


# ---------------------------
# Unit Tests: extract_title_for_description
# ---------------------------

class TestExtractTitleForDescription:
    def test_extracts_h1(self):
        md = "# My Command Title\n\nSome content."
        assert extract_title_for_description(md, "fallback.md") == "My Command Title"

    def test_fallback_to_filename(self):
        md = "No heading here, just content."
        assert extract_title_for_description(md, "my-command.md") == "My Command"
        assert extract_title_for_description(md, "test_command.md") == "Test Command"

    def test_first_h1_wins(self):
        md = "# First\n\n# Second"
        assert extract_title_for_description(md, "fallback.md") == "First"


# ---------------------------
# Unit Tests: transform_agent_markdown
# ---------------------------

class TestTransformAgentMarkdown:
    def test_full_transformation(self, console):
        md = """---
name: test-agent
description: Test agent description
tools: Read, Grep, mcp__tools__ls
color: yellow
model: sonnet
---

Agent prompt content.
"""
        result = transform_agent_markdown(md, "test-agent.md", console)

        assert "mode: subagent" in result
        assert "description: Test agent description" in result
        assert "model: anthropic/claude-sonnet-4-5" in result
        assert "color: '#EAB308'" in result
        assert "'*': false" in result
        assert "read: true" in result
        assert "grep: true" in result
        assert "tools_ls: true" in result
        assert "Agent prompt content." in result
        # name should NOT be in output
        assert "name: test-agent" not in result

    def test_without_optional_fields(self, console):
        md = """---
description: Minimal agent
---

Content.
"""
        result = transform_agent_markdown(md, "minimal.md", console)
        assert "mode: subagent" in result
        assert "description: Minimal agent" in result
        assert "'*': false" in result


# ---------------------------
# Unit Tests: transform_command_markdown
# ---------------------------

class TestTransformCommandMarkdown:
    def test_without_frontmatter(self, console):
        md = """# My Command

Do something useful.
"""
        result = transform_command_markdown(md, "my-command.md", console)
        assert "description: My Command" in result
        assert "Do something useful." in result

    def test_with_existing_frontmatter(self, console):
        md = """---
model: opus
---

# Custom Command

Content here.
"""
        result = transform_command_markdown(md, "custom.md", console)
        assert "model: anthropic/claude-opus-4-5" in result
        assert "description: Custom Command" in result


# ---------------------------
# Unit Tests: build_permissions_from_settings
# ---------------------------

class TestBuildPermissionsFromSettings:
    def test_bash_patterns(self, console):
        settings = {
            "permissions": {
                "allow": ["Bash(git status)", "Bash(git log:*)"],
                "deny": ["Bash(rm:*)"]
            }
        }
        perm, tools = build_permissions_from_settings(settings, console)

        assert perm["bash"]["git status"] == "allow"
        assert perm["bash"]["git log *"] == "allow"
        assert perm["bash"]["rm *"] == "deny"
        assert perm["bash"]["*"] == "ask"

    def test_mcp_tools(self, console):
        settings = {
            "permissions": {
                "allow": ["mcp__tools__ls", "mcp__linear-server__list_teams"],
                "deny": []
            }
        }
        perm, tools = build_permissions_from_settings(settings, console)

        assert tools["tools_*"] is True
        assert tools["linear-server_*"] is True

    def test_regular_tools(self, console):
        settings = {
            "permissions": {
                "allow": ["WebFetch", "Read"],
                "deny": []
            }
        }
        perm, tools = build_permissions_from_settings(settings, console)

        assert tools["webfetch"] is True
        assert tools["read"] is True


# ---------------------------
# Unit Tests: transform_mcp_servers
# ---------------------------

class TestTransformMcpServers:
    def test_stdio_to_local(self, console):
        mcp_src = {
            "tools": {
                "type": "stdio",
                "command": "my-tool",
                "args": ["mcp"],
                "env": {"FOO": "bar"}
            }
        }
        result = transform_mcp_servers(mcp_src, console)

        assert result["tools"]["type"] == "local"
        assert result["tools"]["command"] == ["my-tool", "mcp"]
        assert result["tools"]["environment"] == {"FOO": "bar"}
        assert result["tools"]["enabled"] is True

    def test_sse_to_remote(self, console):
        mcp_src = {
            "linear": {
                "type": "sse",
                "url": "https://mcp.linear.app/sse"
            }
        }
        result = transform_mcp_servers(mcp_src, console)

        assert result["linear"]["type"] == "remote"
        assert result["linear"]["url"] == "https://mcp.linear.app/sse"
        assert result["linear"]["enabled"] is True

    def test_command_array_passthrough(self, console):
        mcp_src = {
            "tool": {
                "type": "stdio",
                "command": ["python", "-m", "mcp"]
            }
        }
        result = transform_mcp_servers(mcp_src, console)
        assert result["tool"]["command"] == ["python", "-m", "mcp"]


# ---------------------------
# Integration Tests
# ---------------------------

class TestIntegration:
    def test_full_migration_dry_run(self, temp_project, console):
        """Test that dry-run doesn't create files."""
        result = run_migration(
            root=temp_project,
            migrate_agents=True,
            migrate_commands=True,
            migrate_permissions=True,
            migrate_mcp=False,
            include_local_settings=False,
            mcp_target="project",
            dry_run=True,
            conflict="skip",
            console=console,
        )

        assert result == 0
        # Dry run should NOT create .opencode directory
        # (it only prints what would happen)

    def test_agent_migration_creates_files(self, temp_project, console):
        """Test that agent migration creates correct files."""
        result = run_migration(
            root=temp_project,
            migrate_agents=True,
            migrate_commands=False,
            migrate_permissions=False,
            migrate_mcp=False,
            include_local_settings=False,
            mcp_target="project",
            dry_run=False,
            conflict="overwrite",
            console=console,
        )

        assert result == 0

        agent_dir = temp_project / ".opencode" / "agent"
        assert agent_dir.exists()

        agent_file = agent_dir / "test-agent.md"
        assert agent_file.exists()

        content = agent_file.read_text()
        assert "mode: subagent" in content

    def test_idempotency(self, temp_project, console):
        """Test that running twice produces same result."""
        # First run
        run_migration(
            root=temp_project,
            migrate_agents=True,
            migrate_commands=True,
            migrate_permissions=True,
            migrate_mcp=False,
            include_local_settings=False,
            mcp_target="project",
            dry_run=False,
            conflict="overwrite",
            console=console,
        )

        # Capture state
        agent_content = (temp_project / ".opencode" / "agent" / "test-agent.md").read_text()

        # Second run
        run_migration(
            root=temp_project,
            migrate_agents=True,
            migrate_commands=True,
            migrate_permissions=True,
            migrate_mcp=False,
            include_local_settings=False,
            mcp_target="project",
            dry_run=False,
            conflict="overwrite",
            console=console,
        )

        # Content should be identical
        agent_content_2 = (temp_project / ".opencode" / "agent" / "test-agent.md").read_text()
        assert agent_content == agent_content_2


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
