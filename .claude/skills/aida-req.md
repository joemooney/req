# AIDA Requirement Creation Skill

## Purpose

Add a new requirement to the AIDA requirements database with AI-powered evaluation feedback.

## When to Use

Use this skill when:
- User wants to add a new requirement or feature request
- User describes something they want the system to do
- User has an idea that should be captured as a requirement
- User asks to "add a requirement" or "create a spec"

## Workflow

### Step 1: Gather Requirement Information

Ask the user for the following information (in conversational style):

1. **Description** (required): What should the system do? This can be:
   - A formal requirement: "The system shall..."
   - A question or idea to be formalized
   - A rough note that needs refinement

2. **Type** (optional, default: Functional):
   - Functional (FR) - System behaviors
   - NonFunctional (NFR) - Quality attributes (performance, security)
   - User (UR) - User needs/goals
   - System (SR) - Technical constraints
   - ChangeRequest (CR) - Modifications to existing features

3. **Priority** (optional, default: Medium):
   - High, Medium, Low

4. **Feature** (optional): Which feature area does this belong to?

5. **Tags** (optional): Comma-separated keywords

### Step 2: Add Requirement to Database

Use the `aida` CLI to add the requirement immediately:

```bash
aida add \
  --title "<generated-title>" \
  --description "<user-description>" \
  --type <type> \
  --priority <priority> \
  --status draft \
  --feature "<feature>" \
  --tags "<tags>"
```

**Title Generation**: Generate a concise title (5-10 words) from the description that captures the essence of the requirement.

### Step 3: Show Confirmation

After adding, display:
```
Requirement added: <SPEC-ID>
Title: <title>
Status: Draft (evaluation pending...)
```

### Step 4: Run AI Evaluation

Evaluate the requirement quality using the AI evaluation prompt. The evaluation should assess:

1. **Clarity** (1-10): Is the requirement clear and unambiguous?
2. **Testability** (1-10): Can this requirement be verified?
3. **Completeness** (1-10): Does it include all necessary information?
4. **Consistency** (1-10): Does it conflict with other requirements?

Provide:
- Overall quality score
- Issues found (if any)
- Suggestions for improvement
- Whether this should be split into multiple requirements

### Step 5: Offer Follow-up Actions

Based on the evaluation, offer:
- **Improve**: Let AI suggest improved description text
- **Split**: Generate child requirements if too broad
- **Link**: Suggest relationships to existing requirements
- **Accept**: Keep as-is and approve

## Example Session

```
User: "I want users to be able to export their data"

Claude: I'll add this as a new requirement.

Let me clarify a few details:
- Type: This sounds like a Functional requirement - is that correct?
- Priority: What priority should this have? (High/Medium/Low)
- Feature: Should this go under a specific feature area?

User: Yes functional, high priority, put it under User Management

Claude: Adding requirement...

Requirement added: FR-0149
Title: User Data Export Capability
Status: Draft (evaluation pending...)

Running AI evaluation...

**Evaluation Results:**
- Clarity: 6/10 - Good concept but needs specifics
- Testability: 5/10 - Missing acceptance criteria
- Completeness: 4/10 - Missing details

**Issues:**
- No specified export formats (CSV, JSON, PDF?)
- No data scope defined (all data? specific types?)
- No retention/download requirements

**Suggestions:**
1. Specify supported export formats
2. Define what data can be exported
3. Add acceptance criteria

Would you like me to:
1. **Improve** - Generate an improved description
2. **Split** - Create separate requirements for each format
3. **Accept** - Keep as-is for now
```

## CLI Reference

```bash
# Add requirement
aida add --title "..." --description "..." --type functional --priority high --status draft

# Show requirement details
aida show <SPEC-ID>

# Edit requirement
aida edit <SPEC-ID> --description "..."

# List features
aida feature list
```

## Integration Notes

- Requirements are stored in `requirements.yaml` or the configured project database
- SPEC-IDs are auto-generated based on type prefix configuration
- The GUI (aida-gui) can be used to view and manage requirements with full AI features
