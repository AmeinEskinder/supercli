# Web links URL contract

Canonical help / docs / download URLs referenced from the supercli apps.
The apps must reference these through `HelpLinks` /
`HelpLinksView` (`clients/supercli-app/lib/web/help_links.dart`) — never by
pasting URLs into screens.

## Contract

| Slug           | URL                                          | Served by                        | Status      |
|----------------|----------------------------------------------|----------------------------------|-------------|
| `docs`         | `https://superc.li/docs`                     | docs site                        | expected    |
| `download-mac` | `https://superc.li/download/mac`             | release site (macOS app)         | expected    |
| `download-ios` | `https://superc.li/download/ios`             | release site (iOS app)           | expected    |
| `install-cli`  | `https://superc.li/install.sh`               | supercli-release-updates worker  | implemented |
| `install-app`  | `https://superc.li/install/<app>/install.sh` | supercli-release-updates worker  | implemented |

`implemented` = the endpoint is served today (`scripts/install.sh`,
`scripts/install-app.sh`; checklist #148). `expected` = the URL contract
the apps code against; the hosted pages are owned by the release
infrastructure, not this repo. Live verification from this sandbox was not
possible (no outbound network), so `expected` rows are contract, not proof
of a live page.
