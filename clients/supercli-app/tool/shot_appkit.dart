/// Screenshot proof: renders the Swift app-kit Usage page fixture
/// (from `ProtocolTests.richListItemsDecodeBandsMediaDividersAndBusyFooters`)
/// through [AppKitRenderer] in a real 1440x900 gpuidart window.
///
/// Usage: xvfb-run -a dart run tool/shot_appkit.dart
/// Then capture with: python3 tool/capture_shot.py /path/to/shot.png
library;

import 'package:gpuidart/gpuidart.dart';
import 'package:supercli_app/widgets/appkit_protocol.dart';
import 'package:supercli_app/widgets/appkit_renderer.dart';

/// Verbatim JSON from the Swift test.
const _usagePageJson = '''
{"id":"usage-page","type":"page","title":"Usage","body":{"type":"list","id":"rows",
"rowLayout":{"type":"auto","stackBelowWidth":60},
"items":[
  {"id":"sep","label":"Providers","divider":true},
  {"id":"codex","label":"Codex","value":"42% left",
   "trailing":{"type":"gauge","id":"slot","ratio":0.42,"label":"Quota","accessibilityText":"42 percent left"},
   "bottom":{"type":"gauge","id":"band","ratio":0.42,"label":"Quota","accessibilityText":"42 percent left"},
   "top":{"type":"text","id":"top","text":"Shipped","tone":"success"},
   "media":{"side":"trailing","width":4,"glyph":"CX","tone":"info"}},
  {"id":"claude","label":"Claude","value":"78% left",
   "trailing":{"type":"gauge","id":"slot2","ratio":0.78,"label":"Quota","accessibilityText":"78 percent left"}}
]},
"footer":{"actions":[{"id":"refresh","label":"refreshing…","action":"refresh","busy":true,"disabled":true}]}}''';

Future<void> main() async {
  final node = appKitNodeFromJsonString(_usagePageJson);
  final tree = AppKitRenderer.render(node);

  await GpuiHost.open(
    tree,
    window: const GpuiWindowOptions(
      title: 'supercli — appkit Usage page (Swift fixture)',
      width: 1440,
      height: 900,
    ),
  );

  // Keep the window open for the screenshot capture.
  await Future<void>.delayed(const Duration(seconds: 30));
}
