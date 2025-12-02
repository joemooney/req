# AIDA Session Capture Skill

## Purpose

Review the current conversation and capture any requirements, features, or implementation work that was discussed but not yet added to the AIDA requirements database. This is a "safety net" skill for conversational workflows.

## When to Use

Use this skill when:
- End of a development session
- User asks to "capture what we did" or "update requirements"
- User worked conversationally without explicit /aida-req calls
- Before ending a session to ensure nothing was missed

## Workflow

### Step 1: Scan the Conversation

Review the conversation history for:

1. **Implemented Features**: Code that was written or modified
2. **Bug Fixes**: Issues that were identified and resolved
3. **New Ideas**: Features or improvements discussed but not yet implemented
4. **Decisions Made**: Architectural or design choices that should be documented

Look for patterns like:
- "Add a feature that..."
- "Fix the bug where..."
- "I'd like to..."
- "Let's implement..."
- "Change X to Y"
- Code commits with feature descriptions

### Step 2: Check Against Requirements Database

For each item found, check if it already exists:

```bash
aida list --status all
aida show <suspected-ID>
```

Categorize findings:
- **Missing**: Not in database at all
- **Outdated**: In database but status/description needs update
- **Captured**: Already properly recorded

### Step 3: Present Summary to User

Format:

```
## Session Review

### Implemented (needs capture/update)
1. ✦ Smart parent selection for new requirements
   - Status: Not in database
   - Suggested: Add as FR with status=completed

2. ✦ Icon editor 4-column layout
   - Status: FR-0161 exists but status=draft
   - Suggested: Update to status=completed

### Already Captured
3. ✔ Auto-populate user settings (FR-0167)

### Discussed (not implemented)
4. ? Export to REQUIREMENTS.md feature
   - Status: Not in database
   - Suggested: Add as FR with status=draft

Would you like me to:
1. Add all missing requirements
2. Update outdated statuses
3. Review one by one
4. Skip capture
```

### Step 4: Execute Captures

Based on user choice, execute the appropriate commands:

**Adding new requirement:**
```bash
aida add \
  --title "Smart parent selection for new requirements" \
  --description "Add button uses smart heuristic: if selected has children, new req becomes child; if selected has no children but has parent, new req becomes sibling; if selected has no children and no parent, new req is root." \
  --status completed \
  --type functional \
  --feature "GUI"
```

**Updating existing requirement:**
```bash
aida edit FR-0161 --status completed
aida comment add FR-0161 "Implemented in session - 4-column grid layout for status/priority icons"
```

### Step 5: Confirm Completion

After all captures:

```
## Capture Complete

Added:
- FR-0168: Smart parent selection for new requirements

Updated:
- FR-0161: Status changed to completed

The requirements database is now in sync with this session's work.
```

## Guidelines

### What to Capture

- **Features**: New functionality added to the system
- **Enhancements**: Improvements to existing features
- **Bug Fixes**: Issues that were resolved
- **UI Changes**: Visual or interaction changes
- **Configuration**: New settings or options

### What NOT to Capture

- Refactoring with no user-visible changes (unless significant)
- Typo fixes or minor documentation updates
- Exploratory work that was abandoned
- Conversations about approach that didn't result in changes

### Status Assignment

- `draft` - Discussed but not implemented
- `completed` - Fully implemented and working
- `approved` - Agreed upon, ready for implementation

### Title Guidelines

- Use imperative mood: "Add...", "Fix...", "Update..."
- Keep concise: 5-10 words
- Focus on what, not how

## Example Session

```
User: /aida-capture

Claude: Let me review our session...

## Session Review

I found 4 items discussed in this session:

### Implemented (needs capture)
1. ✦ Settings/AI has Prompts and Skills subtabs
   - Created AiSubTab enum, show_ai_skills_subtab function
   - Loads and displays .md files from .claude/skills/
   - NOT in database

2. ✦ Use ✦ as status icon for AI
   - Added to StatusIconConfig::default()
   - NOT in database

### Already Captured
3. ✔ Smart parent selection (FR-0165) - status=completed

### Discussed (not implemented)
4. ? Export requirements to markdown
   - Mentioned but not implemented

Would you like me to add items 1-2 as completed requirements?

User: Yes, add them

Claude: Adding requirements...

Added:
- FR-0166: Settings/AI has Prompts and Skills subtabs
- FR-0167: Use ✦ as status icon for AI

Session capture complete!
```

## CLI Reference

```bash
# List all requirements
aida list

# Check specific requirement
aida show <SPEC-ID>

# Add new requirement
aida add --title "..." --description "..." --status <status> --type <type>

# Update requirement
aida edit <SPEC-ID> --status <status>

# Add comment
aida comment add <SPEC-ID> "Implementation note..."
```
