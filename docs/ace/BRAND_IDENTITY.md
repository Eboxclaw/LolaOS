# LOLA brand identity

The project owner has designated two supplied image assets as the official LOLA identity:

1. **LOLA logo** — the primary brand mark.
2. **LOLA mascot** — the character/mascot artwork.

These are authoritative project assets, not inspiration references. Do not redraw, reinterpret, recolour, crop destructively or generate substitutes. Preserve the original source files and their transparency/aspect ratio.

## Intended Android use

- Use the **logo** for the app's brand mark, launcher/adaptive icon and compact identity placements.
- Use the **mascot** for the welcome/empty-conversation state, first-run onboarding and selected friendly status surfaces.
- Keep the chat itself readable and low-latency; the mascot must not consume space in every message row.
- Do not use the mascot as a loading animation until a separate animation asset is supplied or approved.
- Do not stretch either asset. Derive density-specific or cropped variants only from the original master files and document each derivative.

## Source asset handling

The images were supplied as visual previews in the conversation. The original binary files were not exposed to the repository integration, so they are **not yet committed**.

When the source files are available, keep untouched masters in:

- `brand/lola-logo.<original-extension>`
- `brand/lola-mascot.<original-extension>`

Then add Android-optimised derivatives under `android/app/src/main/res/` while retaining the masters. Confirm the source dimensions, alpha channel, colour profile and licence/ownership before conversion.

Until then, the app must not embed approximations or generated substitutes.
