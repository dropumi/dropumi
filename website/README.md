# dropumi website

Landing page, docs, license and privacy pages for [dropumi](https://github.com/dropumi/dropumi), built with [Astro](https://astro.build) and [Starlight](https://starlight.astro.build).

```sh
pnpm install
pnpm dev          # http://localhost:4321
pnpm build        # output in dist/
```

- `src/pages/` landing page, license and privacy
- `src/content/docs/docs/` docs, one Markdown file per page (add new pages to the sidebar in `astro.config.mjs`)
- `src/styles/theme.css` colors and font, from `design.md`
- `public/images/` logo and screenshots, copied from `branding/`

## Deploying

`.github/workflows/deploy-site.yml` publishes to GitHub Pages on every push to `main` that touches `website/`. In the repo, set **Settings > Pages > Source** to **GitHub Actions**.

The site is built with `ASTRO_BASE=/dropumi` because it is served from the `dropumi` repo (`dropumi.github.io/dropumi/`). To serve from the root, move `website/` to a repo named `dropumi.github.io` in the `dropumi` org and drop `ASTRO_BASE`.
