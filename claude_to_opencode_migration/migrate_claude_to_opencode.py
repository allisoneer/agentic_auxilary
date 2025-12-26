#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "PyYAML>=6.0.2",
#   "rich>=13.7.0",
# ]
# ///
"""
Claude Code → OpenCode Migration Script

Run:
  uv run tools/migrate_claude_to_opencode.py --dry-run
  uv run tools/migrate_claude_to_opencode.py --agents --commands --permissions --mcp
"""

from __future__ import annotations

import argparse
import datetime as _dt
import difflib
import json
import os
import re
import shutil
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple

import yaml
from rich.console import Console
from rich.panel import Panel
from rich.table import Table
from rich.text import Text


# ---------------------------
# Constants and mappings
# ---------------------------

COLOR_MAP = {
    "blue": "#3B82F6",
    "cyan": "#06B6D4",
    "green": "#22C55E",
    "yellow": "#EAB308",
    "magenta": "#D946EF",
    "red": "#EF4444",
}

MODEL_MAP = {
    "sonnet": "anthropic/claude-sonnet-4-5",
    "opus": "anthropic/claude-opus-4-5",
    "haiku": "anthropic/claude-haiku-4-5",
    "sonnet-4.5": "anthropic/claude-sonnet-4-5",
    "opus-4.5": "anthropic/claude-opus-4-5",
    "haiku-4.5": "anthropic/claude-haiku-4-5",
}

UNSUPPORTED_TOOLS = {
    "websearch",  # No OpenCode equivalent
    "task",       # Use @mention or subagent instead
}


# ---------------------------
# Utilities
# ---------------------------

def _now_ts() -> str:
    return _dt.datetime.now().strftime("%Y%m%d-%H%M%S")


def read_text(path: Path) -> Optional[str]:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return None


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def load_json(path: Path) -> Dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return {}
    except json.JSONDecodeError as e:
        raise RuntimeError(f"Invalid JSON at {path}: {e}")


def dump_json(data: Dict[str, Any]) -> str:
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


def unified_diff(old: str, new: str, path: str) -> str:
    a = old.splitlines(keepends=True)
    b = new.splitlines(keepends=True)
    diff = difflib.unified_diff(a, b, fromfile=f"a/{path}", tofile=f"b/{path}")
    return "".join(diff)


def ensure_color_hex(value: Optional[str], console: Console, ctx: str) -> Optional[str]:
    if not value:
        return None
    v = value.strip().lower()
    if v in COLOR_MAP:
        return COLOR_MAP[v]
    if re.fullmatch(r"#?[0-9a-f]{6}", v, flags=re.IGNORECASE):
        return v if v.startswith("#") else f"#{v}"
    console.print(f"[yellow]Warning:[/yellow] Unknown color '{value}' at {ctx}; keeping as-is")
    return value


def map_model(value: Optional[str]) -> Optional[str]:
    if not value:
        return None
    v = value.strip()
    low = v.lower()
    if "/" in v:
        return v  # Already namespaced, trust user
    return MODEL_MAP.get(low, v)  # fallback to original if unmapped


MCP_PAT = re.compile(r"^mcp__([A-Za-z0-9_\-]+)__([A-Za-z0-9_\-]+)$")

def normalize_tool_name(token: str, *, console: Optional[Console] = None) -> Optional[str]:
    token_lower = token.lower()

    # Drop unsupported
    if token_lower in UNSUPPORTED_TOOLS:
        if console:
            console.print(f"[yellow]Warning:[/yellow] Dropping unsupported tool '{token}'")
        return None

    m = MCP_PAT.match(token)
    if m:
        server, tool = m.group(1), m.group(2)
        return f"{server.lower()}_{tool.lower()}"

    # PascalCase or other → lowercase
    return token_lower


def tools_list_to_mapping(tools_list: Iterable[str], console: Console) -> Dict[str, bool]:
    out: Dict[str, bool] = {"*": False}
    for raw in tools_list:
        t = normalize_tool_name(raw.strip(), console=console)
        if not t:
            continue
        out[t] = True
    return out


def parse_yaml_frontmatter(md: str) -> Tuple[Optional[Dict[str, Any]], str]:
    """Return (frontmatter_dict_or_None, body)"""
    if not md.startswith("---"):
        return None, md
    lines = md.splitlines()
    if not lines or lines[0].strip() != "---":
        return None, md
    # find closing '---' on a line by itself
    end_idx = None
    for i in range(1, min(len(lines), 2000)):  # safety bound
        if lines[i].strip() == "---":
            end_idx = i
            break
    if end_idx is None:
        return None, md

    fm_str = "\n".join(lines[1:end_idx]) + "\n"
    body = "\n".join(lines[end_idx + 1:]) + ("\n" if md.endswith("\n") else "")
    try:
        fm = yaml.safe_load(fm_str) or {}
        if not isinstance(fm, dict):
            fm = {}
    except Exception:
        fm = {}
    return fm, body


def make_yaml_frontmatter(data: Dict[str, Any]) -> str:
    dumped = yaml.safe_dump(data, sort_keys=False, allow_unicode=True)
    return f"---\n{dumped}---\n"


def extract_title_for_description(md_body: str, fallback_name: str) -> str:
    for line in md_body.splitlines():
        m = re.match(r"^\s*#\s+(.+)$", line.strip())
        if m:
            return m.group(1).strip()
    # fallback from filename
    name = Path(fallback_name).stem
    name = re.sub(r"[_\-]+", " ", name).strip().title()
    return name or "Command"


def parse_bash_pattern(item: str) -> Optional[str]:
    """
    Convert Bash(git log:*) -> "git log *"
    Bash(pwd) -> "pwd"
    Bash(env | grep:*) -> "env | grep *"
    """
    if not item.startswith("Bash(") or not item.endswith(")"):
        return None
    inner = item[len("Bash("):-1].strip()
    inner = inner.replace(":*", " *")
    return inner


# ---------------------------
# Migration Plan Engine
# ---------------------------

@dataclass
class Action:
    kind: str  # "mkdir", "write_text", "update_json"
    path: Path
    description: str
    content: Optional[str] = None
    update_fn: Optional[Callable[[Dict[str, Any]], Dict[str, Any]]] = None


@dataclass
class MigrationPlan:
    root: Path
    dry_run: bool
    conflict: str  # "skip" | "overwrite" | "prompt"
    console: Console
    actions: List[Action] = field(default_factory=list)
    _backup_dir: Optional[Path] = None

    def backup_dir(self) -> Path:
        if self._backup_dir is None:
            ts = _now_ts()
            self._backup_dir = self.root / f".opencode-migrate-backup/{ts}"
        return self._backup_dir

    def add_mkdir(self, path: Path, description: str = "Create directory") -> None:
        self.actions.append(Action(kind="mkdir", path=path, description=description))

    def add_write_text(self, path: Path, content: str, description: str) -> None:
        self.actions.append(Action(kind="write_text", path=path, description=description, content=content))

    def add_update_json(self, path: Path, update_fn: Callable[[Dict[str, Any]], Dict[str, Any]], description: str) -> None:
        self.actions.append(Action(kind="update_json", path=path, description=description, update_fn=update_fn))

    def _backup_if_exists(self, path: Path) -> None:
        if not path.exists():
            return
        backup_root = self.backup_dir()
        try:
            rel = path.resolve().relative_to(self.root.resolve())
            dest = backup_root / rel
        except Exception:
            safe_abs = str(path.resolve()).replace(":", "").replace("/", "_")
            dest = backup_root / "external" / safe_abs
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dest)

    def _maybe_prompt_overwrite(self, path: Path) -> bool:
        if self.conflict == "overwrite":
            return True
        if self.conflict == "skip":
            return False
        self.console.print(f"[yellow]File exists:[/yellow] {path}")
        resp = input("Overwrite? [y/N]: ").strip().lower()
        return resp in {"y", "yes"}

    def execute(self) -> int:
        created = 0
        updated = 0
        skipped = 0
        errored = 0

        for act in self.actions:
            try:
                if act.kind == "mkdir":
                    if self.dry_run:
                        exists = act.path.exists()
                        self.console.print(f"[cyan]mkdir[/cyan] {act.path} {'(exists)' if exists else ''}")
                    else:
                        act.path.mkdir(parents=True, exist_ok=True)
                        self.console.print(f"[green]mkdir[/green] {act.path}")
                    continue

                if act.kind == "write_text":
                    current = read_text(act.path)
                    new = act.content or ""
                    if current == new:
                        skipped += 1
                        self.console.print(f"[blue]up-to-date[/blue] {act.path}")
                        continue

                    if act.path.exists():
                        if self.dry_run:
                            diff = unified_diff(current or "", new, str(act.path))
                            self.console.print(Panel(diff or "(no diff?)", title=f"diff: {act.path}", border_style="yellow"))
                            skipped += 1
                            continue
                        else:
                            if self.conflict == "skip":
                                skipped += 1
                                self.console.print(f"[yellow]skip (conflict)[/yellow] {act.path}")
                                continue
                            if self.conflict == "prompt":
                                if not self._maybe_prompt_overwrite(act.path):
                                    skipped += 1
                                    self.console.print(f"[yellow]skip[/yellow] {act.path}")
                                    continue
                            self._backup_if_exists(act.path)
                            write_text(act.path, new)
                            updated += 1
                            self.console.print(f"[green]updated[/green] {act.path}")
                    else:
                        if self.dry_run:
                            self.console.print(f"[cyan]create[/cyan] {act.path}")
                            diff = unified_diff("", new, str(act.path))
                            self.console.print(Panel(diff or new, title=f"new: {act.path}", border_style="green"))
                            created += 1
                        else:
                            write_text(act.path, new)
                            created += 1
                            self.console.print(f"[green]created[/green] {act.path}")
                    continue

                if act.kind == "update_json":
                    current_obj = load_json(act.path)
                    new_obj = act.update_fn(current_obj if isinstance(current_obj, dict) else {})
                    new_json = dump_json(new_obj)
                    current_json = dump_json(current_obj if isinstance(current_obj, dict) else {})

                    if current_json == new_json:
                        skipped += 1
                        self.console.print(f"[blue]up-to-date[/blue] {act.path}")
                        continue

                    if self.dry_run:
                        diff = unified_diff(current_json, new_json, str(act.path))
                        self.console.print(Panel(diff or "(no diff?)", title=f"json diff: {act.path}", border_style="yellow"))
                        skipped += 1
                    else:
                        if act.path.exists():
                            if self.conflict == "skip":
                                skipped += 1
                                self.console.print(f"[yellow]skip (conflict)[/yellow] {act.path}")
                                continue
                            if self.conflict == "prompt":
                                if not self._maybe_prompt_overwrite(act.path):
                                    skipped += 1
                                    self.console.print(f"[yellow]skip[/yellow] {act.path}")
                                    continue
                            self._backup_if_exists(act.path)
                            self.console.print(f"[green]updated[/green] {act.path}")
                            act.path.parent.mkdir(parents=True, exist_ok=True)
                            act.path.write_text(new_json, encoding="utf-8")
                            updated += 1
                        else:
                            self.console.print(f"[green]created[/green] {act.path}")
                            act.path.parent.mkdir(parents=True, exist_ok=True)
                            act.path.write_text(new_json, encoding="utf-8")
                            created += 1
                    continue

                self.console.print(f"[red]Unknown action kind[/red]: {act.kind}")
                errored += 1
            except Exception as e:
                errored += 1
                self.console.print(f"[red]error[/red] {act.path}: {e}")

        summary = Table(title="Migration Summary", show_header=False)
        summary.add_row("Created", str(created))
        summary.add_row("Updated", str(updated))
        summary.add_row("Skipped", str(skipped))
        summary.add_row("Errored", str(errored))
        if not self.dry_run and (created or updated):
            self.console.print(f"Backups stored under: {self.backup_dir()}")
        self.console.print(summary)
        return 0 if errored == 0 else 1


# ---------------------------
# Discovery and transforms
# ---------------------------

def discover_agent_files(root: Path) -> List[Path]:
    return sorted((root / ".claude" / "agents").glob("*.md"))


def discover_command_files(root: Path) -> List[Path]:
    return sorted((root / ".claude" / "commands").glob("*.md"))


def discover_settings(root: Path, include_local: bool) -> Dict[str, Any]:
    merged: Dict[str, Any] = {}
    settings_path = root / ".claude" / "settings.json"
    base = load_json(settings_path)
    merged.update(base)
    if include_local:
        local_path = root / ".claude" / "settings.local.json"
        local = load_json(local_path)
        if "permissions" in local:
            mp = local["permissions"]
            bp = merged.setdefault("permissions", {})
            for k in ("allow", "deny"):
                ba = set(bp.get(k, []))
                la = set(mp.get(k, []))
                bp[k] = sorted(ba.union(la))
    return merged


def load_mcp_sources(root: Path) -> Dict[str, Any]:
    local = load_json(root / ".mcp.json")
    global_path = Path(os.path.expanduser("~")) / ".claude.json"
    glob = load_json(global_path)
    res: Dict[str, Any] = {}

    def extract(obj: Dict[str, Any]):
        if not obj:
            return
        servers = obj.get("mcpServers") or obj.get("mcp") or {}
        for name, cfg in servers.items():
            res[name] = cfg

    extract(glob)
    extract(local)  # override global
    return res


def transform_agent_markdown(md: str, filename: str, console: Console) -> str:
    fm, body = parse_yaml_frontmatter(md)
    fm = fm or {}
    description = fm.get("description") or ""
    model = map_model(fm.get("model"))
    color = ensure_color_hex(fm.get("color"), console, f"agent {filename}")
    original_tools = fm.get("tools")

    tools_map: Dict[str, bool] = {"*": False}
    if isinstance(original_tools, str):
        parts = [p.strip() for p in original_tools.split(",") if p.strip()]
        tools_map = tools_list_to_mapping(parts, console)
    elif isinstance(original_tools, list):
        tools_map = tools_list_to_mapping(original_tools, console)
    elif isinstance(original_tools, dict):
        base = {"*": bool(original_tools.get("*", False))}
        for k, v in original_tools.items():
            if k == "*":
                continue
            nk = normalize_tool_name(k, console=console)
            if nk:
                base[nk] = bool(v)
        tools_map = base
    else:
        tools_map = {"*": False}

    new_fm: Dict[str, Any] = {
        "mode": "subagent",
        "description": description,
        "tools": tools_map,
    }
    if model:
        new_fm["model"] = model
    if color:
        new_fm["color"] = color

    if body and not body.endswith("\n"):
        body = body + "\n"
    return make_yaml_frontmatter(new_fm) + body


def transform_command_markdown(md: str, filename: str, console: Console) -> str:
    fm, body = parse_yaml_frontmatter(md)
    body = body or ""

    new_fm: Dict[str, Any] = {}
    if fm and "model" in fm:
        mapped = map_model(fm.get("model"))
        if mapped:
            new_fm["model"] = mapped

    desc = fm.get("description") if fm else None
    if not desc:
        desc = extract_title_for_description(body, filename)
    new_fm["description"] = desc

    return make_yaml_frontmatter(new_fm) + body


def build_permissions_from_settings(settings: Dict[str, Any], console: Console) -> Tuple[Dict[str, Any], Dict[str, bool]]:
    """Returns (permission_obj, tools_obj)"""
    p = settings.get("permissions", {}) or {}
    allow: List[str] = p.get("allow", []) or []
    deny: List[str] = p.get("deny", []) or []

    permission: Dict[str, Any] = {}
    tools: Dict[str, bool] = {}

    # Safe defaults
    permission["bash"] = {"*": "ask"}
    permission["edit"] = permission.get("edit", "ask")
    permission["write"] = permission.get("write", "ask")

    tools["*"] = tools.get("*", False)
    tools["bash"] = tools.get("bash", True)
    tools["edit"] = tools.get("edit", True)
    tools["write"] = tools.get("write", True)

    def handle_entry(lst: List[str], mode: str):
        for raw in lst:
            raw = raw.strip()
            if not raw:
                continue

            bp = parse_bash_pattern(raw)
            if bp:
                permission.setdefault("bash", {}).setdefault(bp, mode)
                permission["bash"][bp] = mode
                continue

            if raw.startswith("mcp__"):
                m = MCP_PAT.match(raw)
                if m:
                    server = m.group(1).lower()
                    tools[f"{server}_*"] = (mode == "allow")
                else:
                    console.print(f"[yellow]Warning:[/yellow] Unknown MCP pattern: {raw}")
                continue

            tool = normalize_tool_name(raw, console=console)
            if not tool:
                continue
            tools[tool] = (mode == "allow")
            if mode in ("allow", "deny"):
                permission[tool] = mode

    handle_entry(allow, "allow")
    handle_entry(deny, "deny")

    return permission, tools


def update_opencode_json_factory(
    add_permission: Dict[str, Any],
    add_tools: Dict[str, bool],
    add_mcp: Dict[str, Any],
) -> Callable[[Dict[str, Any]], Dict[str, Any]]:
    def updater(current: Dict[str, Any]) -> Dict[str, Any]:
        new = dict(current) if current else {}
        perm = dict(new.get("permission", {}))
        tools = dict(new.get("tools", {}))
        mcp = dict(new.get("mcp", {}))

        # Merge bash permissions
        bash_cur = dict(perm.get("bash", {}))
        bash_add = dict(add_permission.get("bash", {}))
        for k, v in bash_add.items():
            if k not in bash_cur:
                bash_cur[k] = v
        perm["bash"] = {"*": "ask", **bash_cur} if "*" not in bash_cur else bash_cur

        for k, v in add_permission.items():
            if k == "bash":
                continue
            if k not in perm:
                perm[k] = v

        tools.setdefault("*", False)
        for k, v in add_tools.items():
            tools.setdefault(k, v)

        for name, cfg in add_mcp.items():
            if name not in mcp:
                mcp[name] = cfg

        new["permission"] = perm
        new["tools"] = tools
        if add_mcp:
            new["mcp"] = mcp

        return new
    return updater


def transform_mcp_servers(mcp_src: Dict[str, Any], console: Console) -> Dict[str, Any]:
    """Transform Claude MCP server entries to OpenCode format."""
    out: Dict[str, Any] = {}
    for name, cfg in (mcp_src or {}).items():
        if not isinstance(cfg, dict):
            continue
        typ = cfg.get("type") or cfg.get("transport") or "stdio"
        typ_low = str(typ).lower()
        if typ_low == "stdio":
            dest_type = "local"
        elif typ_low in ("sse", "http", "https"):
            dest_type = "remote"
        else:
            dest_type = "local"

        command = cfg.get("command")
        args = cfg.get("args", [])
        env = cfg.get("env", {}) or cfg.get("environment", {})

        command_arr: Optional[List[str]] = None
        if isinstance(command, str):
            if isinstance(args, list) and args:
                command_arr = [command, *[str(a) for a in args]]
            else:
                command_arr = [command]
        elif isinstance(command, list):
            command_arr = [str(x) for x in command]

        dest = {"type": dest_type, "enabled": True}
        if command_arr and dest_type == "local":
            dest["command"] = command_arr
        if isinstance(env, dict) and env:
            dest["environment"] = env

        for key in ("url", "baseUrl", "endpoint"):
            if key in cfg:
                dest["url"] = cfg[key]
                break

        out[name] = dest
    return out


# ---------------------------
# Orchestration
# ---------------------------

def run_migration(
    root: Path,
    *,
    migrate_agents: bool,
    migrate_commands: bool,
    migrate_permissions: bool,
    migrate_mcp: bool,
    include_local_settings: bool,
    mcp_target: str,
    dry_run: bool,
    conflict: str,
    console: Console,
) -> int:
    plan = MigrationPlan(root=root, dry_run=dry_run, conflict=conflict, console=console)

    op_agent_dir = root / ".opencode" / "agent"
    op_command_dir = root / ".opencode" / "command"
    op_project_config = root / "opencode.json"
    op_global_config = Path(os.path.expanduser("~")) / ".config" / "opencode" / "opencode.json"

    if migrate_agents:
        plan.add_mkdir(op_agent_dir, "Ensure .opencode/agent directory")
        for src in discover_agent_files(root):
            try:
                md = read_text(src)
                if md is None:
                    continue
                new_md = transform_agent_markdown(md, src.name, console)
                dest = op_agent_dir / src.name
                plan.add_write_text(dest, new_md, f"Migrate agent {src.name}")
            except Exception as e:
                console.print(f"[red]Agent error[/red] {src}: {e}")

    if migrate_commands:
        plan.add_mkdir(op_command_dir, "Ensure .opencode/command directory")
        for src in discover_command_files(root):
            try:
                md = read_text(src)
                if md is None:
                    continue
                new_md = transform_command_markdown(md, src.name, console)
                dest = op_command_dir / src.name
                plan.add_write_text(dest, new_md, f"Migrate command {src.name}")
            except Exception as e:
                console.print(f"[red]Command error[/red] {src}: {e}")

    add_permission: Dict[str, Any] = {}
    add_tools: Dict[str, bool] = {}
    add_mcp: Dict[str, Any] = {}

    if migrate_permissions:
        try:
            settings = discover_settings(root, include_local=include_local_settings)
            perm, tools = build_permissions_from_settings(settings, console)
            add_permission.update(perm)
            add_tools.update(tools)
        except Exception as e:
            console.print(f"[red]Permission migration error:[/red] {e}")

    if migrate_mcp:
        try:
            mcp_src = load_mcp_sources(root)
            add_mcp = transform_mcp_servers(mcp_src, console)
            for server in add_mcp.keys():
                add_tools.setdefault(f"{server}_*".lower(), True)
        except Exception as e:
            console.print(f"[red]MCP migration error:[/red] {e}")

    if migrate_permissions or migrate_mcp:
        updater = update_opencode_json_factory(add_permission, add_tools, add_mcp)
        if mcp_target == "project":
            plan.add_update_json(op_project_config, updater, "Update project opencode.json")
        elif mcp_target == "global":
            plan.add_update_json(op_global_config, updater, "Update global opencode.json")

    return plan.execute()


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Migrate Claude Code configuration to OpenCode format."
    )
    p.add_argument("--root", default=".", help="Project root (default: .)")

    scope = p.add_argument_group("Scope selection")
    scope.add_argument("--agents", dest="agents", action="store_true", help="Migrate agents")
    scope.add_argument("--commands", dest="commands", action="store_true", help="Migrate commands")
    scope.add_argument("--permissions", dest="permissions", action="store_true", help="Migrate permissions")
    scope.add_argument("--mcp", dest="mcp", action="store_true", help="Migrate MCP servers")
    scope.add_argument("--all", dest="all", action="store_true", help="Migrate all (default if no specific flags)")

    p.add_argument("--include-local", action="store_true", help="Include .claude/settings.local.json")
    p.add_argument("--mcp-target", choices=["project", "global"], default="project", help="Where to write MCP config")

    p.add_argument("--dry-run", action="store_true", help="Show diffs without writing")
    p.add_argument("--conflict", choices=["skip", "overwrite", "prompt"], default="skip", help="Conflict handling")
    p.add_argument("--no-color", action="store_true", help="Disable colored output")
    p.add_argument("-v", "--verbose", action="store_true", help="Verbose output")

    return p


def main(argv: Optional[List[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    console = Console(no_color=args.no_color)

    root = Path(args.root).resolve()

    if args.all or not (args.agents or args.commands or args.permissions or args.mcp):
        migrate_agents = migrate_commands = migrate_permissions = migrate_mcp = True
    else:
        migrate_agents = args.agents
        migrate_commands = args.commands
        migrate_permissions = args.permissions
        migrate_mcp = args.mcp

    console.print(Panel(
        Text("Claude → OpenCode Migration", style="bold"),
        subtitle=f"root={root} dry_run={args.dry_run} conflict={args.conflict} mcp_target={args.mcp_target}",
        border_style="cyan",
    ))

    if not (root / ".claude").exists():
        console.print("[yellow]Note:[/yellow] .claude directory not found. Continuing for MCP/global config if applicable.")

    return run_migration(
        root=root,
        migrate_agents=migrate_agents,
        migrate_commands=migrate_commands,
        migrate_permissions=migrate_permissions,
        migrate_mcp=migrate_mcp,
        include_local_settings=args.include_local,
        mcp_target=args.mcp_target,
        dry_run=args.dry_run,
        conflict=args.conflict,
        console=console,
    )


if __name__ == "__main__":
    sys.exit(main())
