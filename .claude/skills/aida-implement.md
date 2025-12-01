# AIDA Implementation Skill

## Purpose

Implement an approved requirement with full traceability, evolving the requirement database to capture implementation details and creating child requirements as needed.

## When to Use

Use this skill when:
- User says "implement <SPEC-ID>" or "work on <requirement>"
- User triggers "Copy for Claude Code" from the aida-gui AI menu
- An approved requirement is ready to be implemented
- Continuing implementation of a requirement from a previous session

## Core Principles

### Living Documentation
The requirements database should evolve during implementation to accurately reflect:
- What was actually built (vs. what was initially specified)
- Implementation decisions and trade-offs
- Child requirements discovered during development
- Technical constraints encountered

### Traceability
All AI-generated code must include inline traceability comments linking back to requirement IDs.

## Workflow

### Step 1: Load Requirement Context

Fetch the requirement details:

```bash
aida show <SPEC-ID>
```

Display to user:
- SPEC-ID and title
- Current description
- Status, priority, type
- Related requirements (parent/child, links)
- Any existing implementation notes

### Step 2: Analyze Implementation Scope

Before writing code:
1. Identify files that will be created or modified
2. Identify any sub-tasks or child requirements
3. Confirm approach with user if there are significant decisions

If the requirement is too broad, suggest splitting:
```bash
# Create child requirements
aida add --title "..." --description "..." --type functional --status draft

# Link as child
aida rel add <PARENT-ID> <CHILD-ID> --type Parent
```

### Step 3: Implement with Traceability

When writing or modifying code, add inline traceability comments:

**Rust:**
```rust
// trace:FR-0042 | ai:claude:high
fn implement_feature() {
    // Implementation here
}
```

**Python:**
```python
# trace:FR-0042 | ai:claude:high
def implement_feature():
    """Implementation of FR-0042."""
    pass
```

**TypeScript/JavaScript:**
```typescript
// trace:FR-0042 | ai:claude:high
function implementFeature() {
    // Implementation here
}
```

**Comment Format:**
```
// trace:<SPEC-ID> | ai:<tool>:<confidence>
```

Where:
- `<SPEC-ID>`: The requirement being implemented (e.g., FR-0042)
- `<tool>`: Always `claude` for Claude-generated code
- `<confidence>`: `high` (>80% AI), `med` (40-80%), `low` (<40%)

### Step 4: Update Requirement During Implementation

As you implement, update the requirement to reflect reality:

```bash
# Update description with implementation details
aida edit <SPEC-ID> --description "Updated description with implementation notes..."

# Add implementation notes to history
aida comment add <SPEC-ID> "Implementation note: Used async/await pattern for..."

# Update status as appropriate
aida edit <SPEC-ID> --status completed
```

### Step 5: Create Child Requirements

When implementation reveals sub-tasks:

```bash
# Add child requirement
aida add \
  --title "Handle edge case: empty input" \
  --description "The system shall handle empty input gracefully..." \
  --type functional \
  --status draft

# Link to parent
aida rel add <PARENT-ID> <NEW-CHILD-ID> --type Parent
```

### Step 6: Document Completion

When implementation is complete:

1. Update requirement status:
```bash
aida edit <SPEC-ID> --status completed
```

2. Add completion comment:
```bash
aida comment add <SPEC-ID> "Implementation complete. Files modified: src/foo.rs, src/bar.rs"
```

3. Create "Verifies" relationship if tests were added:
```bash
aida rel add <TEST-SPEC-ID> <SPEC-ID> --type Verifies
```

## Traceability Comment Examples

### New Function
```rust
// trace:FR-0123 | ai:claude:high
/// Calculate the total price with applicable discounts.
pub fn calculate_total(items: &[Item], discount: f64) -> f64 {
    items.iter().map(|i| i.price).sum::<f64>() * (1.0 - discount)
}
```

### Modified Section
```python
def process_order(order):
    # existing code...

    # trace:FR-0124 | ai:claude:med
    # Added validation per FR-0124
    if not order.items:
        raise ValueError("Order must contain at least one item")

    # existing code continues...
```

### Test Implementation
```rust
#[cfg(test)]
mod tests {
    // trace:FR-0123 | ai:claude:high | verifies:FR-0123
    #[test]
    fn test_calculate_total_with_discount() {
        let items = vec![Item { price: 100.0 }];
        assert_eq!(calculate_total(&items, 0.1), 90.0);
    }
}
```

## State Transitions

During implementation, requirements should transition through:

1. **Approved** -> **In Progress** (when starting implementation)
2. **In Progress** -> **Completed** (when implementation is verified)
3. **In Progress** -> **Draft** (if significant changes needed)

Update via:
```bash
aida edit <SPEC-ID> --status <new-status>
```

## Child Requirement Patterns

### Breaking Down Large Features
```
FR-0100: User Authentication System
  ├── FR-0101: Login form UI
  ├── FR-0102: Password validation
  ├── FR-0103: Session management
  └── FR-0104: Logout functionality
```

### Discovered Requirements
When implementation reveals needed functionality:
```bash
aida add --title "Rate limiting for login attempts" \
  --description "Discovered during FR-0100 implementation: Need rate limiting to prevent brute force attacks" \
  --type nonfunctional \
  --status draft

aida rel add FR-0100 <NEW-ID> --type Parent
```

## CLI Reference

```bash
# Show requirement
aida show <SPEC-ID>

# Edit requirement
aida edit <SPEC-ID> --description "..." --status <status>

# Add comment
aida comment add <SPEC-ID> "Comment text"

# Add relationship
aida rel add <FROM-ID> <TO-ID> --type <Parent|Verifies|References|Duplicate>

# Create new requirement
aida add --title "..." --description "..." --type <type> --status draft

# List requirements by feature
aida list --feature <feature-name>
```

## Example Implementation Session

```
User: Implement FR-0042 - User password reset

Claude: Loading FR-0042...

**FR-0042: User Password Reset**
Status: Approved
Priority: High
Description: The system shall allow users to reset their password via email.

This is a substantial feature. I recommend breaking it into child requirements:
1. FR-0042-A: Password reset request form
2. FR-0042-B: Email token generation and sending
3. FR-0042-C: Token validation endpoint
4. FR-0042-D: Password update form

Shall I create these child requirements and proceed?

User: Yes, go ahead

Claude: Creating child requirements...
[Creates 4 child requirements linked to FR-0042]

Starting with FR-0042-A: Password reset request form...

[Writes code with traceability comments]
// trace:FR-0042-A | ai:claude:high

[Updates requirement]
aida comment add FR-0042-A "Implemented reset request form in src/components/PasswordReset.tsx"

Moving to FR-0042-B...
```

## Integration with Git

When committing implementation:

```bash
git add .
git commit -m "feat(auth): implement password reset

Implements FR-0042 and child requirements:
- FR-0042-A: Reset request form
- FR-0042-B: Email token generation
- FR-0042-C: Token validation
- FR-0042-D: Password update form

Co-Authored-By: Claude <noreply@anthropic.com>"
```

## Notes

- This skill supersedes PROMPT_HISTORY.md - all implementation details go into the requirements database
- The requirement database becomes the source of truth for what was built and why
- Use comments on requirements liberally to capture decisions and context
- Child requirements create a natural hierarchy for complex features
