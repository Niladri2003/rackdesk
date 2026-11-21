# RackDesk landing page

The public page for RackDesk. It is an [Astro](https://astro.build) project that builds to plain files. Netlify publishes them.

## Run it
You need Node 22.23.4 or newer. Astro does not start on an older version.
```sh
cd site
npm install
npm run dev        # http://127.0.0.1:8100
```
From the repo root, `make site` does the same.

## Build it
```sh
npm run build      # writes dist/ and checks it
npm run preview    # serves dist/ on http://127.0.0.1:8100
```
The build fails if a built page has an inline script or style. The security header of the site blocks those. Stop the preview server with `npx astro preview stop`.

## Deploy it
Netlify reads `netlify.toml` in the repo root: base folder `site`, command `npm run build`, publish folder `dist`. Connect the repository in Netlify and it builds on every push. The same file sets the security headers and the cache times.

## Change it
| To change | Edit |
|---|---|
| Text of a section | The matching file in `src/components/` |
| Colours, spacing, animation | `src/styles/site.css` |
| The install command or the repository URL | `src/site.ts` |
| The start sequence at the top of the page | `src/scripts/main.js` |
| Page title and description | `src/pages/index.astro` |

The screenshots in `src/assets/shots/` are real pictures of RackDesk. `CLAUDE.md` in this folder lists what else must change when they are retaken, and the rules the page follows.

## Licences
The fonts in `public/fonts/` are under the SIL Open Font License. The licence texts are next to them.
