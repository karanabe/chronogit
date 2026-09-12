# Documentation agent guide

This directory is the Astro Starlight documentation site for ChronoGit. Keep the result fast, readable, accessible, and easy for one maintainer to understand.

## Sources of truth

- `astro.config.mjs` owns project metadata, locales, navigation, and code-block themes.
- `src/styles/theme.css` owns the configurable project color.
- `src/styles/site.css` owns typography, spacing, navigation states, and landing-page presentation.
- `src/content/docs/` owns English content at the site root.
- `src/content/docs/ja/` owns Japanese content under `/ja/`.

Do not edit generated files in `dist/` or `.astro/`, and do not patch files in `node_modules/`.

## Development workflow

Install dependencies with `pnpm install` and run the production check with `pnpm build`.

Always start the development server in background mode:

```sh
pnpm dev
```

Manage it without starting a second server:

```sh
pnpm dev:status
pnpm dev:logs
pnpm dev:stop
```

Consult the relevant official documentation before changing framework behavior:

- [Astro documentation](https://docs.astro.build/)
- [Astro routing](https://docs.astro.build/en/guides/routing/)
- [Astro components](https://docs.astro.build/en/basics/astro-components/)
- [Astro content collections](https://docs.astro.build/en/guides/content-collections/)
- [Astro styling](https://docs.astro.build/en/guides/styling/)
- [Astro internationalization](https://docs.astro.build/en/guides/internationalization/)
- [Starlight documentation](https://starlight.astro.build/)

## Content rules

- Treat English as the primary language and keep a Japanese page at the matching relative path. For example, `guides/install.md` pairs with `ja/guides/install.md`.
- Update both languages when changing meaning, commands, links, frontmatter, or page structure. Do not leave a translation that silently describes old behavior.
- Give every page a specific `title` and a concise `description`. Use `sidebar.order` only when the intended order is meaningful.
- Write task-oriented guides around user outcomes. Put exhaustive fields, options, and defaults in reference pages.
- Keep paragraphs direct, headings descriptive, and links meaningful out of context. Avoid filler, unexplained jargon, and claims that the repository cannot verify.
- Make code samples safe to copy. Clearly identify placeholders, credentials, platform assumptions, and destructive commands.
- Prefer Markdown. Use MDX only when a Starlight or Astro component materially improves the explanation.

## Design and accessibility rules

- Preserve semantic heading order and use native HTML elements for controls and links.
- Give interactive elements an accessible name and keep keyboard focus visible.
- Do not encode meaning with color alone. Check both light and dark modes and narrow screens.
- Keep fonts local to the operating system; do not add web-font downloads without an explicit requirement.
- Change the project accent through `--project-accent-hue` in `src/styles/theme.css`. Do not scatter project colors across component styles.
- Keep code blocks on the configured Slack Ochin and Tokyo Night themes unless the project requirements explicitly change.

## Completion checklist

Before finishing a documentation change:

1. Confirm English and Japanese routes have matching content and working internal links.
2. Run `pnpm build`.
3. Check that `/`, `/ja/`, the changed English page, and its Japanese counterpart are generated.
4. For visual changes, inspect desktop and mobile layouts in both color modes.
5. Update `README.md` when the adoption workflow, commands, structure, or configuration changes.
