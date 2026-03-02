# AI Safety Policy (ASP)

Purpose: prevent AI-assisted work from causing data loss, secret exposure, or system instability.

Scope: all AI-assisted changes in this repository.

## Mandatory Safeguards

| Area | Control | Required Action |
|------|---------|-----------------|
| Secrets | Pre-commit scanning | Run `git secrets --scan --cached` and ASP preflight |
| Commits | Preflight checks | Run `./scripts/utilities/asp-preflight.sh --staged --strict` |
| Batch file ops | Dry-run + sample test | Test with sample data before broad rename/move/delete operations |
| Data paths | Approval + migration | Do not change storage paths without approval and migration plan |
| System/display changes | High-risk gate | Require explicit approval, backup, rollback plan, and recovery verification |

## Commit Workflow (Required)

1. Review staged diff (`git diff --cached`).
2. Run ASP preflight:
   - `./scripts/utilities/asp-preflight.sh --staged --strict`
3. Run git-secrets scan:
   - `git secrets --scan --cached`
4. Confirm no credentials are staged.

Never commit real secrets (`.env`, `*.pem`, `*.key`, `*.p12`, private keys, real tokens).

## Enforcement Tooling

Install the repository pre-commit hook:

```bash
./scripts/automation/install-asp-hooks.sh . --force
```

The installed hook runs:
- `git secrets --scan --cached`
- `./scripts/utilities/asp-preflight.sh --staged --strict`

## Preflight Checklist

Use this checklist for risky changes and commit prep:
- `docs/security/asp-preflight-checklist.md`
- `ai-rules/incident-remediations.md`
