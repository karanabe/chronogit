// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const project = {
	title: 'ChronoGit',
	description: 'A read-only terminal UI for exploring Git history and diffs.',
};
const repository = process.env.PUBLIC_REPOSITORY_URL?.replace(/\/$/, '');
const site = process.env.PUBLIC_SITE_URL;

// https://astro.build/config
export default defineConfig({
	...(site ? { site } : {}),
	integrations: [
		starlight({
			title: {
				en: project.title,
				ja: project.title,
			},
			description: project.description,
			locales: {
				root: { label: 'English', lang: 'en' },
				ja: { label: '日本語', lang: 'ja' },
			},
			...(repository
				? {
						social: [{ icon: 'github', label: 'GitHub', href: repository }],
						editLink: { baseUrl: `${repository}/edit/master/` },
					}
				: {}),
			customCss: ['./src/styles/theme.css', './src/styles/site.css'],
			components: {
				Head: './src/components/MetadataHead.astro',
				SiteTitle: './src/components/SiteNavigation.astro',
			},
			expressiveCode: {
				// Slack Ochin is the light theme; Tokyo Night is the dark theme.
				themes: ['slack-ochin', 'tokyo-night'],
				useStarlightUiThemeColors: true,
				styleOverrides: { borderRadius: '0.75rem' },
			},
			lastUpdated: false,
			sidebar: [
				{
					label: 'Use ChronoGit',
					translations: { ja: 'ChronoGitを使う' },
					items: [{ autogenerate: { directory: 'guides' } }],
				},
				{
					label: 'Reference',
					translations: { ja: 'リファレンス' },
					items: [{ autogenerate: { directory: 'reference' } }],
				},
				{
					label: 'Troubleshooting',
					translations: { ja: 'トラブルシューティング' },
					items: [{ autogenerate: { directory: 'troubleshooting' } }],
				},
				{
					label: 'Development',
					translations: { ja: '開発' },
					items: [{ autogenerate: { directory: 'developer' } }],
				},
			],
		}),
	],
});
