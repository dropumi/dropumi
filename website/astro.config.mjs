// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const base = process.env.ASTRO_BASE ?? '/';

export default defineConfig({
	site: process.env.ASTRO_SITE ?? 'https://dropumi.github.io',
	base,
	integrations: [
		starlight({
			title: 'dropumi',
			description: 'Free, open-source, ultra-lightweight local file sharing. No internet required.',
			logo: {
				light: './src/assets/logo-light.svg',
				dark: './src/assets/logo-dark.svg',
				replacesTitle: true,
			},
			favicon: '/favicon.svg',
			customCss: ['./src/styles/theme.css'],
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/dropumi/dropumi' }],
			editLink: { baseUrl: process.env.ASTRO_EDIT_URL ?? 'https://github.com/dropumi/dropumi.github.io/edit/main/' },
			sidebar: [
				{ label: 'Getting started', items: ['docs', 'docs/install', 'docs/send-a-file', 'docs/receive-a-file'] },
				{ label: 'Using dropumi', items: ['docs/visibility', 'docs/transfers', 'docs/settings'] },
				{ label: 'Help', items: ['docs/troubleshooting', 'docs/faq', 'docs/contributing'] },
			],
		}),
	],
});
