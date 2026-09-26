# Web links URL contract

Canonical help / docs / download URLs referenced from the supercli apps.
The apps must reference these through `HelpLinks` /
`HelpLinksView` (`clients/supercli-app/lib/web/help_links.dart`) — never by
pasting URLs into screens.

## Contract

| Slug           | URL                                          | Served by                        | Status      |
|----------------|----------------------------------------------|----------------------------------|-------------|
| `docs`         | `https://supercli.com/docs`                  | docs site                        | expected    |
| `download-mac` | `https://supercli.com/download/mac`          | release site (macOS app)         | expected    |
| `download-ios` | `https://supercli.com/download/ios`          | release site (iOS app)          | expected    |
| `install-cli`  | `https://supercli.com/install.sh`            | supercli-release-updates worker  | implemented |
| `install-app`  | `https://supercli.com/install/<app>/install.sh` | supercli-release-updates worker | implemented |

`implemented` = the endpoint is served today (`scripts/install.sh`,
`scripts/install-app.sh`; checklist #148). `expected` = the URL contract
the apps code against; the hosted pages are owned by the release
infrastructure, not this repo. Live verification from this sandbox was not
possible (no outbound network), so `expected` rows are contract, not proof
of a live page.

## In-app surface

`HelpLinksView.build()` returns a `UiNode` tree (column of labelled link
buttons, ids `help-link-<slug>`) that embeds anywhere: Settings → General,
a Help menu, or an About panel. The host resolves a pressed button with
`HelpLinks.urlForButtonId(id)` and opens the URL externally. Screens must
not hard-code marketing URLs.

Suggested embed points (for the settings/help workers):
- `SettingsView` General tab → `HelpLinksView().build()` section.
- Desktop Help menu → one menu item per `HelpLinks.all` entry.

## Notes

- The checklist row historically named the old product domain; the product
  domain is now `supercli.com` (rename complete). Old-domain URLs are not
  part of the contract.
- Channel selection (`alpha`/`beta`/`stable`) applies to the installer
  endpoints via `SUPERCLI_CHANNEL`; docs/download pages are channel-free.
