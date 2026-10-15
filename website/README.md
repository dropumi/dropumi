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

The site is served at the root, `https://dropumi.github.io/`, so it lives in its own repo named `dropumi.github.io` in the `dropumi` org. A repo with that exact name is what gives GitHub Pages the root address.

`.github/workflows/deploy.yml` builds and publishes on every push to `main`. In that repo, set **Settings > Pages > Source** to **GitHub Actions**.

### Moving the site out of the main repo

From the `dropumi` repo, with the history of `website/` kept:

```sh
git subtree split --prefix=website -b site-only
git push git@github.com:dropumi/dropumi.github.io.git site-only:main
```

After the first deploy works, remove `website/` and `.github/workflows/deploy-site.yml` from the main repo. While the site is still in the main repo, that workflow publishes it under `/dropumi/` instead.
