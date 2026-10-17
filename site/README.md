# Talaan website

A one-page landing page for Talaan: plain HTML and CSS with one tiny script, no framework and no build step.

**Not published.** Nothing deploys this folder. Where and when it goes live (GitHub Pages, a custom domain or
something else) is still to be decided.

## Preview it

From the repository root, either of these works (nothing to install):

```bash
python -m http.server 1448 --directory site
# or
npx http-server site -p 1448
```

Then open http://localhost:1448/. Opening `site/index.html` straight from disk also works.

## What's in here

| File | What it is |
|---|---|
| `index.html` | The page: what Talaan is, how it works, privacy, downloads, the unsigned-app note, questions. |
| `styles.css` | Styles. Colours and fonts follow the app's "Sinag" tokens in `src/styles/tokens.css`. Light and dark follow the visitor's system setting. |
| `main.js` | Highlights the download for the visitor's computer (Mac or Windows). The page works without it. |
| `assets/meeting-light.webp`, `assets/meeting-dark.webp` | Screenshots of the app's meeting screen (sample data), shown to match light or dark mode. |
| `assets/icon.svg` | The "ta" logo, used as the favicon and in the header and footer. |
| `assets/fonts/` | Syne, Instrument Sans and DM Mono, the same fonts the app ships, with their SIL Open Font License files. |

## Rules for this page

- No trackers, analytics, cookies, external fonts or CDNs. Every file is served from this folder.
- Plain English for people who have never heard of speech models. No em dashes.
- Facts must match `README.md` and `docs/INSTALL.md` (requirements, install steps, privacy). Update the page when they
  change.
- Download buttons point at https://github.com/mmnzns/Talaan/releases/latest.

## Updating the screenshots

They come from the app's browser preview with sample data (`npm run dev`, then open `#/meeting/m-q4`), captured at
1280x800 at 2x and saved as 1600x1000 WebP. Keep each file under about 400 KB.
