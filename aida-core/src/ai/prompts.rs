//! Prompt Templates for AI Operations
//!
//! This module builds structured prompts for various AI operations,
//! providing rich context from the requirements database.

use crate::models::{Requirement, RequirementsStore};

/// Build context about the project
fn build_project_context(store: &RequirementsStore) -> String {
    let total_reqs = store.requirements.len();
    let active_reqs = store
        .requirements
        .iter()
        .filter(|r| !r.archived)
        .count();

    let features: Vec<String> = store
        .features
        .iter()
        .map(|f| format!("{}-{}", f.number, f.name))
        .collect();

    let types: Vec<String> = store
        .type_definitions
        .iter()
        .map(|t| t.name.clone())
        .collect();

    format!(
        r#"## Project Context
- Project Name: {}
- Total Requirements: {} ({} active, {} archived)
- Features: {}
- Requirement Types: {}"#,
        if store.title.is_empty() {
            &store.name
        } else {
            &store.title
        },
        total_reqs,
        active_reqs,
        total_reqs - active_reqs,
        if features.is_empty() {
            "None defined".to_string()
        } else {
            features.join(", ")
        },
        if types.is_empty() {
            "Functional, NonFunctional, System, User, Epic, Story, Task".to_string()
        } else {
            types.join(", ")
        }
    )
}

/// Build context about related requirements
fn build_related_context(req: &Requirement, store: &RequirementsStore) -> String {
    let mut related = Vec::new();

    // Find parent requirements
    for rel in &req.relationships {
        if rel.rel_type.to_string() == "child" || rel.rel_type.to_string() == "parent" {
            if let Some(target) = store.requirements.iter().find(|r| r.id == rel.target_id) {
                related.push(format!(
                    "- {} ({}): {} [{}]",
                    target.spec_id.as_deref().unwrap_or("?"),
                    rel.rel_type,
                    target.title,
                    target.status
                ));
            }
        }
    }

    // Find requirements with same feature
    let same_feature: Vec<String> = store
        .requirements
        .iter()
        .filter(|r| r.id != req.id && r.feature == req.feature && !r.archived)
        .take(5)
        .map(|r| {
            format!(
                "- {} [{}]: {}",
                r.spec_id.as_deref().unwrap_or("?"),
                r.req_type,
                r.title
            )
        })
        .collect();

    let mut context = String::new();

    if !related.is_empty() {
        context.push_str("## Related Requirements\n");
        context.push_str(&related.join("\n"));
        context.push('\n');
    }

    if !same_feature.is_empty() {
        context.push_str("\n## Other Requirements in Same Feature\n");
        context.push_str(&same_feature.join("\n"));
        context.push('\n');
    }

    context
}

/// Build the current requirement as JSON context
fn requirement_to_context(req: &Requirement) -> String {
    format!(
        r#"## Current Requirement
- SPEC-ID: {}
- Title: {}
- Type: {}
- Status: {}
- Priority: {}
- Feature: {}
- Owner: {}
- Tags: {}

### Description
{}

### Existing Relationships
{}"#,
        req.spec_id.as_deref().unwrap_or("(not assigned)"),
        req.title,
        req.req_type,
        req.status,
        req.priority,
        req.feature,
        if req.owner.is_empty() {
            "(none)"
        } else {
            &req.owner
        },
        if req.tags.is_empty() {
            "(none)".to_string()
        } else {
            req.tags.iter().cloned().collect::<Vec<_>>().join(", ")
        },
        if req.description.is_empty() {
            "(no description provided)"
        } else {
            &req.description
        },
        if req.relationships.is_empty() {
            "(none)".to_string()
        } else {
            req.relationships
                .iter()
                .map(|r| format!("- {}: {}", r.rel_type, r.target_id))
                .collect::<Vec<_>>()
                .join("\n")
        }
    )
}

/// Build all requirements summary for duplicate detection
fn build_requirements_summary(store: &RequirementsStore, exclude_id: uuid::Uuid) -> String {
    let summaries: Vec<String> = store
        .requirements
        .iter()
        .filter(|r| r.id != exclude_id && !r.archived)
        .map(|r| {
            format!(
                "- {}: {} [{}] - {}",
                r.spec_id.as_deref().unwrap_or("?"),
                r.title,
                r.req_type,
                if r.description.len() > 100 {
                    format!("{}...", &r.description[..100])
                } else if r.description.is_empty() {
                    "(no description)".to_string()
                } else {
                    r.description.clone()
                }
            )
        })
        .collect();

    format!(
        "## All Requirements (for comparison)\n{}",
        summaries.join("\n")
    )
}

/// Build prompt for evaluating a requirement
pub fn build_evaluation_prompt(req: &Requirement, store: &RequirementsStore) -> String {
    let project_context = build_project_context(store);
    let req_context = requirement_to_context(req);
    let related_context = build_related_context(req, store);
    let config = &store.ai_prompts;
    let req_type = req.req_type.to_string();

    // Check for custom template
    if let Some(custom_template) = &config.evaluation.custom_template {
        return custom_template
            .replace("{project_context}", &project_context)
            .replace("{req_context}", &req_context)
            .replace("{related_context}", &related_context)
            .replace("{global_context}", &config.global_context)
            .replace("{req_type}", &req_type);
    }

    // Build with default template + customizations
    let global_context_section = if !config.global_context.is_empty() {
        format!("\n## Project-Specific Context\n{}\n", config.global_context)
    } else {
        String::new()
    };

    let additional_instructions = if !config.evaluation.additional_instructions.is_empty() {
        format!(
            "\n## Additional Instructions\n{}\n",
            config.evaluation.additional_instructions
        )
    } else {
        String::new()
    };

    let type_extra = config
        .get_type_evaluation_extra(&req_type)
        .map(|s| format!("\n## Type-Specific Instructions ({})\n{}\n", req_type, s))
        .unwrap_or_default();

    format!(
        r#"You are an expert requirements analyst evaluating a software requirement for quality and completeness.
{global_context_section}
{project_context}

{req_context}

{related_context}
{additional_instructions}{type_extra}
## Task
Evaluate this requirement and provide a structured assessment. Consider:
1. Clarity: Is the requirement clearly stated and unambiguous?
2. Completeness: Does it have sufficient detail for implementation?
3. Testability: Can this requirement be verified/tested?
4. Consistency: Does it align with related requirements?
5. Feasibility: Is it realistic and achievable?

## Response Format
Respond ONLY with valid JSON in this exact format:
```json
{{
  "quality_score": <1-10>,
  "issues": [
    {{
      "type": "<vague_language|missing_criteria|ambiguous|incomplete|inconsistent|untestable>",
      "severity": "<low|medium|high>",
      "text": "<description of the issue>",
      "suggestion": "<how to fix it>"
    }}
  ],
  "strengths": ["<strength1>", "<strength2>"],
  "suggested_improvements": {{
    "description": "<improved description text if needed, or null>",
    "rationale": "<why this improvement helps>"
  }}
}}
```

Provide your evaluation now:"#
    )
}

/// Build prompt for finding duplicates
pub fn build_duplicates_prompt(req: &Requirement, store: &RequirementsStore) -> String {
    let project_context = build_project_context(store);
    let req_context = requirement_to_context(req);
    let all_reqs = build_requirements_summary(store, req.id);
    let config = &store.ai_prompts;
    let req_type = req.req_type.to_string();

    // Check for custom template
    if let Some(custom_template) = &config.duplicates.custom_template {
        return custom_template
            .replace("{project_context}", &project_context)
            .replace("{req_context}", &req_context)
            .replace("{all_reqs}", &all_reqs)
            .replace("{global_context}", &config.global_context)
            .replace("{req_type}", &req_type);
    }

    // Build with default template + customizations
    let global_context_section = if !config.global_context.is_empty() {
        format!("\n## Project-Specific Context\n{}\n", config.global_context)
    } else {
        String::new()
    };

    let additional_instructions = if !config.duplicates.additional_instructions.is_empty() {
        format!(
            "\n## Additional Instructions\n{}\n",
            config.duplicates.additional_instructions
        )
    } else {
        String::new()
    };

    format!(
        r#"You are an expert requirements analyst identifying potential duplicate or overlapping requirements.
{global_context_section}
{project_context}

{req_context}

{all_reqs}
{additional_instructions}
## Task
Analyze the current requirement and compare it against all other requirements to find:
1. Exact duplicates (same functionality described differently)
2. Partial overlaps (requirements that cover similar ground)
3. Potential conflicts (requirements that contradict each other)

Only report requirements with similarity > 0.5 (50%).

## Response Format
Respond ONLY with valid JSON in this exact format:
```json
{{
  "potential_duplicates": [
    {{
      "spec_id": "<SPEC-ID of similar requirement>",
      "similarity": <0.0-1.0>,
      "reason": "<why these are similar>",
      "recommendation": "<merge|link|keep_separate|review>"
    }}
  ]
}}
```

If no duplicates found, return: {{"potential_duplicates": []}}

Provide your analysis now:"#
    )
}

/// Build prompt for suggesting relationships
pub fn build_relationships_prompt(req: &Requirement, store: &RequirementsStore) -> String {
    let project_context = build_project_context(store);
    let req_context = requirement_to_context(req);
    let all_reqs = build_requirements_summary(store, req.id);
    let config = &store.ai_prompts;
    let req_type = req.req_type.to_string();

    // Get relationship type definitions
    let rel_types: Vec<String> = store
        .relationship_definitions
        .iter()
        .map(|rd| format!("{}: {}", rd.name, rd.description))
        .collect();

    let rel_types_str = if rel_types.is_empty() {
        "parent, child, duplicate, verifies, verified_by, references".to_string()
    } else {
        rel_types.join("\n- ")
    };

    // Check for custom template
    if let Some(custom_template) = &config.relationships.custom_template {
        return custom_template
            .replace("{project_context}", &project_context)
            .replace("{req_context}", &req_context)
            .replace("{all_reqs}", &all_reqs)
            .replace("{rel_types}", &rel_types_str)
            .replace("{global_context}", &config.global_context)
            .replace("{req_type}", &req_type);
    }

    // Build with default template + customizations
    let global_context_section = if !config.global_context.is_empty() {
        format!("\n## Project-Specific Context\n{}\n", config.global_context)
    } else {
        String::new()
    };

    let additional_instructions = if !config.relationships.additional_instructions.is_empty() {
        format!(
            "\n## Additional Instructions\n{}\n",
            config.relationships.additional_instructions
        )
    } else {
        String::new()
    };

    format!(
        r#"You are an expert requirements analyst identifying missing relationships between requirements.
{global_context_section}
{project_context}

{req_context}

{all_reqs}

## Available Relationship Types
- {rel_types_str}
{additional_instructions}
## Task
Analyze the current requirement and suggest relationships that should exist but don't:
1. Dependencies (what must be done first)
2. Parent/child relationships (decomposition)
3. Verification relationships (what tests/validates this)
4. References (related but not dependent)

Only suggest relationships with confidence > 0.7 (70%).

## Response Format
Respond ONLY with valid JSON in this exact format:
```json
{{
  "suggested_relationships": [
    {{
      "rel_type": "<relationship type>",
      "target_spec_id": "<SPEC-ID of target requirement>",
      "confidence": <0.0-1.0>,
      "rationale": "<why this relationship should exist>"
    }}
  ]
}}
```

If no relationships to suggest, return: {{"suggested_relationships": []}}

Provide your analysis now:"#
    )
}

/// Build prompt for improving description
pub fn build_improve_prompt(req: &Requirement, store: &RequirementsStore) -> String {
    let project_context = build_project_context(store);
    let req_context = requirement_to_context(req);
    let related_context = build_related_context(req, store);
    let config = &store.ai_prompts;
    let req_type = req.req_type.to_string();

    // Find examples of well-written requirements (completed ones with descriptions)
    let examples: Vec<String> = store
        .requirements
        .iter()
        .filter(|r| {
            r.id != req.id
                && !r.archived
                && r.description.len() > 100
                && r.status.to_string() == "Completed"
        })
        .take(2)
        .map(|r| {
            format!(
                "### Example: {} ({})\n{}",
                r.title,
                r.req_type,
                &r.description[..r.description.len().min(300)]
            )
        })
        .collect();

    let examples_str = if examples.is_empty() {
        String::new()
    } else {
        format!(
            "## Examples of Well-Written Requirements\n{}",
            examples.join("\n\n")
        )
    };

    // Check for custom template
    if let Some(custom_template) = &config.improve.custom_template {
        return custom_template
            .replace("{project_context}", &project_context)
            .replace("{req_context}", &req_context)
            .replace("{related_context}", &related_context)
            .replace("{examples}", &examples_str)
            .replace("{global_context}", &config.global_context)
            .replace("{req_type}", &req_type);
    }

    // Build with default template + customizations
    let global_context_section = if !config.global_context.is_empty() {
        format!("\n## Project-Specific Context\n{}\n", config.global_context)
    } else {
        String::new()
    };

    let additional_instructions = if !config.improve.additional_instructions.is_empty() {
        format!(
            "\n## Additional Instructions\n{}\n",
            config.improve.additional_instructions
        )
    } else {
        String::new()
    };

    let type_extra = config
        .get_type_improve_extra(&req_type)
        .map(|s| format!("\n## Type-Specific Instructions ({})\n{}\n", req_type, s))
        .unwrap_or_default();

    format!(
        r#"You are an expert requirements analyst improving a requirement's description for clarity and completeness.
{global_context_section}
{project_context}

{req_context}

{related_context}

{examples_str}
{additional_instructions}{type_extra}
## Task
Improve the requirement's description to be:
1. Clear and unambiguous
2. Complete with acceptance criteria where appropriate
3. Testable/verifiable
4. Consistent with the requirement type ({})
5. Professional and well-structured

Do NOT change the meaning or scope of the requirement.

## Response Format
Respond ONLY with valid JSON in this exact format:
```json
{{
  "improved_description": "<the improved description text>",
  "changes_made": ["<change1>", "<change2>"],
  "rationale": "<why these improvements help>"
}}
```

Provide your improved version now:"#,
        req_type
    )
}

/// Build prompt for generating child requirements
pub fn build_generate_children_prompt(req: &Requirement, store: &RequirementsStore) -> String {
    let project_context = build_project_context(store);
    let req_context = requirement_to_context(req);
    let config = &store.ai_prompts;
    let req_type = req.req_type.to_string();

    // Find existing children
    let existing_children: Vec<String> = req
        .relationships
        .iter()
        .filter(|r| r.rel_type.to_string() == "parent")
        .filter_map(|r| store.requirements.iter().find(|req| req.id == r.target_id))
        .map(|r| format!("- {}: {}", r.spec_id.as_deref().unwrap_or("?"), r.title))
        .collect();

    let existing_str = if existing_children.is_empty() {
        "(none yet)".to_string()
    } else {
        existing_children.join("\n")
    };

    // Get available types
    let types: Vec<String> = store
        .type_definitions
        .iter()
        .map(|t| t.name.clone())
        .collect();

    let types_str = if types.is_empty() {
        "Functional, NonFunctional, System, User, Task, Story".to_string()
    } else {
        types.join(", ")
    };

    // Check for custom template
    if let Some(custom_template) = &config.generate_children.custom_template {
        return custom_template
            .replace("{project_context}", &project_context)
            .replace("{req_context}", &req_context)
            .replace("{existing_children}", &existing_str)
            .replace("{available_types}", &types_str)
            .replace("{global_context}", &config.global_context)
            .replace("{req_type}", &req_type);
    }

    // Build with default template + customizations
    let global_context_section = if !config.global_context.is_empty() {
        format!("\n## Project-Specific Context\n{}\n", config.global_context)
    } else {
        String::new()
    };

    let additional_instructions = if !config.generate_children.additional_instructions.is_empty() {
        format!(
            "\n## Additional Instructions\n{}\n",
            config.generate_children.additional_instructions
        )
    } else {
        String::new()
    };

    let type_extra = config
        .get_type_generate_children_extra(&req_type)
        .map(|s| format!("\n## Type-Specific Instructions ({})\n{}\n", req_type, s))
        .unwrap_or_default();

    format!(
        r#"You are an expert requirements analyst breaking down a high-level requirement into implementable sub-requirements.
{global_context_section}
{project_context}

{req_context}

## Existing Children
{existing_str}

## Available Requirement Types
{types_str}
{additional_instructions}{type_extra}
## Task
Break down this requirement into smaller, implementable sub-requirements. Consider:
1. Logical decomposition of functionality
2. Separation of concerns (UI, backend, data, etc.)
3. Testability of each sub-requirement
4. Appropriate granularity for implementation

Generate 3-7 sub-requirements that together fully cover the parent requirement.

## Response Format
Respond ONLY with valid JSON in this exact format:
```json
{{
  "suggested_children": [
    {{
      "title": "<concise title>",
      "description": "<clear description with acceptance criteria>",
      "type": "<requirement type>",
      "rationale": "<why this is a distinct sub-requirement>"
    }}
  ]
}}
```

Provide your breakdown now:"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RequirementPriority, RequirementStatus, RequirementType};

    fn create_test_req() -> Requirement {
        let mut req = Requirement::new("User Login".to_string(), "Users should be able to log in".to_string());
        req.spec_id = Some("FR-001".to_string());
        req.status = RequirementStatus::Draft;
        req.priority = RequirementPriority::High;
        req.req_type = RequirementType::Functional;
        req.feature = "Authentication".to_string();
        req
    }

    fn create_test_store() -> RequirementsStore {
        let mut store = RequirementsStore::default();
        store.name = "test-project".to_string();
        store.title = "Test Project".to_string();
        store
    }

    #[test]
    fn test_build_evaluation_prompt() {
        let req = create_test_req();
        let store = create_test_store();
        let prompt = build_evaluation_prompt(&req, &store);

        assert!(prompt.contains("User Login"));
        assert!(prompt.contains("FR-001"));
        assert!(prompt.contains("quality_score"));
    }

    #[test]
    fn test_build_duplicates_prompt() {
        let req = create_test_req();
        let store = create_test_store();
        let prompt = build_duplicates_prompt(&req, &store);

        assert!(prompt.contains("duplicate"));
        assert!(prompt.contains("similarity"));
    }
}
