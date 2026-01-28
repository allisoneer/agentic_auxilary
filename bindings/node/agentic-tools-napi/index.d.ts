/**
 * N-API bindings for agentic-tools.
 *
 * Provides JavaScript/TypeScript integration for the agentic-tools library family.
 *
 * @module agentic-tools-napi
 */

// =============================================================================
// Configuration Types
// =============================================================================

/** Configuration options for init() */
export interface InitConfig {
  /** Array of tool names to enable (empty or omit for all tools) */
  allowlist?: string[];
  /** Enable strict schema mode (additionalProperties: false) */
  strict?: boolean;
}

// =============================================================================
// Tool Input Types
// =============================================================================

/** Input for ls tool */
export interface LsInput {
  /** Directory path (absolute or relative to cwd) */
  path?: string;
  /** Traversal depth: 0=header, 1=children (default), 2-10=tree */
  depth?: number;
  /** Filter: 'all' (default), 'files', or 'dirs' */
  show?: 'all' | 'files' | 'dirs';
  /** Additional glob patterns to ignore */
  ignore?: string[];
  /** Include hidden files (default: false) */
  hidden?: boolean;
}

/** Input for ask_agent tool */
export interface AskAgentInput {
  /** Agent type: 'locator' (fast discovery) or 'analyzer' (deep analysis) */
  agent_type?: 'locator' | 'analyzer';
  /** Location: 'codebase', 'thoughts', 'references', or 'web' */
  location?: 'codebase' | 'thoughts' | 'references' | 'web';
  /** Task to perform; plain language question/instructions */
  query: string;
}

/** Input for search_grep tool */
export interface SearchGrepInput {
  /** Regex pattern to search for */
  pattern: string;
  /** Directory path (absolute or relative to cwd) */
  path?: string;
  /** Output mode: 'files' (default), 'content', or 'count' */
  mode?: 'files' | 'content' | 'count';
  /** Include-only glob patterns (files to consider) */
  globs?: string[];
  /** Additional glob patterns to ignore (exclude) */
  ignore?: string[];
  /** Include hidden files (default: false) */
  include_hidden?: boolean;
  /** Case-insensitive matching (default: false) */
  case_insensitive?: boolean;
  /** Allow '.' to match newlines (default: false) */
  multiline?: boolean;
  /** Show line numbers in content mode (default: true) */
  line_numbers?: boolean;
  /** Context lines before and after matches */
  context?: number;
  /** Context lines before match */
  context_before?: number;
  /** Context lines after match */
  context_after?: number;
  /** Search binary files as text (default: false) */
  include_binary?: boolean;
  /** Max results to return (default: 200, capped at 1000) */
  head_limit?: number;
  /** Skip the first N results (default: 0) */
  offset?: number;
}

/** Input for search_glob tool */
export interface SearchGlobInput {
  /** Glob pattern to match against (e.g., '**\/*.rs') */
  pattern: string;
  /** Directory path (absolute or relative to cwd) */
  path?: string;
  /** Additional glob patterns to ignore (exclude) */
  ignore?: string[];
  /** Include hidden files (default: false) */
  include_hidden?: boolean;
  /** Sort order: 'name' (default) or 'mtime' (newest first) */
  sort?: 'name' | 'mtime';
  /** Max results to return (default: 500, capped at 1000) */
  head_limit?: number;
  /** Skip the first N results (default: 0) */
  offset?: number;
}

/** Input for just_search tool */
export interface JustSearchInput {
  /** Search query (substring match on name/docs) */
  query?: string;
  /** Directory filter (repo-relative or absolute) */
  dir?: string;
}

/** Input for just_execute tool */
export interface JustExecuteInput {
  /** Recipe name (e.g., 'check', 'test', 'build') */
  recipe: string;
  /** Directory containing the justfile */
  dir?: string;
  /** Arguments keyed by parameter name; star params accept arrays */
  args?: Record<string, unknown>;
}

/** Input for reasoning request tool */
export interface ReasoningRequestInput {
  /** Prompt to pass to the reasoning model */
  prompt: string;
  /** List of files with descriptions */
  files: Array<{ filename: string; description: string }>;
  /** Type of output: 'reasoning' or 'plan' */
  prompt_type: 'reasoning' | 'plan';
  /** Directories to expand into files */
  directories?: Array<{
    directory_path: string;
    description: string;
    extensions?: string[];
    recursive?: boolean;
    include_hidden?: boolean;
    max_files?: number;
  }>;
  /** Optional filename for plan output */
  output_filename?: string;
}

// =============================================================================
// N-API Result Types
// =============================================================================

/** Result from a native tool call, containing both JSON data and human-readable text. */
export interface ToolCallResult {
  /** JSON string containing the tool result data. */
  data: string;
  /** Human-readable text representation of the result. */
  text: string;
}

// =============================================================================
// Tool Output Types
// =============================================================================

/** Entry in ls output */
export interface LsEntry {
  path: string;
  kind: 'file' | 'dir' | 'symlink';
}

/** Output from ls tool */
export interface LsOutput {
  root: string;
  entries: LsEntry[];
  has_more: boolean;
  warnings: string[];
}

/** Output from ask_agent tool */
export interface AgentOutput {
  text: string;
}

/** Output from search_grep tool */
export interface GrepOutput {
  root: string;
  mode: 'files' | 'content' | 'count';
  lines: string[];
  has_more: boolean;
  warnings: string[];
  summary?: string;
}

/** Output from search_glob tool */
export interface GlobOutput {
  root: string;
  entries: string[];
  has_more: boolean;
  warnings: string[];
}

/** Item in just_search output */
export interface JustSearchItem {
  recipe: string;
  dir: string;
  doc?: string;
  params: string[];
}

/** Output from just_search tool */
export interface JustSearchOutput {
  items: JustSearchItem[];
  has_more: boolean;
}

/** Output from just_execute tool */
export interface JustExecuteOutput {
  dir: string;
  recipe: string;
  success: boolean;
  exit_code?: number;
  stdout: string;
  stderr: string;
}

// =============================================================================
// Schema Patch Types
// =============================================================================

/** Field-level schema patch */
export interface FieldPatch {
  /** Restrict field to specific values */
  enum?: unknown[];
  /** Minimum numeric value */
  minimum?: number;
  /** Maximum numeric value */
  maximum?: number;
  /** Regex pattern for string validation */
  pattern?: string;
}

/** Tool-level schema patches */
export interface ToolSchemaPatch {
  properties?: Record<string, FieldPatch>;
  [key: string]: unknown;
}

/** Schema patches object (tool name -> patches) */
export type SchemaPatches = Record<string, ToolSchemaPatch>;

// =============================================================================
// Generic API Functions
// =============================================================================

/**
 * Initialize the agentic-tools registry with all available tools.
 *
 * @param configJson - JSON configuration string
 * @throws Error if config is invalid JSON or registry is already initialized
 *
 * @example
 * ```typescript
 * import { init } from 'agentic-tools-napi';
 * init('{}'); // Initialize with all tools
 * init('{"allowlist": ["ls", "search_grep"]}'); // Only specific tools
 * ```
 */
export function init(configJson: string): void;

/**
 * List available tools with their schemas for a specific provider.
 *
 * @param provider - Provider format: "openai", "anthropic", or "mcp"
 * @returns JSON string containing an array of tool definitions
 * @throws Error if registry is not initialized
 */
export function listTools(provider: 'openai' | 'anthropic' | 'mcp'): string;

/**
 * Execute a tool with JSON arguments.
 *
 * @param name - Name of the tool to call
 * @param argsJson - JSON string containing the tool arguments
 * @returns Promise resolving to a ToolCallResult with data (JSON string) and text
 * @throws Error if registry is not initialized or tool execution fails
 */
export function callTool(name: string, argsJson: string): Promise<ToolCallResult>;

/**
 * Apply schema patches for runtime customization.
 *
 * @param patchesJson - JSON string containing schema patches
 * @throws Error if patches JSON is invalid
 */
export function setSchemaPatches(patchesJson: string): void;

/**
 * Check if the registry has been initialized.
 */
export function isInitialized(): boolean;

/**
 * Get the number of registered tools.
 */
export function toolCount(): number;

/**
 * Get names of all registered tools.
 */
export function getToolNames(): string[];

// =============================================================================
// Typed Tool Functions
// =============================================================================

/**
 * List files and directories.
 *
 * @param argsJson - JSON string with LsInput
 * @returns Promise resolving to a ToolCallResult with LsOutput in data
 */
export function callLs(argsJson: string): Promise<ToolCallResult>;

/**
 * Spawn a Claude subagent for discovery or analysis.
 *
 * @param argsJson - JSON string with AskAgentInput
 * @returns Promise resolving to a ToolCallResult with AgentOutput in data
 */
export function callAskAgent(argsJson: string): Promise<ToolCallResult>;

/**
 * Regex-based code search.
 *
 * @param argsJson - JSON string with SearchGrepInput
 * @returns Promise resolving to a ToolCallResult with GrepOutput in data
 */
export function callGrep(argsJson: string): Promise<ToolCallResult>;

/**
 * Glob-based file matching.
 *
 * @param argsJson - JSON string with SearchGlobInput
 * @returns Promise resolving to a ToolCallResult with GlobOutput in data
 */
export function callGlob(argsJson: string): Promise<ToolCallResult>;

/**
 * Search justfile recipes.
 *
 * @param argsJson - JSON string with JustSearchInput
 * @returns Promise resolving to a ToolCallResult with JustSearchOutput in data
 */
export function callJustSearch(argsJson: string): Promise<ToolCallResult>;

/**
 * Execute a justfile recipe.
 *
 * @param argsJson - JSON string with JustExecuteInput
 * @returns Promise resolving to a ToolCallResult with JustExecuteOutput in data
 */
export function callJustExecute(argsJson: string): Promise<ToolCallResult>;

/**
 * Request assistance from the reasoning model.
 *
 * @param argsJson - JSON string with ReasoningRequestInput
 * @returns Promise resolving to a ToolCallResult with reasoning result in data
 */
export function callReasoningRequest(argsJson: string): Promise<ToolCallResult>;

// =============================================================================
// Type-Safe Wrapper Helpers
// =============================================================================

/**
 * Helper to call ls with typed input and output.
 *
 * @example
 * ```typescript
 * import { init, callLsTyped } from 'agentic-tools-napi';
 * init('{}');
 * const result = await callLsTyped({ path: '.', depth: 2 });
 * console.log(result.entries);
 * ```
 */
export function callLsTyped(input?: LsInput): Promise<LsOutput>;

/**
 * Helper to call ask_agent with typed input and output.
 */
export function callAskAgentTyped(input: AskAgentInput): Promise<AgentOutput>;

/**
 * Helper to call search_grep with typed input and output.
 */
export function callGrepTyped(input: SearchGrepInput): Promise<GrepOutput>;

/**
 * Helper to call search_glob with typed input and output.
 */
export function callGlobTyped(input: SearchGlobInput): Promise<GlobOutput>;

/**
 * Helper to call just_search with typed input and output.
 */
export function callJustSearchTyped(input?: JustSearchInput): Promise<JustSearchOutput>;

/**
 * Helper to call just_execute with typed input and output.
 */
export function callJustExecuteTyped(input: JustExecuteInput): Promise<JustExecuteOutput>;

/**
 * Helper to call reasoning request with typed input.
 * Returns a string because the reasoning tool's output is a JSON string literal.
 */
export function callReasoningRequestTyped(input: ReasoningRequestInput): Promise<string>;
