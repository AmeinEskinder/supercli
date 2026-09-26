/// Help / docs / download link surface for the supercli apps.
///
/// Canonical URL contract (see `docs/web-links.md`):
///
/// - Docs:            https://superc.li/docs
/// - macOS download:  https://superc.li/download/mac
/// - iOS download:    https://superc.li/download/ios
/// - CLI installer:   https://superc.li/install.sh
/// - Per-app install: `https://superc.li/install/<app>/install.sh`
///
/// [HelpLinksView.build] returns a [UiNode] tree (a column of buttons)
/// that any screen — Settings → General, a Help menu, an About panel —
/// can embed. Buttons are named `help-link-<slug>`; the host resolves the
/// URL with [HelpLinks.urlForButtonId]. Keeping the mapping in one place
/// means the apps never hard-code marketing URLs in screens.
library;

import 'package:gpuidart/gpuidart.dart';

/// A single help/docs/download link.
final class HelpLink {
  const HelpLink({
    required this.slug,
    required this.label,
    required this.url,
    required this.description,
  });

  final String slug;
  final String label;
  final String url;
  final String description;
}

/// The canonical link list. Slugs are stable; screens must reference links
/// by slug, never by pasting URLs.
final class HelpLinks {
  const HelpLinks._();

  static const docs = HelpLink(
    slug: 'docs',
    label: 'Documentation',
    url: 'https://superc.li/docs',
    description: 'Guides, CLI reference and troubleshooting.',
  );

  static const downloadMac = HelpLink(
    slug: 'download-mac',
    label: 'Download for macOS',
    url: 'https://superc.li/download/mac',
    description: 'The supercli desktop app for macOS.',
  );

  static const downloadIos = HelpLink(
    slug: 'download-ios',
    label: 'Download for iOS',
    url: 'https://superc.li/download/ios',
    description: 'The supercli companion app for iOS.',
  );

  static const installCli = HelpLink(
    slug: 'install-cli',
    label: 'Install the CLI',
    url: 'https://superc.li/install.sh',
    description: 'One-line installer for the supercli CLI.',
  );

  static const List<HelpLink> all = [docs, downloadMac, downloadIos, installCli];

  static String? urlForSlug(String slug) {
    for (final link in all) {
      if (link.slug == slug) return link.url;
    }
    return null;
  }

  /// Maps a button node id produced by [HelpLinksView] back to its URL.
  static String? urlForButtonId(String buttonId) {
    const prefix = 'help-link-';
    if (!buttonId.startsWith(prefix)) return null;
    return urlForSlug(buttonId.substring(prefix.length));
  }
}

/// Builds the embeddable "Help & downloads" section as a UiNode tree.
final class HelpLinksView {
  const HelpLinksView({this.links = HelpLinks.all});

  final List<HelpLink> links;

  UiNode build() {
    return UiColumn('help-links', [
      const UiText('help-links-title', 'Help & downloads'),
      for (final link in links)
        UiRow('help-link-row-${link.slug}', [
          UiButton('help-link-${link.slug}', link.label),
          UiText('help-link-desc-${link.slug}', link.description),
        ]),
    ]);
  }
}
