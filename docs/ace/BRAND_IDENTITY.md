# LOLA brand identity — canonical assets

The project owner has corrected the earlier asset identification. The authoritative LOLA identity is the original **White Pearl** and **Obsidian Blue** brand artwork, including the genuine mascot and logo.

## Canonical identity

- **White Pearl** — official original LOLA brand artwork.
- **Obsidian Blue** — official original LOLA brand artwork.

These names, and the original supplied master artwork for each, supersede the image previews that were first identified in this conversation. The earlier previews are not authoritative and must not be used as the source for app icons, splash screens, onboarding or marketing assets.

Do not infer a logo/mascot role from the colour name alone. Preserve the exact source-to-role mapping supplied with the canonical original files. Do not redraw, reinterpret, recolour, crop destructively, or generate substitutes.

## Android usage

Once the original master files are added to the repository:

- Use the canonical logo master for the application mark and launcher/adaptive icon.
- Use the canonical mascot master for welcome, onboarding and selected empty-state surfaces.
- Keep the chat transcript compact and readable; do not put mascot artwork in every message row.
- Do not animate either master unless an approved animation asset is supplied.
- Preserve the original aspect ratio, transparency, colour profile and source dimensions. Generate density-specific derivatives only from the masters and document them.

## Source asset handling

The original White Pearl and Obsidian Blue binary masters have not yet been committed to the repository. The previously supplied image previews must not be substituted.

When the canonical source files are available, preserve untouched masters under:

- `brand/white-pearl.<original-extension>`
- `brand/obsidian-blue.<original-extension>`

Add Android-optimised derivatives under `android/app/src/main/res/` while retaining the masters. Before conversion, record dimensions, alpha channel, colour profile, file format and the supplied logo/mascot role.

Until the canonical original files are present, keep the current Android brand surfaces neutral and do not embed approximations.
