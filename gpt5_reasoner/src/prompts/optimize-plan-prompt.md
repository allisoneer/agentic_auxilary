<optimize_prompt>
  <input_data>
    <files>
      {FILES_ARRAY}
      <!-- Array of {filename: string, description: string} objects -->
      <!-- Will ALWAYS include plan_structure.md or similar template file -->
    </files>

    <original_prompt>
      {USER_PROMPT}
      <!-- Simple request like "Create a plan to refactor the authentication system" -->
    </original_prompt>

    <type>plan</type>
  </input_data>

  <ultra_thinking>
    You are optimizing a prompt for GPT-5 to generate structured plans. You will:

    1. Identify the plan template file (usually plan_structure.md or similar)
    2. Separate template/format files from implementation context files
    3. Group context files by their role in plan generation
    4. Create a prompt template with bookending focused on plan output requirements
    5. Ensure the plan format is reinforced at beginning AND end

    The actual file contents will be injected by deterministic code - you work with metadata only.
  </ultra_thinking>

  <optimization_requirements>
    <!-- Special Plan File Grouping -->
    <file_grouping>
      Create these specific groups for plan generation:
      - "plan_template": The plan structure/format file (MUST be first)
      - "implementation_targets": Files that will be modified in the plan
      - "architectural_context": System design and structure files
      - "reference_examples": Similar implementations or patterns
      - "constraints_and_dependencies": Files that define limitations

      The plan_template group is CRITICAL and must be clearly separated.
    </file_grouping>

    <!-- Plan-Specific GPT-5 Optimizations -->
    <plan_optimizations>
      1. DOUBLE BOOKENDING: Both task AND format requirements at start and end
      2. TEMPLATE PROMINENCE: Plan structure file immediately after first task statement
      3. CONTEXT HIERARCHY: Template → Targets → Architecture → Examples
      4. FORMAT REINFORCEMENT: Explicit "follow this exact format" instructions
      5. STRUCTURED OUTPUT: Multiple reminders to use the provided template
      6. VALIDATION CHECKLIST: End with "ensure your plan follows the template"
    </plan_optimizations>

    <!-- Output Format Requirements -->
    <output_format>
      Your response must include:

      1. FILE_GROUPING (YAML with plan-specific groups):
      ```yaml
      file_groups:
        - name: "plan_template"
          purpose: "MANDATORY format for the plan output"
          critical: true
          files:
            - "thoughts/docs/plan_structure.md"
        - name: "implementation_targets"
          purpose: "Files that need modification"
          files:
            - "path/to/target1"
            - "path/to/target2"
        - name: "architectural_context"
          purpose: "System design understanding"
          files:
            - "path/to/architecture"
      ```

      2. OPTIMIZED_TEMPLATE:
      ```xml
      <reasoning_effort>high</reasoning_effort>

      <!-- BOOKEND START: Task and Format Requirements -->
      <primary_task>{original_prompt}</primary_task>
      <output_format_requirement>
        Generate a structured plan following the EXACT format provided in the plan template.
      </output_format_requirement>

      <!-- CRITICAL: Plan Template First -->
      <plan_template>
        <!-- GROUP: plan_template -->
      </plan_template>

      <!-- Implementation Context -->
      <implementation_context>
        <!-- GROUP: implementation_targets -->
        <!-- GROUP: architectural_context -->
        <!-- GROUP: reference_examples -->
        <!-- GROUP: constraints_and_dependencies -->
      </implementation_context>

      <plan_generation_instructions>
        <think_harder>
          Analyze all files to understand the system's current state and constraints.
          Design a plan that addresses the task while respecting existing architecture.
        </think_harder>

        <structure_requirements>
          - Follow the template structure EXACTLY
          - Include all required sections from the template
          - Provide concrete, actionable steps
          - Consider dependencies and order of operations
        </structure_requirements>
      </plan_generation_instructions>

      <!-- BOOKEND END: Reinforcement -->
      <primary_task>{original_prompt}</primary_task>
      <format_validation>
        Your plan MUST follow the exact structure shown in the plan_template.
        Verify all required sections are present before finalizing.
      </format_validation>
      ```
    </output_format>
  </optimization_requirements>

  <meta_instructions>
    Key differences for plan optimization:
    1. Plan template file gets special prominence and repeated emphasis
    2. Double bookending: both task AND format requirements
    3. Explicit validation reminders at the end
    4. Hierarchical grouping that prioritizes format over content

    You're creating structure, not content. The <!-- GROUP: name --> markers show
    where file contents will be injected. Focus on ensuring the plan template is
    unmissable and the output format is crystal clear.
  </meta_instructions>
</optimize_prompt>