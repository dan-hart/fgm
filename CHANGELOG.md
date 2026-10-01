# Changelog

## 1.4.0

- Add iOS simulator and Android capture, including direct compare-url capture.
- Apply project source, aliases, scale, export directory, threshold and snapshot defaults.
- Add frame/component name search with page filters and unambiguous alias saving.
- Add mobile visual review with crop, masks, explicit normalization, overlay and changed bounds.
- Improve semantic Swift/Kotlin token identifiers, opacity, string escaping and typography helpers.
- Add mode-aware variables export with alias resolution and offline JSON fallback.
- Add scoped agent packs with full node metadata, contact sheets and changed-only writes.
- Add offline component source-path/symbol coverage checks.
- Add isolated named accounts for credentials, settings, cache and keychain.
- Add portable offline HTML review bundles, optional opening and source-metadata sanitization.
- Fail image-size mismatches consistently; refresh watch polling and invalidate changed-file caches.
- Avoid forwarding Figma authentication headers to asset download hosts.

Live variables access still depends on Figma permissions and plan capabilities.
Device capture requires an already booted/authorized device and installed platform tools.
Sharing bundles requires manual inspection of the design and screenshot content.
