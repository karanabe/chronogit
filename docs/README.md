# ChronoGit documentation site

This directory contains the Astro Starlight site for ChronoGit's English and Japanese user and developer documentation.

The private package exists only to build the site locally and is not published to npm. The generated `dist/` directory may later be deployed to GitHub Pages or Azure after the canonical site URL and hosting target are selected.

## Work locally

Requirements:

- a Node.js release supported by the locked Astro version;
- [pnpm](https://pnpm.io/).

Install dependencies and start the background development server:

```sh
pnpm install --frozen-lockfile
pnpm dev
```

| Command | Purpose |
| --- | --- |
| `pnpm dev` | Start the Astro server in background mode |
| `pnpm dev:status` | Show the background server status |
| `pnpm dev:logs` | Read server logs |
| `pnpm dev:stop` | Stop the background server |
| `pnpm build` | Build all pages and Pagefind indexes into `dist/` |
| `pnpm preview` | Preview an existing production build |

## Content layout

English is the root locale. Japanese pages use `/ja/` and mirror the same relative paths:

```text
src/content/docs/
├── index.mdx
├── guides/                 # Installation and user workflows
├── reference/              # CLI, safety, limits, and non-goals
├── troubleshooting/        # Failure diagnosis and recovery
├── developer/              # Architecture, validation, smoke, release
├── tags.mdx
└── ja/
    ├── index.mdx
    ├── guides/
    ├── reference/
    ├── troubleshooting/
    ├── developer/
    └── tags.mdx
```

When changing behavior, update both language versions in the same change. Keep their file paths and `sidebar.order` values aligned so Starlight can connect translations and present matching navigation.

Every content page has a search description and locale-specific tags. The `/tags/` and `/ja/tags/` explorers intentionally keep the two languages separate.

## Site configuration

`astro.config.mjs` owns the ChronoGit title, locales, sidebar groups, code theme, and optional publication metadata. `src/content.config.ts` extends Starlight frontmatter with locale-scoped tags.

The repository and site origins are intentionally unset because their canonical public URLs have not been selected. Supply `PUBLIC_REPOSITORY_URL` to enable the GitHub and edit-page links. Supply `PUBLIC_SITE_URL` before publishing so canonical metadata and sitemap URLs are correct. Do not insert a placeholder or assume a hosting provider.

Theme tokens live in `src/styles/theme.css`; layout and landing-page rules live in `src/styles/site.css`. The custom components provide navigation, metadata, and the locale-scoped tag explorer.

## Validate

Run the production build after every content, configuration, component, or styling change:

```sh
pnpm build
```

Confirm that every English route has its Japanese counterpart, internal links resolve, Pagefind indexes both locales, and no starter names or placeholder URLs remain. Styling changes also need desktop/mobile and light/dark inspection with keyboard focus.

The project is licensed under either Apache-2.0 or MIT; see `../LICENSE-APACHE` and `../LICENSE-MIT`.
