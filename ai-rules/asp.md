# AI Safety Policy (ASP)

Applies To: all AI-assisted work in this repository
When to Load: always (especially before commits, scripts, and risky changes)
Priority: must

## Intent

Prevent AI-assisted work from causing data loss, security breaches, or system outages.

## Must

- Never commit secrets (keys, tokens, passwords, `.env`, `*.pem`, `*.p12`, etc.)
- Run ASP preflight before any commit:

```bash
./scripts/utilities/asp-preflight.sh --staged --strict
```

- Scan for secrets before committing:

```bash
git secrets --scan --cached
```

- Never change data storage paths without explicit approval and a migration plan
- Preserve backward compatibility for data-related changes
- No AI attribution in commits, code, or docs

## Should

- Use placeholders for credential examples (for example: `"YOUR_API_KEY_HERE"`)
- Update security notes and policy docs when a new risk is discovered

## Checklist

- [ ] Ran ASP preflight on staged files
- [ ] Ran secret scan on staged files
- [ ] No data path changes without migration plus approval
- [ ] No AI attribution in commit or docs
