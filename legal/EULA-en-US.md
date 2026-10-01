<!-- PLACEHOLDER-NOT-FOR-RELEASE -->

# End User License Agreement (EULA)

> **This document is not final.** `scripts/sync-assets.mjs` detects the placeholder
> marker above, refuses to upload this file, and warns on the console. That is also
> exactly what triggers the client's "cannot fetch the terms" branch: the UI only
> shows "Please accept the End User License Agreement (EULA)" with a link to the
> official website.
>
> **To replace**: overwrite this file with the real terms and **delete the placeholder
> marker on the first line**, then run `pnpm sync:assets`. If the terms changed in a
> way that requires renewed consent, bump `EULA_VERSION` at the top of
> `scripts/sync-assets.mjs` (a version change makes every user agree again).

## 1. Parties

(to be written)

## 2. Scope of license

(to be written)

## 3. User obligations

(to be written)

## 4. Disclaimer

(to be written)

## 5. Changes to this agreement

When the terms change, the version in `legal/manifest.json` changes with them. The
client asks for renewed consent during onboarding; afterwards it only shows a notice.
