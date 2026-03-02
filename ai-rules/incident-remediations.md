# Incident Remediations (Biggest AI Mistakes)

Applies To: all tasks
When to Load: always
Priority: must

## Intent

Encode hard-learned lessons from major AI incidents so they do not recur.

## Must

### 1) Batch operation data loss
Failure: faulty batch rename/move/delete operations can overwrite large sets of files.
Rule: never run batch operations without dry-run, sample test, and verified backup.

### 2) API key exposure
Failure: secrets committed to a public repository.
Rule: never commit secrets; run ASP preflight and secret scans before commit.

```bash
./scripts/utilities/asp-preflight.sh --staged --strict
git secrets --scan --cached
```

### 3) Data path change regression
Failure: storage path changes break access to existing user data.
Rule: never change data storage paths without explicit approval and a migration plan; preserve backward compatibility.

### 4) Risky system changes without rollback
Failure: system-level changes can cause outages or boot/display failures.
Rule: no high-risk system changes without backup, rollback plan, and explicit approval.

### 5) Unapproved restarts
Failure: unplanned restarts can disrupt active user work.
Rule: never restart a machine without explicit user approval.

## Checklist

- [ ] Dry-run + sample test + backup verification for batch operations
- [ ] ASP preflight + secret scan before commits
- [ ] Data path changes reviewed with migration plan
- [ ] High-risk system changes gated by approval + rollback
- [ ] Explicit approval for any restart
