# ASP Preflight Checklist

DATE: [YYYY-MM-DD]
CONTEXT: [Commit | Script | System change | Other]
RISK LEVEL: [Low | Medium | High | Critical]

## 1) Data Safety
- [ ] Batch ops tested on sample data
- [ ] Dry-run output reviewed in full
- [ ] Backup verified
- [ ] Rollback plan documented

## 2) Secrets and Privacy
- [ ] `git secrets --scan --cached` passed
- [ ] No `.env`, `*.pem`, `*.key`, `*.p12`, or secret files staged
- [ ] No real credentials in code or docs (placeholders only)

## 3) Data Integrity
- [ ] No storage path changes, or migration plan documented
- [ ] Existing data location confirmed
- [ ] Backward compatibility preserved

## 4) System and Display Safety
- [ ] Backup, approval, and rollback requirements satisfied
- [ ] Recovery path verified before risky system/display changes
- [ ] Changes tested one at a time

## 5) Review and Approval
- [ ] Staged diff reviewed
- [ ] High-risk change explicitly approved
- [ ] Monitoring plan in place during execution

## 6) Documentation
- [ ] Changes documented (what/why/how)
- [ ] Incident notes captured if needed

## Preflight Command

```bash
./scripts/utilities/asp-preflight.sh --staged --strict
```
