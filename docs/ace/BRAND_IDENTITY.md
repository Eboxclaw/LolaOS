# LOLA brand identity — official assets

The project owner has confirmed the canonical identity and its exact role mapping:

- **White Pearl** — the official LOLA mascot: the white/pearl cyber-cat character with blue luminous details.
- **Obsidian Blue** — the official LOLA logo: the blue/cyan paw emblem.

These two uploaded images are the originals. They supersede the earlier purple previews, which are not part of the official identity and must not be used.

## Original master files

The supplied masters are unchanged PNG files, both 1254 × 1254 pixels, RGBA with a fully opaque alpha channel (alpha = 255 throughout) and a black background.

| Role | Original filename | SHA-256 |
|---|---|---|
| White Pearl mascot | `Neon Pearl Cyber Cat Mascot(1).png` | `73b5d9e12a750a45179e84dddf3cd26b529cd3562f02db7fdd1fefee1e4282b1` |
| Obsidian Blue logo | `Futuristic Neon Paw Emblem(1).png` | `44e667b99e8104640638ec601707c76592d8342464f421045e749060ae796a92` |

## Android display assets

An asset pack has been prepared from these exact masters. It contains the original PNGs plus optimised WebP display derivatives:

- `android/drawable-nodpi/lola_white_pearl_mascot.webp` — 512 × 512, use in welcome/onboarding and selected empty states.
- `android/drawable-nodpi/lola_obsidian_blue_logo.webp` — 384 × 384, use for the app mark and launcher/adaptive icon artwork.

The originals remain the source of truth. WebP derivatives are app delivery formats only; do not edit the master PNGs.

## Usage rules

- Never use the earlier purple mascot or purple paw preview.
- Never swap the two roles: White Pearl is the mascot; Obsidian Blue is the logo.
- Do not redraw, recolour, re-style, crop destructively, or stretch either official asset.
- Preserve the square aspect ratio and black background. The supplied masters are not transparent despite their RGBA pixel format.
- Use the mascot sparingly for welcoming and empty-state moments; keep the chat transcript compact.
- Use the logo as the primary compact brand mark and launcher artwork.
- Do not animate either master unless a separate animation asset is supplied and approved.

## Repository integration

The binary originals and Android derivatives are supplied together in the **LOLA Official Brand Assets** archive. The archive includes a manifest with dimensions and checksums. Copy its `masters/` contents to `brand/` and its WebP files to `android/app/src/main/res/drawable-nodpi/` when committing the binary assets to the repository.

Until those binary files are committed, do not add unresolved Android drawable references or generate substitutes.
