import Foundation
import Testing
import SupercliShared
@testable import SupercliNative

struct PluginSettingsListTests {
    @Test func appQuickAccessUsesOneChipWithAllVariantsInHostOrder() {
        let app = RemoteAppSummary(id: "supercli.app.markdown", name: "Markdown", command: "supercli-markdown", installed: true)
        let presets = [
            Preset(id: "markdown", label: "Markdown", command: "supercli-markdown", enabled: true, quickLaunch: true),
            Preset(id: "notes", label: "Notes", command: "supercli-markdown notes.md", enabled: true, quickLaunch: false),
            Preset(id: "claude", label: "Claude", command: "claude", enabled: true, quickLaunch: true),
        ]
        let groups = collectQuickPresetGroups(presets, apps: [app])
        #expect(groups.map(\.id) == ["supercli.app.markdown", "claude"])
        #expect(groups[0].presets.map(\.id) == ["markdown", "notes"])
        #expect(groups[0].displayName == "Markdown")
        // Without the App in the Host inventory the starred command is still
        // a chip — a custom one, identified by its leader preset.
        let withoutApp = collectQuickPresetGroups(presets, apps: [])
        #expect(withoutApp.map(\.id) == ["markdown", "claude"])
        #expect(withoutApp[0].cli == nil && withoutApp[0].app == nil)
    }

    @Test func newSessionMenuListsAgentsFirstAndPluginsInTheirOwnSection() {
        SupercliAppIconCatalog.update([
            RemoteAppSummary(id: "supercli.app.markdown", name: "Markdown", command: "supercli-markdown", installed: true),
        ])
        defer { SupercliAppIconCatalog.update([]) }
        let presets = [
            Preset(id: "markdown", label: "Markdown", command: "supercli-markdown", enabled: true, quickLaunch: true),
            Preset(id: "codex", label: "codex --yolo", command: "codex --yolo", enabled: true, quickLaunch: true),
            Preset(id: "dev", label: "Dev server", command: "./scripts/dev.sh", enabled: true, quickLaunch: false),
            Preset(id: "notes", label: "Notes", command: "/opt/bin/supercli-markdown notes.md", enabled: true, quickLaunch: false),
        ]
        let split = splitPresetsForNewSessionMenu(presets)
        #expect(split.agents.map(\.id) == ["codex", "dev"])
        #expect(split.plugins.map(\.id) == ["markdown", "notes"])
    }

    @Test func mixedRowsKeepVariantsTogetherAndIgnoreLegacyProjectOverrides() throws {
        let settings = try JSONDecoder().decode(RemoteWorkspaceSettings.self, from: Data(#"{"pluginOrder":["supercli.app.markdown","claude"],"availableAgents":[{"id":"claude","name":"Claude","command":"claude","installed":true}],"autoStopArchiveMinutes":120,"sidebarStoppedLimit":5,"browserDefaultAccess":"on","mcpNonchildWriteAccess":"ask","computerAccess":"ask","mcpWorktreeAccess":false,"mcpAutoAddBrowserScreenshots":true}"#.utf8))
        let snapshot = RemoteBootstrapSnapshot(
            macID: "remote", macName: "Remote", folders: [], projects: [],
            presets: [
                .init(id: "base", label: "Project override", command: "ignored", projectID: "project"),
                .init(id: "base", label: "Claude", command: "claude", pluginID: "claude"),
                .init(id: "variant", label: "Plan", command: "claude --plan", pluginID: "claude"),
                .init(id: "markdown", label: "Markdown", command: "supercli-markdown", pluginID: "supercli.app.markdown"),
            ],
            workspaceSettings: settings,
            availableApps: [.init(id: "supercli.app.markdown", name: "Markdown", command: "supercli-markdown", installed: true)],
            sessions: [], capturedAtUnixMs: 1
        )
        let items = PluginSettingsList.items(in: snapshot)
        #expect(items.map(\.id) == ["supercli.app.markdown", "claude"])
        #expect(items[1].commands.map(\.command) == ["claude", "claude --plan"])
    }

    @Test func filteredDragPreservesHiddenAndInactiveSlots() {
        #expect(PluginSettingsList.merging(["codex", "claude"], into: ["claude", "hidden", "codex", "inactive"])
                == ["codex", "hidden", "claude", "inactive"])
    }

    @Test func draggingTallCardOpensItsFullHeightBetweenShortRows() {
        // A three-command card is 112pt high. Its 16pt gap moves along with it.
        let ids = ["claude", "codex", "markdown", "git"]
        #expect(PluginListDragController.reordered(ids, source: 0, target: 2) == ["codex", "markdown", "claude", "git"])
        #expect(PluginListDragController.slotOffset(index: 1, source: 0, target: 2, stride: 128) == -128)
        #expect(PluginListDragController.slotOffset(index: 2, source: 0, target: 2, stride: 128) == -128)
        #expect(PluginListDragController.slotOffset(index: 3, source: 0, target: 2, stride: 128) == 0)
        #expect(PluginListDragController.slotOffset(index: 1, source: 0, target: 0, stride: 128) == 0)
    }
}
