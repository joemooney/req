# Integration Summary: requirements-manager + ai-provenance

## Decision

**Keep Both Projects and Integrate Them**

After reviewing both projects, we've decided to integrate requirements-manager (Rust) with ai-provenance (Python) rather than consolidating into a single tool.

## Rationale

### Why Keep Both?

**requirements-manager** strengths:
- ✅ Mature, functional requirements database
- ✅ Rich CLI with interactive/non-interactive modes
- ✅ Multi-project registry support
- ✅ Type-safe Rust implementation
- ✅ Feature-based organization with auto-numbering
- ✅ Clean data model with UUID identifiers

**ai-provenance** strengths:
- ✅ Git integration and commit metadata
- ✅ AI code attribution and provenance tracking
- ✅ Traceability matrices
- ✅ Compliance and audit capabilities
- ✅ Inline code metadata parsing
- ✅ Historical reconstruction

### Why Integrate?

**Minimal Overlap**: The tools solve different problems:
- requirements-manager → Requirements database (what to build)
- ai-provenance → Code provenance tracking (how it was built)

**Complementary**: Together they provide:
- Single source of truth for requirements
- Full traceability from requirements → code → tests
- AI attribution at every level
- Multi-project support
- Comprehensive audit trails

## Integration Architecture

```
┌─────────────────────────┐
│  requirements-manager   │  ← Source of Truth
│  (Rust)                 │     - Requirements CRUD
│  requirements.yaml      │     - Multi-project registry
└───────────┬─────────────┘
            │
            │ Export to ai-prov format
            │ UUID → SPEC-ID mapping
            ▼
┌─────────────────────────┐
│  ai-provenance          │  ← Traceability Layer
│  (Python)               │     - Git integration
│  Reads requirements.yaml│     - Commit metadata
└─────────────────────────┘     - Trace matrices
```

## Key Components

### 1. Export System (requirements-manager)

Add `export` command to requirements-manager:

```bash
requirements-manager export --format ai-prov
```

Creates:
- `.ai-prov/requirements/SPEC-{N}.json` files
- `.requirements-mapping.yaml` (UUID ↔ SPEC-ID)

### 2. ID Mapping

Maintains bidirectional mapping:
- UUID (requirements-manager) ↔ SPEC-ID (ai-provenance)
- Persistent across exports
- Sequential SPEC numbering

### 3. Adapter (ai-provenance)

Python adapter to read requirements.yaml:

```python
from ai_provenance.requirements.adapters import RequirementsManagerAdapter

adapter = RequirementsManagerAdapter()
requirements = adapter.load_requirements()
```

### 4. Configuration

`.ai-prov/config.yaml`:
```yaml
requirements:
  source: requirements-manager
  path: requirements.yaml
  mapping: .requirements-mapping.yaml
```

## Workflows

### Workflow 1: Create Requirement → Implement → Track

```bash
# 1. Create requirement
requirements-manager add -i
# → Feature: Authentication
# → Title: JWT token system
# → Output: Created SPEC-089

# 2. Implement with AI
# (use Claude, Copilot, etc.)

# 3. Commit with traceability
ai-prov commit -m "feat: JWT tokens" --trace SPEC-089 --tool claude

# 4. Generate matrix
ai-prov trace-matrix
# Shows: SPEC-089 → src/auth.py → abc123 → TC-210 → 85% AI
```

### Workflow 2: Multi-Project Support

```bash
# Register projects
requirements-manager db register \
  --name ai-provenance \
  --path ~/ai/ai-provenance/requirements.yaml

requirements-manager db register \
  --name requirements-manager \
  --path ~/ai/req/requirements-manager/requirements.yaml

# Switch between projects
export REQ_DB_NAME=ai-provenance
requirements-manager list
ai-prov trace-matrix
```

## Implementation Status

### Completed ✅
- [x] Integration architecture design
- [x] Data model mapping specification
- [x] Documentation (INTEGRATION.md in both repos)
- [x] Detailed implementation plan (IMPLEMENTATION_PLAN.md)
- [x] Committed and pushed to GitHub

### Remaining Work (Estimated: 15 hours)

**Phase 1: Export Functionality** (4 hours)
- [ ] Add `export` command to requirements-manager CLI
- [ ] Implement export module in Rust
- [ ] Support ai-prov format export
- [ ] Unit tests

**Phase 2: ID Mapping** (2 hours)
- [ ] Define mapping file format
- [ ] Implement mapping module
- [ ] Add mapping CLI commands

**Phase 3: Adapter** (3.75 hours)
- [ ] Create RequirementsManagerAdapter in Python
- [ ] Update RequirementManager to use adapter
- [ ] Add configuration support

**Phase 4: CLI Integration** (1 hour)
- [ ] Update ai-prov commands to read from requirements.yaml
- [ ] Add sync command

**Phase 5: Testing & Docs** (4 hours)
- [ ] Integration tests
- [ ] Update READMEs
- [ ] Create example project

## Next Steps

1. **Review** integration plan with stakeholders
2. **Prioritize** tasks (see IMPLEMENTATION_PLAN.md for priorities)
3. **Implement** Phase 1 (export functionality)
4. **Test** export → import workflow
5. **Iterate** based on real usage

## Benefits

### For Users

1. **Single source of truth**: All requirements in requirements.yaml
2. **Rich tooling**: Use best tool for each job
3. **Multi-project**: Manage multiple projects from one registry
4. **Full traceability**: Requirements → code → tests → AI metrics
5. **No lock-in**: Both tools can work independently

### For Developers

1. **Separation of concerns**: Clear boundaries between tools
2. **Language choice**: Rust for performance, Python for git integration
3. **Maintainability**: Smaller, focused codebases
4. **Testability**: Independent testing of each component
5. **Flexibility**: Can swap out components if needed

## Files Created

### In requirements-manager repo
- `INTEGRATION.md` - Integration architecture and workflows
- `IMPLEMENTATION_PLAN.md` - Detailed task breakdown (15 tasks)
- `INTEGRATION_SUMMARY.md` - This file

### In ai-provenance repo
- `INTEGRATION.md` - User guide for integration

## Resources

- **requirements-manager**: `/home/joe/ai/req/requirements-manager/`
- **ai-provenance**: `/home/joe/ai/ai-provenance/`
- **Integration docs**: See INTEGRATION.md in both repos
- **Implementation plan**: `/home/joe/ai/req/IMPLEMENTATION_PLAN.md`

## Questions?

See:
- `INTEGRATION.md` - Architecture details
- `IMPLEMENTATION_PLAN.md` - Task breakdown and estimates
- `ai-provenance/INTEGRATION.md` - User guide

---

**Status**: Planning Complete, Ready for Implementation
**Last Updated**: 2025-11-22
**Estimated Effort**: 15 hours across 5 phases
