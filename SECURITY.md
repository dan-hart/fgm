# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in fgm, please report it responsibly:

1. **Do NOT open a public issue**
2. Use [GitHub's private vulnerability reporting](https://github.com/dan-hart/fgm/security/advisories/new)
3. Include: description, steps to reproduce, potential impact

## Response Timeline

- Acknowledgment: within 48 hours
- Initial assessment: within 1 week
- Fix timeline: depends on severity

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅        |

## Security Best Practices

When using fgm:

- **Never commit your Figma token** to version control
- Use `fgm auth login` to store tokens securely in your system keychain
- If using environment variables, set `FIGMA_TOKEN` in your shell session, not in committed config files
- The `.gitignore` includes patterns to prevent accidental credential commits
- `git-secrets` hooks are configured to catch common token patterns
