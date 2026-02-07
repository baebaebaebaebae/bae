// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://bae.fm',
	integrations: [
		starlight({
			title: 'bae',
			description: 'Album-oriented desktop music player',
			social: [
				{ icon: 'github', label: 'GitHub', href: 'https://github.com/bae-fm/bae' }
			],
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Getting Started',
					items: [
						{ label: 'Installation', slug: 'getting-started/installation' },
						{ label: 'Quick Start', slug: 'getting-started/quick-start' },
					],
				},
				{
					label: 'Importing Music',
					items: [
						{ label: 'Local Files', slug: 'importing/local-files' },
					],
				},
				{
					label: 'Storage',
					items: [
						{ label: 'Overview', slug: 'storage/overview' },
						{ label: 'Profiles', slug: 'storage/profiles' },
					],
				},
				{
					label: 'Library',
					items: [
						{ label: 'Browsing', slug: 'library/browsing' },
						{ label: 'Metadata', slug: 'library/metadata' },
					],
				},
			],
		}),
	],
});
