<optimize_prompt>
  <input_data>
    <files>
      {FILES_ARRAY}
      <!-- Array of {filename: string, description: string} objects -->
    </files>

    <original_prompt>
      {USER_PROMPT}
      <!-- Simple user request like "Update our rust justfile execution" -->
    </original_prompt>

    <type>reasoning</type>
  </input_data>

  <ultra_thinking>
    You are optimizing a prompt for GPT-5 high reasoning mode. You will:

    1. Analyze the file metadata (names and descriptions only - you don't see contents)
    2. Group files intelligently based on their role in the task
    3. Create a prompt template showing optimal structure
    4. Apply GPT-5's bookending strategy (task at beginning AND end)
    5. Output parseable grouping decisions and template structure

    Remember: The actual file contents will be injected later by deterministic code.
  </ultra_thinking>

  <optimization_requirements>
    <!-- File Grouping Strategy -->
    <file_grouping>
      Analyze the filenames and descriptions to create logical groups:
      - "primary_targets": Files directly mentioned in the task
      - "reference_implementations": Similar files that serve as examples
      - "architectural_context": Supporting files that provide structure understanding
      - "dependencies": Files that might be affected or provide constraints

      Group files by their semantic role, not just location. A file's description often reveals its purpose.
    </file_grouping>

    <!-- GPT-5 Optimization Rules -->
    <gpt5_optimizations>
      1. BOOKENDING: Place the task at BOTH beginning and end of the prompt
      2. CONTEXT PLACEMENT: All file contents go at the beginning (after first task statement)
      3. XML STRUCTURE: Use clear XML tags for organization
      4. THINKING TRIGGERS: Add <think_harder> blocks for complex reasoning
      5. AVOID MIDDLE: Don't place critical info in the middle third of the prompt
      6. HIERARCHICAL: Most important files first within each group
    </gpt5_optimizations>

    <!-- Output Format Requirements -->
    <output_format>
      Your response must include two sections:

      1. FILE_GROUPING (YAML format for easy parsing):
      ```yaml
      file_groups:
        - name: "primary_targets"
          purpose: "Files directly related to the task"
          files:
            - "path/to/file1"
            - "path/to/file2"
        - name: "reference_implementations"
          purpose: "Examples to learn from"
          files:
            - "path/to/example"
      ```

      2. OPTIMIZED_TEMPLATE (with injection markers):
      ```xml
      <reasoning_effort>high</reasoning_effort>

      <!-- BOOKEND START -->
      <primary_task>{original_prompt}</primary_task>

      <!-- FILE CONTEXT SECTION -->
      <codebase_context>
        <!-- GROUP: primary_targets -->
        <!-- GROUP: reference_implementations -->
        <!-- GROUP: architectural_context -->
      </codebase_context>

      <analysis_requirements>
        <!-- Specific analysis based on task -->
      </analysis_requirements>

      <think_harder>
        <!-- Deep reasoning triggers -->
      </think_harder>

      <!-- BOOKEND END -->
      <primary_task>{original_prompt}</primary_task>
      <critical_focus>
        <!-- Key requirements extracted from task -->
      </critical_focus>
      ```
    </output_format>
  </optimization_requirements>

  <meta_instructions>
    Remember you're creating a TEMPLATE. The <!-- GROUP: name --> markers show where
    file contents will be injected. You decide the groupings based solely on filenames
    and descriptions. The deterministic code will handle reading files and injecting content.

    Your job is architectural: determine optimal structure, grouping, and placement for
    maximum GPT-5 performance on the given task.
  </meta_instructions>
</optimize_prompt>